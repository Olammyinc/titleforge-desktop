# TitleForge — Full Project Context

> **Last updated:** 2026-07-15
> **Repos:** `github.com/Olammyinc/titleforge` (web) · `github.com/Olammyinc/titleforge-desktop` (desktop)

---

## 1. Project Overview

**TitleForge** is an AI-powered title generator for creators — generates titles for books, articles, YouTube videos, songs, podcasts, newsletters, speeches, product names, character names, children's names, and more. Two products:

| | Web App | Desktop App |
|---|---|---|
| **Deployment** | Netlify (free tier) | Tauri v2 native binary |
| **Pricing** | Free tier + $15.83/mo annual Pro ($19/mo monthly) | $29 Basic / $49 Pro one-time |
| **AI** | Serverless via Netlify Functions (DeepSeek V4 Flash default, configurable) | Bring-your-own-key (OpenAI, DeepSeek, Claude, Gemini) + offline engine |
| **Database** | Supabase Postgres (6 tables) | Local SQLite (`titles.db`) |
| **Auth** | Supabase Auth (CDN + localStorage fallback) | License key activation (24h offline cache) |
| **Platforms** | Any browser | Windows (.exe NSIS), macOS (.dmg), Linux (.deb, .AppImage) |

### Brand Identity
- **Name:** Editorial Industrial — warm, typography-first, industrial craft
- **Palette:** Ink `#0B0A0A`, Paper `#F9F7F2`, Forge `#E8782B`, Forge Glow `#FF9147`, Forge Cool `#5B7B8A`, Success `#5C8A67`
- **Fonts:** Clash Display (headings) + Satoshi (body) via Fontshare CDN, with strong serif/sans-serif fallbacks
- **Logo:** SVG anvil + forge spark in amber gradient (`#E8782B` → `#D45C1A`)
- **Target audience:** Authors, YouTubers, bloggers, podcasters, marketers, songwriters, product naming — anyone who publishes content

---

## 2. Web App (`titleforge/`)

### 2.1 Tech Stack
- **Hosting:** Netlify (free tier, drag-and-drop or git-connected)
- **Frontend:** Vanilla HTML/CSS/JS — no framework
- **Backend:** 7 Netlify Functions (serverless Node.js with `node-fetch`)
- **AI Provider:** DeepSeek V4 Flash (`deepseek-v4-flash`) — configurable to OpenAI, Anthropic, Flux Router via `AI_PROVIDER` env var
- **Auth:** Supabase Auth (CDN: `@supabase/supabase-js@2`) + localStorage fallback (`titleforge_auth` key)
- **Database:** Supabase Postgres — 6 tables with Row Level Security
- **Payments:** Stripe Payment Links + Customer Portal, webhook upgrades `user_metadata.isPro`

### 2.2 Key Files

| File | Lines | Purpose |
|---|---|---|
| `index.html` | 663 | Landing page: hero, benefits, comparison, pricing (web + desktop), testimonials, FAQ, auth/waitlist/exit modals, sticky CTA |
| `app.js` | 2826 | All UI logic: auth, generation, results display, floating generator, dashboard rendering, settings, license management, export, projects |
| `styles.css` | 3003 | Full stylesheet: design system (CSS variables), nav, hero, benefits, why section, comparison strip, pricing, FAQ, tool section, results, cross-medium, floating generator, dashboard, responsive breakpoints |
| `dashboard.html` | 134 | Dashboard shell: 6 tabs (Overview, History, Favorites, Projects, Export, Settings) |
| `dashboard.js` | 85 | Dashboard page init: auth check from localStorage, Stripe redirect handler, tab wiring |
| `netlify.toml` | 12 | Netlify config: functions dir, redirects for `/api/generate` and `/api/validate-license` |
| `supabase-setup.sql` | 231 | Idempotent schema: 6 tables, RLS policies, RPC for atomic usage increment, indexes |
| `updates.json` | 23 | Desktop auto-updater manifest: v0.1.0, platform URLs (empty signatures — needs private key) |
| `logo.svg` | — | Vector logo: anvil + forge spark in amber |
| `seed-data.json` | 0.4MB | 480 templates + 475 word pool entries + 769 curated titles (same as desktop seed) |

### 2.3 Netlify Functions

| Function | Lines | Purpose | HTTP Methods |
|---|---|---|---|
| `config.js` | 24 | Returns public config: Supabase URL, anon key, Stripe links | GET |
| `generate.js` | 569 | AI title generation: multi-provider support (OpenAI/DeepSeek/Anthropic/Flux), 3 prompt modes (standard, cross-medium, name rubric), robust JSON repair (4 fallback layers), 7 fine-tune fields | POST |
| `licenses.js` | 193 | License CRUD: validate from desktop (public), generate for Pro users, deactivate, machine registration (max 3 devices) | GET, POST |
| `stripe-webhook.js` | 100 | Listens for `checkout.session.completed`, verifies Stripe signature, looks up user by email, sets `user_metadata.isPro = true` | POST |
| `usage.js` | 329 | Usage tracking + dashboard API: GET returns usage/history/favorites/projects; POST handles: increment (atomic RPC), save history, add/remove favorite, create/delete project, add to project, update title notes | GET, POST |
| `verify-subscription.js` | 76 | Checks Pro status via token, syncs usage table | GET |
| `waitlist.js` | 45 | Captures email signups to Supabase waitlist table | POST |

### 2.4 Database Schema (Supabase — 6 tables)

All tables have Row Level Security enabled with per-user policies.

**1. `usage`** — Daily usage tracking
- `id` UUID PK, `user_id` UUID FK→`auth.users`, `date` DATE, `count` INTEGER, `is_pro` BOOLEAN
- Unique constraint on `(user_id, date)`
- RPC `increment_usage(p_user_id, p_is_pro)` for atomic race-condition-free increments

**2. `title_history`** — Saved generation batches
- `id` UUID PK, `user_id` UUID FK, `keyword` TEXT, `categories` TEXT[], `genre` TEXT, `style` TEXT, `titles` JSONB

**3. `title_favorites`** — Starred/bookmarked titles
- `id` UUID PK, `user_id` UUID FK, `title` TEXT, `score` INTEGER, `keyword` TEXT, `category` TEXT

**4. `title_projects`** — Title collections
- `id` UUID PK, `user_id` UUID FK, `name` TEXT, `titles` JSONB

**5. `licenses`** — Desktop app license keys
- `id` UUID PK, `user_id` UUID FK, `license_key` TEXT UNIQUE, `tier` TEXT, `source` TEXT, `is_active` BOOLEAN, `activated_machines` TEXT[], `expires_at` TIMESTAMPTZ
- Key format: `TF-BASIC-XXXX-XXXX-XXXX-XXXX` or `TF-PRO-XXXX-XXXX-XXXX-XXXX`

**6. `waitlist`** — Desktop app waitlist signups
- `id` UUID PK, `email` TEXT UNIQUE, `source` TEXT

### 2.5 Auth Flow
1. Supabase CDN script loaded: `@supabase/supabase-js@2`
2. `tryInitSupabase()` fetches config from `/.netlify/functions/config` to get Supabase URL + anon key
3. If CDN fails to load (blocked by ad blockers, etc.), `localStorage` fallback reads `titleforge_auth` key
4. On successful auth: `onAuthSuccess()` persists `{email, token, isLoggedIn}` to localStorage, applies Pro UI
5. `onAuthRestoredFromStorage()` — cross-page auth (dashboard reads localStorage if Supabase CDN didn't load)
6. Guest mode always works: 3 generations, no signup, local-only tracking via `titleforge_guest_usage` localStorage key
7. Free tier: 5/day, requires account (authenticated Supabase user)

### 2.6 Payments
- **Stripe Payment Links** for Pro subscription (monthly `$19` or annual `$190`)
- **Customer Portal** for subscription management (cancellation)
- **Webhook flow:** `checkout.session.completed` → `stripe-webhook.js` → verify signature → find user by email → set `user_metadata.isPro = true`
- **Dashboard redirect:** After Stripe checkout, redirects to `dashboard.html?session_id=...` → `verifySubscription()` → checks `verify-subscription` function → refreshes page
- **Billing toggle:** Frontend shows monthly/yearly pricing with 17% annual discount

### 2.7 AI Generation (`generate.js`)
- **4 providers supported:** OpenAI (`gpt-4o-mini`), DeepSeek (`deepseek-v4-flash` — default), Anthropic (`claude-3-5-sonnet`), Flux Router (`flux-auto`)
- **3 prompt modes:**
  1. **Standard:** Categories as comma-separated list, generates title array with scores + breakdowns
  2. **Cross-medium:** Per-category adaptation with medium-specific conventions (YouTube ALL CAPS, books poetic, etc.)
  3. **Name rubric:** For `childname`, `character`, `street` categories — uniqueness, memorability, meaning depth, pronunciation, origin vibe
- **7 fine-tune fields:** audience, emotion, length, angle, mustInclude, avoid, beatTitle
- **JSON repair pipeline:** 4 fallback layers:
  1. Direct `JSON.parse`
  2. `repairJson()` — fixes non-ASCII quotes, spaces in property names, trailing commas, unquoted keys, comments, single quotes
  3. `repairTruncatedJson()` — closes truncated brackets and strings
  4. Last-good-position extraction — scans for `}}` boundaries and tries parsing substrings
- **`response_format: { type: "json_object" }`** used on OpenAI-compatible providers
- **Temperature:** 0.85, with `frequency_penalty: 0.6`, `presence_penalty: 0.4`

### 2.8 Frontend Features
| Feature | Guest | Free | Pro |
|---|---|---|---|
| Generations | 3 total | 5/day | Unlimited |
| Titles per batch | 10 | 10 | 100 |
| Categories | 5 | 5 | 16 |
| Styles | 4 | 4 | 9 |
| Fine-tune | No | No | Yes |
| Gender selector | No | No | Yes |
| Cross-medium | No | No | Yes |
| Subtitles | No | No | Yes |
| Translation | No | No | Yes (12 languages) |
| Score visible | Yes | Yes (teasered) | Full |
| Breakdown | PRO badges | PRO badges | Full values |
| Dashboard | No | Yes | Yes |
| Favorites | No | Yes | Yes |
| Projects | No | No | Yes |
| CSV Export | No | No | Yes |
| Desktop license | No | No | Basic included |

**Landing page sections:** Hero → Benefits → Why TitleForge (with comparison vs vidIQ/SEMrush) → Desktop App teaser → Tool section → Testimonials → Pricing (web + desktop tiers) → FAQ → Footer

**Floating generator:** Sticky FAB (⚡) available on all pages — opens modal with keyword input, category/style selectors, genre, quantity, cross-medium toggle, generates via same Netlify function.

**Dashboard tabs:** Overview (stats + recent activity + quick actions) → History (search/filter/sort, score badges, breakdown popups, favorites, project buttons) → Favorites (starred titles) → Projects (3-column responsive grid, inline notes, project picker dropdown) → Export (checkbox selection, CSV download, clipboard copy) → Settings (plan info, billing management, desktop license management)

**Exit intent modal:** Shows on mouseout (top of page) for non-logged-in users: "Before you go... get 3 free titles."

### 2.9 Deployment
- **Netlify env vars required:**
  ```
  SUPABASE_URL        — Supabase project URL
  SUPABASE_SERVICE_KEY — Supabase service_role key (for admin operations)
  SUPABASE_ANON_KEY    — Supabase anon/public key (for client-side init)
  DEEPSEEK_API_KEY     — DeepSeek API key (default AI provider)
  AI_PROVIDER          — "deepseek" (default), "openai", "anthropic", or "flux"
  STRIPE_SECRET_KEY    — Stripe secret key
  STRIPE_WEBHOOK_SECRET — Stripe webhook signing secret
  STRIPE_PRO_LINK      — Stripe Payment Link for Pro subscription
  STRIPE_PORTAL_LINK   — Stripe Customer Portal link
  STRIPE_SUCCESS_URL   — Redirect URL after successful payment
  ```
- **Deploy methods:** `npx netlify deploy --prod`, git push (if connected), or drag-and-drop

---

## 3. Desktop App (`titleforge-desktop/`)

### 3.1 Tech Stack
- **Framework:** Tauri v2 (Rust backend + webview frontend)
- **Frontend:** Vanilla HTML/CSS/JS (604 lines lighter than web app — 1427 vs 2826)
- **Rust crates:** `tauri 2`, `rusqlite 0.31` (bundled SQLite), `reqwest 0.12` (blocking HTTP), `serde/serde_json`, `rand 0.8`, `chrono 0.4`, `dirs 5`, `hostname 0.4`, `tauri-plugin-shell 2`, `tauri-plugin-updater 2`
- **Database:** Local SQLite via `rusqlite` with bundled compilation (no system SQLite needed)
- **Seed data:** 480 templates (30 per category × 16), 475 word pool entries, 769 curated titles
- **Build targets:** Windows (NSIS installer), macOS (.dmg), Linux (.deb + .AppImage)

### 3.2 Key Files

| File | Lines | Purpose |
|---|---|---|
| `src/index.html` | 293 | Main app page: compact hero, tool section with engine toggle, license activation overlay |
| `src/app.js` | 1427 | Desktop UI logic: license gate, generation (local engine + AI), dashboard data loading via `invoke()`, settings with API key management |
| `src/styles.css` | 3164 | Extended stylesheet (same base as web + desktop-specific: license overlay, engine toggle) |
| `src/dashboard.html` | 134 | Dashboard shell (same structure as web) |
| `src/dashboard.js` | 35 | Dashboard init (no auth needed — always Pro, local data via `invoke()`) |
| `src-tauri/src/lib.rs` | 799 | All 19 IPC commands: generation, history, favorites, projects, settings, license validation, AI integration. AppState holds `db` and `generator` (EGCG). |
| `src-tauri/src/engine.rs` | 373 | Title generation orchestrator: calls EGCG `Generator::generate()` first, falls back to template engine. Also contains legacy `slot_name_to_pool_name()` mapping and `generate_from_templates()`. |
| `src-tauri/src/title_gen.rs` | 1270 | **EGCG algorithm** (replaces `markov.rs`). Three generation modes (70/20/10): exemplar-guided template fill, phrase stitching, keyword-embedded exemplar. Coherence-scored with pairwise affinity matrix, softmax sampling, and stemmer-based lexical affinity. |
| `src-tauri/src/db.rs` | 144 | SQLite schema (8 tables) + seed data import from `seed-data.json` |
| `src-tauri/src/main.rs` | 5 | Entry point → calls `titleforge_lib::run()` |
| `src-tauri/tauri.conf.json` | 65 | App config: version `0.1.0`, window 1100×750, CSP, bundle config, updater endpoint |
| `src-tauri/Cargo.toml` | 26 | Rust dependencies |
| `src-tauri/capabilities/default.json` | 12 | Tauri v2 permissions: core, shell:allow-open, updater |
| `src-tauri/build.rs` | 3 | Standard Tauri build hook |
| `seed-data.json` | 0.4MB | Generated by DeepSeek V4 Pro (~$3 one-time cost): 480 templates (30/category), 475 word pool entries across 8 pools, 769 curated titles across 16 categories |
| `.github/workflows/build.yml` | 84 | CI: 3-platform builds, artifact upload, auto GitHub Release on tag push |
| `package.json` | 16 | NPM: `@tauri-apps/api ^2`, `@tauri-apps/cli ^2`, scripts `dev`/`build` |
| `README.md` | 36 | Setup instructions: clone, `npm install`, `npm run dev` |

### 3.3 Rust Backend — All IPC Commands

**State management:** `AppState` struct holds `Mutex<rusqlite::Connection>` and `Mutex<title_gen::Generator>`.

| Command | Signature | Description |
|---|---|---|
| `generate_titles` | `(keyword, categories, style, genre, quantity, state) -> Vec<TitleResult>` | Offline engine: template mixer + curated fallback |
| `generate_with_ai` | `(keyword, categories, style, genre, quantity, provider, api_key, cross_medium, include_subtitles, include_translation, translate_lang, gender, finetune) -> Vec<TitleResult>` | Cloud AI via user's API key (4 providers) |
| `get_categories` | `() -> Vec<&str>` | Returns 16 category strings |
| `get_usage_stats` | `(state) -> Value` | Returns `totalGenerations`, `todayGenerations`, `totalFavorites`, `isPro: true` |
| `record_generation` | `(keyword, categories, genre, style, titles, state)` | Saves to `user_history` table |
| `get_history` | `(state) -> Vec<HistoryEntry>` | Returns all history entries ordered by date DESC |
| `get_favorites` | `(state) -> Vec<FavoriteEntry>` | Returns all favorites |
| `toggle_favorite` | `(title, keyword, score, category, state) -> bool` | Add/remove (toggle) — returns `true` if now favorited |
| `get_projects` | `(state) -> Vec<ProjectEntry>` | Returns projects with joined `project_titles` as JSON array |
| `create_project` | `(name, state) -> ProjectEntry` | Creates project, returns new entry |
| `delete_project` | `(project_id, state)` | Deletes project + cascading project_titles |
| `add_to_project` | `(project_id, title, keyword, score, state)` | Adds title to `project_titles` table |
| `update_title_notes` | `(project_id, title, notes, state)` | Updates notes on a project title |
| `get_settings` | `(state) -> HashMap<String, String>` | Returns all settings (with XOR deobfuscation for sensitive keys) |
| `set_setting` | `(key, value, state)` | Upserts setting (with XOR obfuscation for sensitive keys) |
| `get_app_info` | `(state) -> Value` | Returns `{app, version, seeded, templateCount}` |
| `validate_license` | `(key, email, state) -> Value` | HTTP call to Netlify `/licenses?action=validate`, 24h cache fallback |
| `deactivate_license` | `(state)` | Clears all `license_%` settings |

### 3.4 Engine (`engine.rs` + `title_gen.rs`)

**Orchestrator (`engine.rs`):**
- Calls EGCG `Generator::generate()` first (the new algorithm)
- Falls back to legacy template engine (`generate_from_templates()`) if EGCG doesn't produce enough results
- Deduplication and score-sorting across both passes
- Contains the `slot_name_to_pool_name()` mapping function for 80+ pool aliases → 8 standard pools

**EGCG Algorithm (`title_gen.rs`) — replaces old Markov chain:**
- **Data structures:** `Generator` struct with `word2id`, `id2word`, `affinity` (pairwise co-occurrence within window=5), `unigram_cat` (per-category word frequency), `templates`, `pools`, `exemplar_vocab`, `intro_fragments`, `closer_fragments`, `all_curated`
- **`Generator::build(conn)`:** Loads all data from SQLite, builds all indices at startup
- **`Generator::generate(keyword, categories, style, genre, qty)`:** Public API with 70/20/10 proportional mode allocation
- **Mode A — Exemplar-Guided Template Fill (70%):** Fill template slots by scoring candidates against left context + keyword affinity + category naturalness. Softmax sampling, never uniform random. Retries up to 6x per slot if below `MIN_COHERENCE=0.05`.
- **Mode B — Phrase Stitching (20%):** Mined intro fragments + keyword + closer fragments from curated titles
- **Mode C — Keyword-Embedded Exemplar (10%):** Find highest-affinity curated title, swap its topic token with the keyword
- **Scoring:** `EGCG raw = 2.0 × avg_pairwise_affinity + 0.5 × ln(1 + unigram_sum) - 1.5 × repeat_penalty` → normalized to 0-65 base + heuristic bonuses (keyword, numbers, curiosity, emotional, power words, word count) → capped at 100
- **Utilities:** `tokenize()`, `stem()` (crude suffix-stripping), `softmax_sample()` (temperature 0.7, top-K 12), `resolve_pool_name()` (standalone copy of pool name mapping)

**Key improvements over old Markov:**
| Issue | Markov | EGCG |
|---|---|---|
| Sparse transitions | freq-1 = dead end | Soft score, sparsity degrades gracefully |
| Noise | 15% uniform backoff | No uniform term. Fallback ladder: affinity → unigram → keyword |
| Semantics | None | Pairwise co-occurrence + stemmer-based lexical affinity |
| Keyword splice | Bidirectional creates unnatural junction | Left-to-right only, keyword fills topic slot |
| Slot filling | Random from pool | Exemplar-restricted, coherence-scored, softmax-sampled |

### 3.5 Database (`db.rs`) — SQLite
- **Data path:** `dirs::data_dir() / titleforge-desktop / titles.db`
- **8 tables:** `patterns`, `word_pools`, `curated_titles`, `user_history`, `user_favorites`, `user_settings`, `user_projects`, `project_titles`
- **Seed import:** Reads `seed-data.json`, inserts templates/word pools/curated titles with `INSERT OR IGNORE`
- **Seed lookup paths:** `./seed-data.json` (next to binary) or `$DATA_DIR/titleforge-desktop/seed-data.json`

### 3.6 Settings & API Key Security
- **XOR obfuscation:** API keys are XOR'd with the machine hostname before storage — prevents plaintext keys in SQLite
- **Marker prefix:** Obfuscated values prefixed with `obf:` and stored as hex
- **Sensitive key detection:** Any setting key containing `api_key`, `apikey`, `secret`, `token`, or `password` is obfuscated on write, deobfuscated on read
- **Known limitation:** This is obfuscation, not encryption. A determined attacker with filesystem access can extract keys. Planned migration to OS-level credential storage (macOS Keychain, Windows DPAPI, Linux libsecret).

### 3.7 AI Integration (Desktop)
- **4 providers supported:** OpenAI (`gpt-4o-mini`), DeepSeek (`deepseek-v4-flash`), Anthropic Claude (`claude-sonnet-4-5`), Google Gemini (`gemini-2.0-flash`)
- **User-managed:** API key entered in Dashboard → Settings → AI Integration, stored via `set_setting`
- **Prompt:** Single prompt with quality rules, style, and optional fine-tune injections (audience, emotion, length, angle, mustInclude, avoid)
- **Response parsing:** Same JSON extraction (strip code fences, parse `titles` key)
- **Error handling:** Returns `API error (status)` or `AI returned malformed JSON`
- **Engine toggle:** UI button switches between "Database" (local) and "AI" (cloud). Status bar shows provider and key status.

### 3.8 License System
- **Activation flow:** User enters key + email → `validate_license` Rust command → blocking HTTP GET to `https://titleforge-tool.netlify.app/.netlify/functions/licenses?action=validate&key=...&email=...`
- **Server validation (`licenses.js`):** Queries Supabase `licenses` table, checks email matches owner, verifies `is_active`, registers machine (max 3), records `activated_machines`
- **Offline cache:** On successful validation, stores `license_status=valid`, `license_tier`, `license_validated_at=<RFC3339>` in `user_settings`
- **Cache expiry:** If server unreachable, checks if last validation was < 24 hours ago
- **UI gate:** On load, `checkLicense()` calls `get_settings` — if `license_status != 'valid'`, hides `.nav`, `.hero-compact`, `.tool-section`, `.footer` and shows activation overlay
- **`initApp()`** restores all UI elements after successful activation
- **Buy link** in overlay opens `https://titleforge-tool.netlify.app/dashboard` via Tauri shell (or `window.open` fallback)

### 3.9 CI/CD (`build.yml`)
- **Triggers:** Push to `master`/`main` branches, `v*` tags, manual `workflow_dispatch`
- **3 build jobs (parallel):**
  - `build-linux` (ubuntu-22.04): `--bundles deb,appimage`
  - `build-windows` (windows-latest): `--bundles nsis`
  - `build-macos` (macos-latest): `--bundles dmg`
- **Artifacts:** Each job uploads `src-tauri/target/release/bundle/**/*` with names `titleforge-linux`, `titleforge-windows`, `titleforge-macos`
- **Release job:** Only on tag push (`startsWith(github.ref, 'refs/tags/v')`). Downloads all artifacts, generates release notes, uses `softprops/action-gh-release@v2` to create GitHub Release
- **Env vars:** Uses `TAURI_UPDATER_PRIVATE_KEY` and `TAURI_UPDATER_KEY_PASSWORD` from repo secrets for updater signature generation
- **Node 20** used across all builds

### 3.10 Auto-Updater
- **Configured in `tauri.conf.json`:** Plugin `updater` with public key `nMmbyRXVNON1KJT3yWIb0m/2xrfNFRPeZGrsRUEMk2I=`
- **Endpoint:** `https://titleforge-tool.netlify.app/updates.json`
- **`updates.json` format:** Version `0.1.0`, platform-specific URLs pointing to GitHub Releases, empty signatures (needs private key setup to fill)
- **Capability permissions:** `updater:default`, `updater:allow-check`, `updater:allow-download-and-install`

### 3.11 Versioning
- Desktop: `0.1.0` (beta semver) in both `package.json` and `tauri.conf.json`
- Web: Version also in `updates.json`
- Cargo.toml still says `1.0.0` (package version — separate from app version in tauri.conf.json)

### 3.12 Seed Data Structure
```json
{
  "generated_at": "ISO timestamp",
  "model": "deepseek-v4-pro",
  "stats": { "total_templates": 480, "total_word_pool_entries": 475, "total_curated_titles": 769 },
  "templates": {
    "book": [{ "template": "...", "slots": [...], "genre": "any", "tone": "normal", "quality_score": 0.8 }]  // 30 each
    // ... 16 categories
  },
  "word_pools": {
    "action_verbs": [50 words], "power_adjectives": [55], "nouns": [60],
    "timeframes": [50], "emotions": [60], "numbers": [70], "hooks": [70], "results": [60]
  },
  "curated_titles": {
    "book": [{ "title": "...", "genre": "...", "tone": "...", "appeal_score": 85, "notes": "" }]  // ~50 each
    // ... 16 categories (article has only 26)
  }
}
```

---

## 4. Frontend Differences: Web vs Desktop

| Aspect | Web | Desktop |
|---|---|---|---|
| **Layout** | Top nav bar + scrollable page | Left sidebar (Ink, 220px) + content area (Paper) |
| **Activation** | Supabase auth modal | Full-screen split-panel takeover (no UI until activated) |
| **Pages** | `index.html`, `dashboard.html` (separate) | Single page — Generator, Dashboard, Settings are sidebar panels |
| **Auth** | Supabase (CDN + localStorage fallback) | License key (HTTP → offline cache) |
| **Pro gate** | Tiered (guest/free/pro) | Always Pro — `isPro = true`, `isLoggedIn = true` |
| **Data source** | Supabase via Netlify Functions | SQLite via `invoke()` |
| **Generation** | AI only (via Netlify Function) | Local engine OR AI (bring-your-own-key) |
| **Dashboard** | 5 sub-tabs + Settings separate page | 5 sub-tabs + Settings as own sidebar panel |
| **Favorites/Projects** | Server-side (Supabase tables) | Local SQLite tables |
| **Floating generator** | Yes (FAB button) | No |
| **Engine toggle** | No | Yes (Database / AI) |

---

## 5. What We Changed (This Session & Prior)

### Logo Redesign
- Old: Blue gradient `#2563eb→#1e3a5f` with anvil + pen nib + spark
- New: Amber forge palette `#E8782B→#D45C1A`, simplified anvil shape, forge flame indicator, dark base

### Font Fallbacks
- `--font-display`: `'Clash Display', Georgia, 'Times New Roman', serif` (was `'Syne', sans-serif`)
- `--font-body`: `'Satoshi', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, ...` (was `'Instrument Sans', sans-serif`)
- Applied in both `styles.css` files

### Complete UI Redesign (v0.2.0)
- **Activation screen:** Full-screen split-panel (Ink 40% left + Paper 60% right). No chrome, no nav, no tool visible until activation.
- **Left sidebar:** 220px Ink sidebar with 3 nav items (Generator, Dashboard, Settings). Single-page app — no separate pages.
- **Generator:** Two-column card layout (input left 55%, config right 45%) with full-width results below.
- **Dashboard merged:** Dashboard tabs (Overview, History, Favorites, Projects, Export) rendered inline. Settings moved to its own sidebar panel.
- **Files removed:** `dashboard.html` and `dashboard.js` eliminated.
- **Version bump:** 0.1.1 → 0.2.0, tag `v0.2.0` pushed.

### Version Bump History
- `1.0.0` → `0.1.0` (initial beta)
- `0.1.0` → `0.1.1` (icon fix + FOUC fix)
- `0.1.1` → `0.2.0` (complete UI redesign)

### EGCG Algorithm (July 15, 2026)
- **New file:** `src-tauri/src/title_gen.rs` (1270 lines) — full EGCG implementation replacing the old Markov chain
- **Deleted file:** `src-tauri/src/markov.rs` (779 lines) — fully superseded
- **Modified:** `engine.rs` — now orchestrates EGCG first, falls back to legacy template engine
- **Modified:** `lib.rs` — AppState holds `Mutex<Generator>` instead of `Mutex<MarkovModel>`; initialization builds EGCG Generator at startup
- **Three generation modes:** exemplar-guided template fill (70%), phrase stitching (20%), keyword-embedded exemplar (10%)
- **Coherence scoring:** pairwise affinity matrix + unigram frequency + repeat penalty, normalized to 0-100
- **Dependencies:** Pure Rust (`std` + `rand 0.8` + `rusqlite` + `serde`) — no new crates needed
- **Build status:** Compiles clean, all 10 tests pass, zero warnings

### Version Bump History (continued)
- `0.2.0` → current: EGCG algorithm replaces Markov (version not yet bumped)

### EGCG Follow-up Fixes (July 15, 2026, second pass)
- Style/tone was collected by the UI but never used inside EGCG (`title_gen.rs`) — only the legacy fallback engine respected it. `TemplateInfo` now loads `tone` from `patterns`, and `fill_template_mode`/`embed_mode` filter by genre+style with a fallback ladder (genre+style → genre-only → unfiltered).
- Mode B (phrase stitching) mined intro/closer fragments globally with no category segmentation, so stitched titles could combine fragments from unrelated categories. Fragments are now mined per-category (`HashMap<String, Vec<String>>`) and `stitch_mode` only stitches within the requested category.
- See `EGCG_Audit_Report.md` §5 for details. **Not yet verified with `cargo build`/`cargo test`** — bash access was unavailable in the session that made this pass; changes were manually type-traced but should be compiled and tested before release.

### CI Update
- Added tag trigger (`tags: ['v*']`)
- Added release job with auto GitHub Release on tag push

---

## 6. Current Status & Blockers

### 6.1 Done
- Web app: Landing page, tool, auth, Stripe payments, 7 Netlify functions, Supabase schema (6 tables), deployed on Netlify
- Desktop app: Tauri v2 shell, Rust IPC (19 commands), SQLite schema (8 tables), offline engine, seed data imported, AI integration (4 providers), license system, activation screen, XOR API key obfuscation
- Brand: Amber palette, Clash Display + Satoshi fonts, anvil logo, proper OS-level app icons
- CI: 3-platform builds with auto-release on tag
- UI redesign: Activation screen, left sidebar, single-page app, merged dashboard (v0.2.0)

### 6.2 Blocked / Needs Action
1. **TAURI_UPDATER_PRIVATE_KEY** — needs to be added as GitHub repo secret for signature generation in CI
2. **GitHub Release from v0.2.0** — tag just pushed, CI should be running
3. **`updates.json` signatures** — will be filled automatically by `tauri build` once the private key secret is set
4. **Waitlist email sequence** — emails being collected, no follow-up yet (P2 on roadmap)
5. **Marketing & launch** — Product Hunt, Reddit, Twitter, demo video (P4 on roadmap)

### 6.3 Roadmap — July 2026

| Priority | Item | Time | Why |
|---|---|---|---|
| **P1** | Desktop app build (Tauri + Rust) | 3–4 weeks | Revenue engine — $29/$49 one-time. Seed data ready. |
| **P2** | Waitlist email sequence | 1–2 days | Emails being collected, no follow-up yet. Fill pipeline while building. |
| **P3** | Dashboard visual polish | 2–3 days | Functional but not premium. Match landing page aesthetic. |
| **P4** | Marketing & launch | 1 week | Product Hunt, Reddit, Twitter, demo video. Only after desktop ships. |

**Strategy:** Build desktop app → capture waitlist leads in parallel → polish dashboard → launch everything together in ~4 weeks.

---

## 7. Key Decisions & Conventions

- **No framework:** Both apps use vanilla HTML/CSS/JS — no React, Vue, or other frameworks
- **Desktop is always Pro:** No tier gating — all 16 categories, 9 styles, all features unlocked
- **XOR obfuscation for API keys:** Basic device-bound obfuscation using machine hostname — planned migration to OS-level credential storage
- **License overlay blocks all UI:** Nav, hero, tool section, and footer all hidden until valid activation
- **Seed data generated by AI:** DeepSeek V4 Pro, one-time ~$3 cost, 480 templates + 475 word pool entries + 769 curated titles
- **Template scoring:** Quality scores in seed data range from ~0.3 to ~1.0, templates queried by quality_score DESC
- **Idempotent SQL:** `supabase-setup.sql` uses `CREATE TABLE IF NOT EXISTS`, `DROP POLICY IF EXISTS` before recreating — safe to re-run
- **Cross-medium specificity:** Prompt instructions vary by medium (YouTube ALL CAPS, books poetic, podcasts conversational, etc.)
- **JSON repair is critical:** AI models frequently return malformed JSON — 4-layer fallback in `generate.js`, simpler approach in desktop Rust

---

## 8. File Reference (Complete)

### Web App (`titleforge/`)

| Path | Lines | Type | Purpose |
|---|---|---|---|
| `index.html` | 663 | HTML | Landing page with all sections, modals, sticky CTA |
| `app.js` | 2826 | JS | All UI: auth, generation, results, floating gen, dashboard, settings, licenses, export, projects |
| `styles.css` | 3003 | CSS | Full design system, responsive, dashboard, floating gen, cross-medium |
| `dashboard.html` | 134 | HTML | Dashboard shell with 6 tabs |
| `dashboard.js` | 85 | JS | Dashboard auth check, Stripe redirect handler, tab wiring |
| `netlify.toml` | 12 | TOML | Netlify functions dir + redirects |
| `netlify/functions/config.js` | 24 | JS | Public config endpoint |
| `netlify/functions/generate.js` | 569 | JS | AI generation (4 providers, 3 modes, JSON repair) |
| `netlify/functions/licenses.js` | 193 | JS | License CRUD + validation + machine registration |
| `netlify/functions/stripe-webhook.js` | 100 | JS | Stripe payment webhook → user upgrade |
| `netlify/functions/usage.js` | 329 | JS | Usage + history/favorites/projects API |
| `netlify/functions/verify-subscription.js` | 76 | JS | Pro status check |
| `netlify/functions/waitlist.js` | 45 | JS | Email waitlist signup |
| `supabase-setup.sql` | 231 | SQL | Schema: 6 tables, RLS policies, RPC, indexes |
| `updates.json` | 23 | JSON | Desktop auto-updater manifest |
| `seed-data.json` | ~50K | JSON | 480 templates + 475 words + 769 curated titles |
| `logo.svg` | — | SVG | Vector logo (amber anvil + forge spark) |
| `package.json` | — | JSON | NPM package (minimal, for Netlify) |

### Desktop App (`titleforge-desktop/`)

| Path | Lines | Type | Purpose |
|---|---|---|---|---|
| `src/index.html` | 293 | HTML | Main app: hero, tool, engine toggle, license overlay |
| `src/app.js` | 1427 | JS | Desktop UI: license gate, generation, dashboard, settings |
| `src/styles.css` | 3164 | CSS | Extended stylesheet (base + license overlay + engine toggle) |
| `src/dashboard.html` | 134 | HTML | Dashboard shell (same as web) |
| `src/dashboard.js` | 35 | JS | Dashboard init (no auth) |
| `src/logo.svg` | — | SVG | Vector logo |
| `src-tauri/src/lib.rs` | 799 | Rust | 19 IPC commands, XOR obfuscation, license validation, AI generation |
| `src-tauri/src/engine.rs` | 373 | Rust | Type generation orchestrator: EGCG dispatch + template fallback |
| `src-tauri/src/title_gen.rs` | 1270 | Rust | EGCG algorithm: 3 modes, coherence scoring, softmax sampling, pairwise affinity |
| `src-tauri/src/db.rs` | 144 | Rust | SQLite schema (8 tables), seed import |
| `src-tauri/src/main.rs` | 5 | Rust | Entry point |
| `src-tauri/tauri.conf.json` | 65 | JSON | App config, window, CSP, bundle, updater |
| `src-tauri/Cargo.toml` | 26 | TOML | Rust dependencies |
| `src-tauri/capabilities/default.json` | 12 | JSON | Tauri v2 permissions |
| `src-tauri/build.rs` | 3 | Rust | Tauri build hook |
| `seed-data.json` | ~50K | JSON | Seed data (same as web repo copy) |
| `.github/workflows/build.yml` | 84 | YAML | CI: 3-platform builds + auto-release |
| `package.json` | 16 | JSON | NPM: `@tauri-apps/api`, `@tauri-apps/cli` |
| `README.md` | 36 | MD | Setup instructions |

---

## 9. Quick Reference

### Git Remotes
```bash
# Web app
cd /home/olammy/projects/paul/titleforge
git remote -v  # → github.com/Olammyinc/titleforge

# Desktop app
cd /home/olammy/projects/paul/titleforge-desktop
git remote -v  # → github.com/Olammyinc/titleforge-desktop
```

### Build Commands
```bash
# Web — deploy
cd titleforge && npx netlify deploy --prod

# Desktop — dev
cd titleforge-desktop && npm run dev

# Desktop — build all platforms
cd titleforge-desktop && npm run build
# or with platform filter:
npx tauri build --bundles deb,appimage   # Linux only
npx tauri build --bundles nsis           # Windows only
npx tauri build --bundles dmg            # macOS only
```

### Env Vars Required
```bash
# Netlify (required for web app)
SUPABASE_URL            # Supabase project URL
SUPABASE_SERVICE_KEY    # Supabase service_role key
SUPABASE_ANON_KEY       # Supabase anon/public key
DEEPSEEK_API_KEY        # DeepSeek API key (or OPENAI_API_KEY / ANTHROPIC_API_KEY / FLUX_API_KEY)
AI_PROVIDER             # "deepseek" | "openai" | "anthropic" | "flux"
STRIPE_SECRET_KEY       # Stripe secret key
STRIPE_WEBHOOK_SECRET   # Stripe webhook signing secret
STRIPE_PRO_LINK         # Stripe Payment Link URL
STRIPE_PORTAL_LINK      # Stripe Customer Portal URL
STRIPE_SUCCESS_URL      # Post-payment redirect URL

# GitHub Secrets (required for CI)
TAURI_UPDATER_PRIVATE_KEY   # Private key for updater signature
TAURI_UPDATER_KEY_PASSWORD  # Password for private key (if encrypted)
```

### Database URLs
- **Web:** Supabase project dashboard → `titleforge` schema with 6 tables
- **Desktop:** `~/.local/share/titleforge-desktop/titles.db` (Linux), `~/Library/Application Support/titleforge-desktop/titles.db` (macOS), `%APPDATA%/titleforge-desktop/titles.db` (Windows)

### Key Endpoints
- **Web app:** `https://titleforge-tool.netlify.app/`
- **Generator API:** `https://titleforge-tool.netlify.app/.netlify/functions/generate`
- **License validation:** `https://titleforge-tool.netlify.app/.netlify/functions/licenses?action=validate&key=...&email=...`
- **Auto-updater:** `https://titleforge-tool.netlify.app/updates.json`
- **Desktop releases:** `https://github.com/Olammyinc/titleforge-desktop/releases`
