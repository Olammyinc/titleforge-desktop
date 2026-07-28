# TitleForge — Full Project Context

> **Last updated:** 2026-07-28
> **Repos:** `github.com/Olammyinc/titleforge` (web) · `github.com/Olammyinc/titleforge-desktop` (desktop)

---

## 1. Project Overview

**TitleForge** is an AI-powered title generator for creators — generates titles for books, articles, YouTube videos, songs, podcasts, newsletters, speeches, product names, character names, children's names, and more. Two products:

| | Web App | Desktop App |
|---|---|---|
| **Deployment** | Netlify (free tier) | Tauri v2 native binary |
| **Pricing** | Free tier + $15.83/mo annual Pro ($19/mo monthly) | $29 Core / $59 Pro / $89 Studio one-time |
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
| `seed-data.json` | 1.0MB | 1,300 templates + 889 word pool entries + 2,623 curated titles (same as desktop seed) |

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
- **Seed data:** 1,300 templates (30 per category × 16), 889 word pool entries across 8 pools, 2,623 curated titles across 16 categories with 9 tones each
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
| `seed-data.json` | 1.0MB | Generated by DeepSeek V4 Pro (~$12 one-time cost): 1,300 templates (30/category), 889 word pool entries across 8 pools, 2,623 curated titles across 16 categories (tone-tagged: normal + 8 distinct tones) |
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
  "stats": { "total_templates": 1300, "total_word_pool_entries": 889, "total_curated_titles": 2623 },
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
| **Pro gate** | Tiered (guest/free/pro) | Tiered — Core/Pro/Studio with backend gating. Tier stored in local SQLite, checked at generation time. |
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

### Complete UI Redesign (v0.2.0)
- Activation screen, left sidebar, merged dashboard, single-page app. `dashboard.html`/`dashboard.js` removed.

### EGCG Algorithm (July 15, 2026)
- Replaced old Markov chain with 3-mode EGCG. 11 bugs found over 4 audit rounds. `{placeholder}` leak fixed (July 28) via `strip_placeholders()`. 196-title sanity test: 0 leaks, 100% keyword presence. Still produces template-garbage on some keywords.

### TitleSmith → TitleForge Rebrand (July 25-28, 2026)
- All source files renamed. Crate name `titlesmith-desktop` → `titleforge-desktop`. `tauri.conf.json` productName and identifier updated. UI labels, AI prompts, module docs all changed. Some data paths and `site/` folder still have old references (see §6.5).

### License System Overhaul (July 25-28, 2026)
- Licenses stored by email, not Supabase user_id. Desktop buyers don't need a web account.
- `generate_from_purchase` endpoint in `licenses.js` for Stripe webhook → auto license generation.
- `stripe-webhook.js` detects desktop purchases (metadata check) and emails license keys via Resend.
- License key format: `TF-CORE-XXXX`, `TF-PRO-XXXX`, `TF-STUDIO-XXXX`.
- Machine tracking: `validate_license` and `background_verify` send `&machine=hostname`.
- Website license checks don't burn device slots.

### Background License Verification (July 25, 2026)
- New `background_verify` Rust command. 5s timeout, refresh-only — never revokes.
- Frontend `startBackgroundTasks()` runs every 30 minutes.
- Stores `license_key` and `license_email` for re-verification.

### Tier Gating (July 25, 2026)
- Backend: Core capped at 25 titles/request, no cloud AI, no projects. Pro/Studio: 100 titles, full access.
- Known gap: frontend always displays "PRO" badge regardless of tier. Studio "unlimited" limit not honored.

### EGCG Template Leak Fix (July 28, 2026)
- Root cause: templates had more `{placeholder}` occurrences than defined slot entries.
- Fixed via `strip_placeholders()` in `assemble_title()`. Manual implementation (no regex crate added).
- Sanity test: 0 bracket leaks in 196 titles, 100% keyword presence.

### Local LLM Integration (July 25-28, 2026)
- `local_llm.rs` compiled and tested with candle-rs 0.11 on Windows.
- SmolLM2-135M (100.6MB): 4-14s/title. SmolLM2-360M (258MB): 12-16s/title.
- Fixed `sample_token()` to handle 2D logits from prefill pass.
- Quality is insufficient as sole engine — 135M ignores title-only instructions, produces category-irrelevant output ~36% of the time even with few-shot prompting. BYO-key cloud AI is the recommended quality path.
- Model paths use `titleforge-desktop` and `com.titleforge.desktop`.

### SEO Scoring Engine (July 27, 2026)
- New `seo.rs`. 9 signals: length fit (chars per platform), keyword presence (front-loaded bonus), keyword density (10-25% sweet spot), search pattern match (~110 n-grams), question format, number/year detection, Flesch reading ease, power word density, uniqueness (n-gram vs curated corpus). All local, no API calls.
- 19 unit tests pass (10 EGCG + 9 SEO). Weighted to 0-100 per spec.
- Frontend: SEO badge pill (green 80+/amber 50-79/gray <50), SEO details panel with per-signal breakdown, dashboard mini-badge in history.
- Integrated into engine.rs: LLM pass scores inline, EGCG+curated scored via post-generation sweep.

### keyring Migration (July 27, 2026)
- XOR obfuscation replaced with OS credential storage (keyring crate v3).
- macOS Keychain / Windows Credential Manager / Linux libsecret.
- Dual-write pattern: stores in keyring + XOR-obfuscated SQLite fallback.
- Empty values clear both stores.

### Provider Cascade (July 25, 2026)
- `generate.js`: single AI_PROVIDER → cascade: DeepSeek → OpenAI → Anthropic.
- First provider with a configured API key wins. Falls through on failure.
- AI_PROVIDER env var is now unused.

### Web Content Fixes (July 25-28, 2026)
- Fake "247,000+ titles" hero stat removed. Replaced with "1 payment, lifetime access".
- Fabricated testimonials (Sarah K., Marcus T., Marcus O., Rachel K.) replaced with honest product benefit cards.
- Footer copy fixed: removed "Every market rate is a true market reference" placeholder text.
- vidIQ/SEMrush comparison tightened to title-specific claims with factual pricing and acknowledged strengths.

### Desktop Sales Pages (July 25, 2026)
- `desktop.html`: expanding sales page (hero, features, walkthrough mockups, 3-tier pricing, FAQ, download CTA).
- `desktop-download.html`: OS-detecting download page, collapsible install instructions, system requirements, license verification form.
- `desktop.css`: ported from `site/styles.css` with full palette remap to TitleForge Editorial Industrial.
- Clean URL redirects in `netlify.toml`: `/desktop`, `/desktop/download`, `/download`.

### Plausible Analytics (July 25, 2026)
- Script tags added to `index.html` and `desktop.html`. Missing on `desktop-download.html`.
- Custom events tracked: `signup`, `generate`, `pro_upgrade_click`, `favorite_add`.
- Account not yet set up.

### Code Review Fixes (July 28, 2026)
- Claude UI value `claude` → `anthropic` (was broken, users picking Claude got an error).
- `background_verify` no longer revokes — only refreshes on success. No 24h cache fallback needed.
- Machine tracking: both validate and background_verify now send `&machine=hostname`.
- Tier renamed `"basic"` → `"core"` everywhere in Rust + JS. License prefix `TF-BASIC` → `TF-CORE`.
- All remaining `titlesmith-desktop` / `com.titlesmith.desktop` references in lib.rs renamed.
- Fake SHA256 hashes removed from download page.

---

## 6. Current Status (July 28, 2026)

### 6.1 Done (Compiled + Tested)
- `cargo check` — 0 errors, 0 warnings
- `cargo test` — 19/19 pass (10 EGCG + 9 SEO)
- `cargo build --release` — 22.74 MB binary on Windows
- `npm run dev` — app launches, EGCG generator builds (2,112 words), LLM lazy-loads
- Desktop pages live at `titleforge-tool.netlify.app/desktop` and `/desktop/download`
- License system overhaul (email-based, Stripe webhook, Resend email delivery)
- Web deploy with nav, pricing teaser, honest testimonials, factual comparisons

### 6.2 Blocked / Needs Action

1. **EGCG is still the primary engine.** The plan to adopt SmolLM2 as sole engine was not completed — SmolLM2 quality is insufficient for production. EGCG remains as Pass 2 with a fixed placeholder leak. BYO-key cloud AI is the only path to production-quality titles.

2. **SmolLM2 quality needs improvement.** 135M produces output but can't follow title-only instructions consistently. Few-shot prompting helps but doesn't fix the instruction-following issue. Options: try 360M with better prompting, try llama.cpp for faster inference, or accept cloud-first with EGCG as fallback.

3. **EGCG Mode A and B are still live.** The strategic decision to "kill EGCG and keep only curated retrieval" was not executed. The engine.rs 3-pass pipeline (LLM → EGCG → curated) is the current shipped architecture.

4. **Version numbers are inconsistent.** App is `v1.0.0-beta.1` in Cargo.toml but shows `v0.8.0` in activation screen, `v0.2.0` in download page links, and `v0.5.0` in settings. No single source of truth.

5. **Frontend always shows PRO badge.** Tier gating exists on backend but the UI doesn't read `get_usage_stats.tier` — it always displays "Desktop Pro" regardless of actual tier.

6. **Studio "unlimited batch" doesn't exist.** Code caps all tiers at 100 titles. Slider max is hard-locked to 100. The sales page claim is false.

7. **Download page SHA256 hashes removed.** Need real hashes computed after production builds.

8. **Plausible account not set up.** Script tags are in place but no account created. Missing on desktop-download.html.

9. **`site/` folder still TitleSmith-branded.** The legacy marketing prototype was never cleaned up. Should be archived or deleted.

10. **`package-lock.json` stale.** Still says `titlesmith-desktop` while `package.json` says `titleforge-desktop`.

### 6.3 Strategic Decisions (Active)

| # | Decision | Status |
|---|----------|--------|
| 1 | EGCG as primary engine, BYO cloud AI for quality | Active (LLM unproven) |
| 2 | Three pricing tiers: $29 Core / $59 Pro / $89 Studio | Deployed |
| 3 | One-time purchase + optional update renewal | Planned, not implemented |
| 4 | Unify brand under TitleForge | Done (minor remnants in site/) |
| 5 | Web Pro → free Core desktop license | Planned, not implemented |
| 6 | Background license verification every 30 min | Done |
| 7 | License by email, not user_id | Done |
| 8 | Recurring revenue via annual updates + major version upgrades | Not implemented |

---

## 7. Key Decisions & Conventions

- **No framework:** Both apps use vanilla HTML/CSS/JS — no React, Vue, or other frameworks
- **Desktop is tiered:** Core/Pro/Studio tiers with backend gating. Tier stored in local SQLite, verified by server.
- **Offline-first, online-verified:** App works fully offline. Background license refresh every 30 min when online.
- **License by email, not user_id:** Desktop buyers don't need a Supabase account.
- **One brand, two products:** "TitleForge" (web SaaS) and "TitleForge Desktop" (downloadable). Same palette, fonts, logo.
- **EGCG is the primary offline engine.** Not ideal but compiles and produces output. BYO-key cloud AI for quality.
- **License key prefix:** TF-CORE (not TF-BASIC) for the $29 tier.
- **Upgrade pricing = difference (not yet implemented):** Users pay only the gap between tiers.

---

## 8. Quick Reference

### 8.1 Build Commands
```bash
cd titleforge-desktop && npm run dev          # dev server + app
cd titleforge-desktop && npx tauri build       # production bundles
cargo test --release                            # 19 tests
cd titleforge && npx netlify deploy --prod      # web deploy
```

### 8.2 License Key Formats
| Prefix | Tier | Price | Source |
|--------|------|-------|--------|
| `TF-CORE-XXXX` | Core | $29 | Free with Web Pro, or $29 standalone |
| `TF-PRO-XXXX` | Pro | $59 | $59 standalone, or upgrade from Core |
| `TF-STUDIO-XXXX` | Studio | $89 | $89 standalone |

### 8.3 Web Routes
| URL | File | Purpose |
|-----|------|---------|
| `/` | `index.html` | Web app landing + tool |
| `/desktop` | `desktop.html` | Desktop sales page |
| `/desktop/download` | `desktop-download.html` | Download page |
| `/dashboard` | `dashboard.html` | User dashboard |

### 8.4 Database URLs
- **Web:** Supabase project → `titleforge` schema (6 tables)
- **Desktop:** `%APPDATA%/titleforge-desktop/titles.db` (Windows), `~/Library/Application Support/titleforge-desktop/titles.db` (macOS), `~/.local/share/titleforge-desktop/titles.db` (Linux)
