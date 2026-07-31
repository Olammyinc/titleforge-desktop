# TitleForge — Full Project Context

> **Last updated:** 2026-07-31 (Qwen fixed at k=1, but sampler is deterministic — batch generation is broken)
> **Repos:** `github.com/Olammyinc/titleforge` (web) · `github.com/Olammyinc/titleforge-desktop` (desktop)
> **Canonical:** This file at `paul/CONTEXT.md` is the single source of truth for both products. `titleforge-desktop/CONTEXT.md` is a read-only mirror of §3 and §6 only.

---

## 1. Project Overview

**TitleForge** is an AI-powered title generator for creators — generates titles for books, articles, YouTube videos, songs, podcasts, newsletters, speeches, product names, character names, children's names, and more. Two products:

| | Web App | Desktop App |
|---|---|---|
| **Deployment** | Netlify (free tier) | Tauri v2 native binary |
| **Pricing** | Free / Pro ($15.83/mo annual, $19/mo monthly) | $29 Core / $59 Pro / $89 Studio (one-time) |
| **AI** | Serverless via Netlify Functions (provider cascade: DeepSeek → OpenAI → Anthropic) | Bring-your-own-key (OpenAI, DeepSeek, Claude, Gemini) + offline engine |
| **Database** | Supabase Postgres (6 tables, RLS) | Local SQLite (`titles.db`) |
| **Auth** | Supabase Auth (CDN + localStorage fallback) | License key activation (24h offline cache + 30min background refresh) |
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
- **AI Provider:** Cascade — DeepSeek V4 Flash → OpenAI (`gpt-4o-mini`) → Anthropic (`claude-3-5-sonnet`). First provider with a configured API key wins; falls through on failure.
- **Auth:** Supabase Auth (CDN: `@supabase/supabase-js@2`) + localStorage fallback (`titleforge_auth` key)
- **Database:** Supabase Postgres — 6 tables with Row Level Security
- **Payments:** Stripe Payment Links + Customer Portal, webhook upgrades `user_metadata.isPro`

### 2.2 Key Files

| File | Lines | Purpose |
|---|---|---|
| `index.html` | 596 | Landing page: hero, benefits, comparison, pricing (web + desktop), FAQ, auth/waitlist/exit modals, sticky CTA |
| `app.js` | 2839 | All UI logic: auth, generation, results display, floating generator, dashboard rendering, settings, license management, export, projects |
| `styles.css` | 3003 | Full stylesheet: design system (CSS variables), nav, hero, benefits, why section, comparison strip, pricing, FAQ, tool section, results, cross-medium, floating generator, dashboard, responsive breakpoints |
| `dashboard.html` | 134 | Dashboard shell: 6 tabs (Overview, History, Favorites, Projects, Export, Settings) |
| `dashboard.js` | 84 | Dashboard init: auth check from localStorage, Stripe redirect handler, tab wiring |
| `desktop.html` | 595 | Desktop sales page: hero, features, walkthrough mockups, 3-tier pricing, FAQ, download CTA |
| `desktop-download.html` | 956 | OS-detecting download page, collapsible install instructions, system requirements, license verification form |
| `desktop.css` | 908 | Desktop page styles (ported from legacy `site/styles.css`, remapped to TitleForge palette) |
| `netlify.toml` | 35 | Netlify config: functions dir, redirects for `/api/*`, `/desktop`, `/desktop/download`, `/download` |
| `supabase-setup.sql` | 300 | Idempotent schema: 6 tables, RLS policies, RPC for atomic usage increment, indexes |
| `logo.svg` | — | Vector logo: anvil + forge spark in amber |
| `seed-data.json` | 1.0 MB | 1,300 templates + 889 word pool entries + 2,623 curated titles (mirror of desktop seed) |

### 2.3 Netlify Functions

| Function | Lines | Purpose |
|---|---|---|
| `config.js` | 24 | Returns public config: Supabase URL, anon key, Stripe links |
| `generate.js` | 649 | AI title generation: provider cascade, 3 prompt modes (standard, cross-medium, name rubric), 4-layer JSON repair pipeline, 7 fine-tune fields |
| `licenses.js` | 279 | License CRUD: validate (public, email-based), generate for Pro users, `generate_from_purchase` (Stripe path), deactivate, machine registration (max 3 devices) |
| `stripe-webhook.js` | 214 | `checkout.session.completed` — signature verify, desktop-purchase branch (metadata-detected → generates key → emails via Resend), web-Pro branch (sets `user_metadata.isPro`) |
| `usage.js` | 346 | Usage tracking + dashboard API: GET → usage/history/favorites/projects; POST → increment (atomic RPC), history, favorites, projects, notes |
| `verify-subscription.js` | 130 | Checks Pro status via token, syncs usage table |
| `waitlist.js` | 45 | Captures email signups to Supabase waitlist table |

### 2.4 Database Schema (Supabase — 6 tables)

All tables have Row Level Security enabled with per-user policies.

**1. `usage`** — Daily usage tracking
- `id` UUID PK, `user_id` UUID FK→`auth.users`, `date` DATE, `count` INTEGER, `is_pro` BOOLEAN
- Unique constraint on `(user_id, date)`
- RPC `increment_usage(p_user_id, p_is_pro)` for atomic increments

**2. `title_history`** — Saved generation batches
- `id` UUID PK, `user_id` UUID FK, `keyword` TEXT, `categories` TEXT[], `genre` TEXT, `style` TEXT, `titles` JSONB

**3. `title_favorites`** — Starred/bookmarked titles
- `id` UUID PK, `user_id` UUID FK, `title` TEXT, `score` INTEGER, `keyword` TEXT, `category` TEXT

**4. `title_projects`** — Title collections
- `id` UUID PK, `user_id` UUID FK, `name` TEXT, `titles` JSONB

**5. `licenses`** — Desktop app license keys
- `id` UUID PK, `user_id` UUID FK **(nullable — desktop-only buyers have no auth row)**, `email` TEXT, `license_key` TEXT UNIQUE, `tier` TEXT (`core`/`pro`/`studio`), `source` TEXT, `is_active` BOOLEAN, `activated_machines` TEXT[], `expires_at` TIMESTAMPTZ
- Key formats: `TF-CORE-XXXX`, `TF-PRO-XXXX`, `TF-STUDIO-XXXX`
- Validation is by (`license_key`, `email`) — no Supabase account required

**6. `waitlist`** — Desktop app waitlist signups
- `id` UUID PK, `email` TEXT UNIQUE, `source` TEXT

### 2.5 Auth Flow
1. Supabase CDN script loaded: `@supabase/supabase-js@2`
2. `tryInitSupabase()` fetches config from `/.netlify/functions/config` for Supabase URL + anon key
3. If CDN is blocked, `localStorage` fallback reads `titleforge_auth` key
4. On successful auth, `onAuthSuccess()` persists `{email, token, isLoggedIn}` to localStorage, applies Pro UI
5. `onAuthRestoredFromStorage()` — cross-page auth (dashboard reads localStorage if Supabase CDN didn't load)
6. Guest mode always works: 3 generations, no signup, local-only tracking via `titleforge_guest_usage` localStorage key
7. Free tier: 5/day, requires account (authenticated Supabase user)

### 2.6 Payments
- **Web Pro subscription:** Stripe Payment Link, `$19/mo` or `$190/yr` (17% annual discount)
- **Desktop one-time:** Separate Stripe Payment Links per tier ($29 / $59 / $89), `metadata.product = "desktop"`, `metadata.tier = "core|pro|studio"`
- **Customer Portal:** Subscription management (cancellation) for web Pro
- **Webhook — web Pro:** `checkout.session.completed` → verify signature → find user by email → set `user_metadata.isPro = true`
- **Webhook — desktop:** `checkout.session.completed` with `metadata.product == "desktop"` → generate `TF-<TIER>-XXXX` key → insert into `licenses` table with buyer email → email the key via Resend
- **Dashboard redirect:** After Stripe checkout, redirects to `dashboard.html?session_id=...` → `verifySubscription()` → checks `verify-subscription` → refreshes

### 2.7 AI Generation (`generate.js`)
- **Provider cascade:** DeepSeek → OpenAI → Anthropic. `AI_PROVIDER` env var is no longer used.
- **Models:** DeepSeek `deepseek-v4-flash`, OpenAI `gpt-4o-mini`, Anthropic `claude-3-5-sonnet`
- **3 prompt modes:**
  1. **Standard:** Categories as comma-separated list, generates title array with scores + breakdowns
  2. **Cross-medium:** Per-category adaptation with medium-specific conventions (YouTube ALL CAPS, books poetic, etc.)
  3. **Name rubric:** For `childname`, `character`, `street` — uniqueness, memorability, meaning depth, pronunciation, origin vibe
- **7 fine-tune fields:** audience, emotion, length, angle, mustInclude, avoid, beatTitle
- **JSON repair pipeline (4 layers):** direct parse → `repairJson()` → `repairTruncatedJson()` → last-good-position scan
- **`response_format: { type: "json_object" }`** used on OpenAI-compatible providers
- **Sampling:** Temperature 0.85, `frequency_penalty: 0.6`, `presence_penalty: 0.4`

### 2.8 Frontend Features
| Feature | Guest | Free | Pro |
|---|---|---|---|
| Generations | 3 total | 5/day | Unlimited |
| Titles per batch | 10 | 10 | 100 |
| Categories | 5 | 5 | 16 |
| Styles | 4 | 4 | 9 |
| Fine-tune / Cross-medium / Subtitles / Translation | No | No | Yes |
| Score visible | Yes | Yes (teasered) | Full breakdown |
| Dashboard / Favorites | No | Yes | Yes |
| Projects / CSV Export | No | No | Yes |
| Desktop license | No | No | Core included |

**Landing page sections:** Hero → Benefits → Why TitleForge (comparison strip) → Desktop App teaser → Tool section → Pricing → FAQ → Footer
**Floating generator:** Sticky FAB (⚡) on all pages
**Exit intent modal:** Shows on mouseout for non-logged-in users
**Analytics:** PostHog on all 3 pages (`index.html`, `desktop.html`, `desktop-download.html`). Free tier (1M events/month). Project key `phc_AeXLRUsRwDu3Gi7CWe7U6fxqcRzCrnGG4NyVp6kyWvmP`. Events: `signup`, `generate`, `pro_upgrade_click`, `favorite_add`.

### 2.9 Deployment
```
SUPABASE_URL, SUPABASE_SERVICE_KEY, SUPABASE_ANON_KEY
DEEPSEEK_API_KEY, OPENAI_API_KEY, ANTHROPIC_API_KEY  (cascade — at least one required)
STRIPE_SECRET_KEY, STRIPE_WEBHOOK_SECRET, STRIPE_PRO_LINK, STRIPE_PORTAL_LINK, STRIPE_SUCCESS_URL
STRIPE_DESKTOP_CORE_LINK, STRIPE_DESKTOP_PRO_LINK, STRIPE_DESKTOP_STUDIO_LINK
RESEND_API_KEY, RESEND_FROM_EMAIL  (for desktop license delivery)
```
Deploy: `npx netlify deploy --prod`, git push, or drag-and-drop.

---

## 3. Desktop App (`titleforge-desktop/`)

### 3.1 Tech Stack
- **Framework:** Tauri v2 (Rust backend + webview frontend)
- **Frontend:** Vanilla HTML/CSS/JS — single-page app, left sidebar layout
- **Rust crates:** `tauri 2`, `rusqlite 0.31` (bundled SQLite), `reqwest 0.12` (blocking HTTP), `serde/serde_json`, `rand 0.8`, `chrono 0.4`, `dirs 5`, `hostname 0.4`, `keyring 3`, `llama-cpp-2 0.1.153` (Path A LLM), `candle-core / candle-transformers / candle-nn 0.11` + `tokenizers` (dead deps — kept for legacy, imported by nothing), `tauri-plugin-shell 2`, `tauri-plugin-updater 2`
- **Database:** Local SQLite via `rusqlite` with bundled compilation (no system SQLite needed)
- **Seed data:** 1,300 templates (30/category × 16), 889 word pool entries across 8 pools, 2,623 curated titles across 16 categories × 9 tones
- **Build targets:** Windows (NSIS), macOS (.dmg), Linux (.deb + .AppImage)

### 3.2 Key Files

| File | Lines | Purpose |
|---|---|---|
| `src/index.html` | 373 | Single-page app: sidebar nav, generator, dashboard panel, settings panel, activation overlay |
| `src/app.js` | 1862 | Desktop UI logic: license gate, background verify, generation (local + AI), dashboard rendering via `invoke()`, settings with API key management |
| `src/styles.css` | 3556 | Full stylesheet (base + desktop-specific: sidebar, activation overlay, engine toggle) |
| `src/logo.svg` | — | Same amber logo as web |
| `src-tauri/src/lib.rs` | 1025 | All IPC commands: generation, history, favorites, projects, settings, license validation, background verify, AI, tier gating. `AppState` = `Mutex<Connection>` + `Mutex<title_gen::Generator>` + `Mutex<Option<LocalLlm>>` |
| `src-tauri/src/engine.rs` | 256 | 3-pass orchestrator: LLM (Pass 1, lazy) → EGCG (Pass 2) → curated fallback (Pass 3). Deduplication + SEO scoring. Passes few-shot examples via `retrieve_similar()`. |
| `src-tauri/src/title_gen.rs` | 1577 | **EGCG algorithm** — 3 modes (exemplar-guided template fill / phrase stitching / keyword-embedded exemplar). `strip_placeholders()` fix for `{placeholder}` leak. Includes `retrieve_similar(keyword, category, k)` for LLM few-shot. |
| `src-tauri/src/local_llm.rs` | 183 | llama-cpp-2 wrapper — `LlamaModel`, `generate_chat_raw()` with batched prefill, `generate_one_clean()` with RAG + retry. Prefers Qwen2.5-1.5B then SmolLM2 fallbacks. |
| `src-tauri/src/seo.rs` | 368 | Local SEO scoring — 9 signals (length, keyword presence/density, search patterns, question, number/year, Flesch reading, power words, uniqueness). Zero API calls. |
| `src-tauri/src/db.rs` | 152 | SQLite schema (8 tables) + seed data import from `seed-data.json` |
| `src-tauri/src/main.rs` | 5 | Entry point → `titleforge_lib::run()` |
| `src-tauri/tauri.conf.json` | 66 | App config, updater endpoint, CSP, bundle config |
| `src-tauri/Cargo.toml` | 37 | Rust dependencies |
| `src-tauri/capabilities/default.json` | 12 | Tauri v2 permissions |
| `seed-data.json` | 1.0 MB | Same as web seed |
| `site/` | — | **Deleted July 29.** Legacy TitleSmith prototype — all content ported to `desktop.html`/`desktop.css`. |

### 3.3 Rust Backend — IPC Commands

`AppState` = `Mutex<rusqlite::Connection>` + `Mutex<title_gen::Generator>` + `Mutex<Option<LocalLlm>>`

**Generation:**
- `generate_titles(keyword, categories, style, genre, quantity, state) -> Vec<TitleResult>` — offline 3-pass pipeline. Tier-capped (Core=25, Pro=100, Studio=500).
- `generate_with_ai(keyword, categories, style, genre, quantity, provider, api_key, cross_medium, include_subtitles, include_translation, translate_lang, gender, finetune)` — BYO cloud AI. **Pro/Studio only.**

**History / Favorites / Projects:**
- `get_history`, `get_favorites`, `toggle_favorite`
- `get_projects`, `create_project`, `delete_project`, `add_to_project`, `update_title_notes` — **Pro/Studio only** for projects
- `record_generation` — writes to `user_history`

**Settings / Meta:**
- `get_categories`, `get_usage_stats` (returns `tier`, `isPro`), `get_app_info`
- `get_settings`, `set_setting` — sensitive keys stored in OS keyring, fallback XOR/SQLite

**License:**
- `validate_license(key, email)` — HTTP → Netlify `/licenses?action=validate&key=...&email=...&machine=...`. 24h offline cache.
- `background_verify(key, email)` — silent refresh, 5s timeout, refresh-only (never revokes)
- `deactivate_license()` — clears license settings

### 3.4 Engine — 2-Pass Pipeline

`engine.rs` orchestrates:

1. **Pass 1 — LLM (lazy).** If model file present and loaded, generate via `local_llm.rs` (Qwen2.5-1.5B, llama-cpp-2, T=0.8 sampling). Fires 50/50, ~96% usable.
2. **Pass 2 — Curated fallback.** Retrieval from 2,623 curated titles for remaining slots, returned as-is (no keyword-swap).

**EGCG was retired from the pipeline July 31 (Task 2)** — 20% usable, mean ~37. `title_gen.rs` still holds `retrieve_similar()` (Qwen few-shot + curated retrieval) and the EGCG generator used only by benchmark tests.

All passes: dedup + SEO score sweep post-generation.

**Scoring:** `raw = 2.0 × avg_pairwise_affinity + 0.5 × ln(1 + unigram_sum) - 1.5 × repeat_penalty` → normalized 0–65 base + heuristic bonuses → capped at 100. *(Legacy EGCG scoring, retained in title_gen.rs for benchmark parity.)*

### 3.5 SEO Scoring (`seo.rs`)

Deduced locally, zero API calls. 9 weighted signals → 0–100:

| Signal | Weight | Method |
|---|---|---|
| Length fit | 20% | Platform-specific sweet spot (Google 50–60 chars, YouTube 48–60, Amazon 60–80). Peak-and-decay. |
| Keyword presence | 20% | Front-loaded (first 3 words) scores higher than tail. |
| Keyword density | 10% | Sweet spot 10–25%. Penalize >40% (stuffing). |
| Search pattern match | 15% | N-gram match against ~110 bundled search modifiers ("how to", "best", "why", "vs", "in [year]"). |
| Question format | 5% | who/what/when/where/why/how/is/are/can/does prefix → bonus. |
| Number/year | 10% | Digit → bonus. Year ±2 of current → extra. |
| Flesch reading ease | 5% | Pure math. Sweet spot 60–80. |
| Power words | 5% | Density from bundled lexicon (shared with EGCG). |
| Uniqueness | 10% | N-gram overlap vs `curated_titles` corpus. Generic → penalty. |

**Output:** `SeoBreakdown { length_fit, keyword_position, keyword_density, search_pattern_hits, is_question, has_number, reading_ease, uniqueness, platform_target }`

**Frontend:** SEO badge pill (green 80+/amber 50–79/gray <50), breakdown panel on hover, dashboard mini-badge in history. **Free web users see the score, Pro sees the breakdown.**

**Tests:** 9 unit tests in `seo.rs`. All passing.

### 3.6 Database (`db.rs`) — SQLite
- **Data path:** `dirs::data_dir() / titleforge-desktop / titles.db`
- **8 tables:** `patterns`, `word_pools`, `curated_titles`, `user_history`, `user_favorites`, `user_settings`, `user_projects`, `project_titles`
- **Seed import:** Reads `seed-data.json`, inserts with `INSERT OR IGNORE`
- **Seed lookup:** `./seed-data.json` (next to binary) or `$DATA_DIR/titleforge-desktop/seed-data.json`

### 3.7 API Key Storage
- **Primary:** OS credential store via `keyring` crate v3 (macOS Keychain, Windows Credential Manager, Linux libsecret)
- **Fallback:** XOR-obfuscated SQLite `user_settings` row (used only when keyring unavailable)
- **Dual-write:** `set_setting` writes to both stores; `get_settings` reads keyring first
- **Sensitive key detection:** Any setting key containing `api_key`, `apikey`, `secret`, `token`, or `password` is routed through keyring
- **Clearing:** Empty value clears both stores

### 3.8 BYO Cloud AI (Desktop)
- **4 providers:** OpenAI (`gpt-4o-mini`), DeepSeek (`deepseek-v4-flash`), Anthropic (`claude-sonnet-4-5`), Google Gemini (`gemini-2.0-flash`)
- **UI mapping:** Dropdown value `anthropic` (was broken as `claude` — fixed July 28)
- **User-managed:** Key entered in Settings → AI Integration, stored via keyring
- **Prompt:** Single quality-rules prompt + style + optional fine-tune injections
- **Engine toggle:** UI button switches Database ↔ AI. Status bar shows provider + key status.

### 3.9 License System
- **Activation:** User enters key + email → `validate_license` → HTTP GET `.../licenses?action=validate&key=...&email=...&machine=<hostname>`
- **Server (`licenses.js`):** Query `licenses` table by key, verify email matches record, verify `is_active`, register machine (max 3), return `{valid, tier}`
- **Offline cache:** On success, stores `license_status=valid`, `license_tier`, `license_validated_at=<RFC3339>`, `license_key`, `license_email` in `user_settings`
- **Cache expiry:** 24h if server unreachable
- **Background refresh:** `startBackgroundTasks()` runs every 30 min; calls `background_verify` (refresh-only, never revokes)
- **UI gate:** `checkLicense()` hides `.nav`, `.hero-compact`, `.tool-section`, `.footer` and shows activation overlay if `license_status != 'valid'`
- **Website license validation** does not burn a machine slot

### 3.10 CI/CD (`.github/workflows/build.yml`)
- **Triggers:** Push to `master`/`main`, `v*` tags, manual dispatch
- **3 parallel jobs:** `build-linux` (ubuntu-22.04, `deb,appimage`), `build-windows` (`nsis`), `build-macos` (`dmg`)
- **Artifacts:** Uploaded per-platform (`titleforge-{linux|windows|macos}`)
- **Release job:** Only on `v*` tag → `softprops/action-gh-release@v2`, downloads all artifacts
- **Signing:** `TAURI_UPDATER_PRIVATE_KEY` + `TAURI_UPDATER_KEY_PASSWORD` repo secrets
- **Node 20** everywhere

### 3.11 Auto-Updater
- **Config:** `tauri.conf.json` → updater plugin, public key `nMmbyRXVNON1KJT3yWIb0m/2xrfNFRPeZGrsRUEMk2I=`
- **Endpoint:** `https://titleforge-tool.netlify.app/updates.json`
- **Permissions:** `updater:default`, `updater:allow-check`, `updater:allow-download-and-install`
- **Status:** Signing key regenerated July 29; `TAURI_SIGNING_PRIVATE_KEY` set as GitHub secret; CI deploys `updates.json` with signatures on tag push. **Untested** — first `v*` tag is the trial.

---

## 4. Frontend Differences: Web vs Desktop

| Aspect | Web | Desktop |
|---|---|---|
| **Layout** | Top nav + scrollable page | Left sidebar (Ink, 220px) + content area |
| **Activation** | Supabase auth modal | Full-screen split-panel takeover |
| **Pages** | `index.html`, `dashboard.html` (separate) | Single page — Generator/Dashboard/Settings are sidebar panels |
| **Auth** | Supabase (CDN + localStorage fallback) | License key (HTTP + offline cache + 30-min background verify) |
| **Tier gate** | Guest / Free / Pro | Core / Pro / Studio — backend enforces, frontend reads `currentTier` from `get_usage_stats` and renders actual tier (fixed July 29) |
| **Data source** | Supabase via Netlify Functions | SQLite via `invoke()` |
| **Generation** | Cloud AI only | 3-pass local (LLM → EGCG → curated) OR BYO cloud AI |
| **Favorites/Projects** | Supabase tables | Local SQLite |
| **Floating generator** | Yes (FAB) | No |
| **Engine toggle** | No | Yes (Database / AI) |

---

## 5. Change Log (Rolling)

### 2026-07-31 (late night) — Task 0 complete: Qwen non-deterministic + temperature sweep

**Task 0 (make Qwen non-deterministic) — DONE.** Replaced both argmax loops in `generate_chat_raw` with `sample_token()`: top-k 40, softmax temperature, inverse-CDF sampling via `rand::thread_rng()`.

- **Determinism bug fixed:** same keyword now yields different titles every call (test `qwen_non_deterministic.rs` asserts ≥2 unique from 5 runs — got 5/5 unique for "coffee").
- **EOS handling corrected vs the brief's sketch:** the brief's `sample_token` filtered EOS entirely, which leaked `<|im_start|>` junk at max_new=60 (verified in smoke test). Fixed: EOS stays sampleable at continuation positions; decode loop breaks when EOS is sampled. EOS is banned only at position 0 (added to `banned_first` at load).
- **Performance:** echo-prefix ban list precomputed ONCE at load into `HashSet<LlamaToken>` (ID compare) instead of `token_to_str` on all 151,936 candidates per token. Natural EOS termination also cut generation time (~20s → ~8s per title in smoke test).
- **Sweep harness:** `TF_LLM_TEMP` env override (no rebuild between runs); `BENCH_ENGINE=qwen` filter (one-variable runs, zero cloud API calls).

**Temperature sweep (50 keywords, Qwen only, all re-judged fresh):**

| T | Fires | Usable ≥70 | Mean | Verdict |
|---|---|---|---|---|
| **0.6** | 50/50 | **100%** (50/50) | 83.7 | Quality matches argmax but titles ≈ deterministic ("Revolutionize Your Workstation with the Ultimate Laptop" identical to old run) — defeats Task 0's purpose |
| **0.8** | 50/50 | **96%** (48/50) | 81.0 | Visible diversity ("Code Breaks the Heart" for love; "PowerUp Your Tech with the Ultimate Laptop!"). 2 misses: a brand-slogan-style shirt title (22) and a redundant 'future' title (42) |
| **1.0** | 50/50 | **84%** (42/50) | 76.6 | Too hot: "Your Perfect Castorama", "Mornings Get Coffee Kick", "Crashing Mystartup Dreams" — 8 failures |

**Decision: T=0.8 (the current default).** Brief's rule: "pick the highest temperature that keeps usable ≥ 95%". 0.8 gives 96% + real diversity; 1.0 fails the gate; 0.6 is deterministic in disguise. Mean 81.0 vs 83.7 at argmax is the accepted cost of variety. If batch diversity proves insufficient, revisit 0.9 with a fresh run — one variable at a time.

**Success gate status:**
- [x] Same keyword twice → different titles (5/5 unique)
- [x] Qwen usable ≥ 95% (96% at T=0.8)
- [x] Qwen mean ≥ 78 (81.0)
- [x] `cargo test --release --lib` 19/19
- [x] Sweep table recorded here

**Note on Qwen usable rate:** the fixed benchmark (keyword gate removed) shows Qwen at 100% usable at k=1; at T=0.8 with sampling it is 96%. Both exceed the 95% bar.

### 2026-07-31 (late night, later) — Task 0b complete: real 25-title batch measured for the first time

**First real batch through the production path.** `tests/batch_measure.rs` calls `engine::generate` (the exact path the app uses) for `coffee` × 25 — the Core tier promise.

| Metric | Before Task 0 | After Task 0 |
|---|---|---|
| Unique titles / 25 | **1** (deterministic) | **25** |
| Wall clock | ~3 min (estimate, unbounded) | **169.6s (6.79s/title)** |
| Source mix | — | 100% local-llm (zero EGCG/curated fallback) |

- All 25 titles keyword-compliant, grammatical, publishable.
- **New gap found: template diversity within a batch is weak.** 7/25 titles use the "From X to Y" structure ("From Bean to Cup: ..." variants); several share "A Journey Through"/"Uncovering the..." frames. Uniqueness is solved; *formula* repetition is the next problem.
- This is a prompt-quality-rules issue, not a sampling issue: the desktop system prompt is one sentence, while the web prompt has "No two titles may share their opening three words. No two titles may use the same structural template." → **Task 1 (port web quality rules) directly targets this.**

**Implication for the timing table (was §6.2 #0b):** at 6.79s/title with the *current* single-context-per-call design:

| Tier | Titles | LLM calls (×2 for dedup) | Wall clock |
|---|---|---|---|
| Core | 25 | ~30-50 | **~2.8-5.7 min** (measured 169.6s) |
| Pro | 100 | ~200 | ~22 min |
| Studio | 500 | ~1000 | ~110 min |

Core is acceptable. Pro/Studio are still product problems — surface to user before optimising. Context reuse across a batch (allocate one KV cache, reuse for all titles in a category) is the obvious first win, but is NOT needed for Core.

### 2026-07-31 (late night, later) — Task 1 result: quality rules REVERTED. Qwen 1.5B can't follow multi-constraint prompts.

**Task 1 (port web QUALITY RULES to desktop prompt) was attempted and REVERTED.** The brief's success gate: "Qwen mean rises above 83.7 with usable at or near 100%. Cliché count drops materially. If the mean falls, revert."

**Two variants tested, one variable at a time (both at T=0.8, BENCH_ENGINE=qwen):**

| Variant | Usable | Mean | Cliché titles |
|---|---|---|---|
| **Baseline (one-sentence prompt)** | **96%** (48/50) | **81.0** | ~25/50 |
| Full 6-rule block (verbatim from generate.js) | 80% (40/50) | 75.2 | 21/50 |
| Condensed 3-line rules | 90% (45/50) | 77.6 | 27/50 |

**Both rule variants measured WORSE.** Mean fell 75.2 / 77.6 vs baseline 81.0. Cliché count did NOT drop — the model used "Unlock/Unleash/Secrets" 27 times despite an explicit ban. 5 titles scored <70 including "Finding Your Noddy: A Journey Through Parenting's Mysteries" (25) and "Negotiating With Shadows" (42) — the rules pushed the model into over-creativity that drifted off-topic.

**Root cause: model capacity, not prompt quality.** Qwen 1.5B cannot hold a multi-constraint instruction ("MUST contain keyword" + specificity + curiosity gap + no-clichés + variety) simultaneously. Each added rule dilutes the others. The SAME 6-rule block works on DeepSeek V4 Flash (98-100% usable, mean ~90) — the web app benefits because its model is ~100x larger. Prompt engineering has hit the 1.5B ceiling.

**Also tested and reverted (same session):** softening "MUST contain keyword" → "clearly about X" + relaxing the attempt-1 QC filter. Made it worse (off-topic drift: investing → "High-Retention Fundraising" scored 12). The strict keyword instruction + strict QC filter stay.

**Decision: keep the original one-sentence prompt.** It scores 81.0 mean / 96% usable — the best Qwen 1.5B can do with this prompt engineering. The path to web-level quality is a bigger model (Qwen2.5-3B, ~2 GB), NOT more prompt rules. **Template diversity within a batch (7/25 "From X to Y") remains unsolved for offline** — but the fix is a model upgrade, not a prompt tweak.

**Files changed then reverted:** `local_llm.rs` (system prompt, keyword QC), `diag_prompt_len.rs` (prompt mirror). `bench-usability-task1-regression.csv` kept as evidence (2 runs: full-rules 75.2, condensed 77.6).

**Task 1 status: CLOSED as not-shippable.** Recorded in §6.2 #1 (replaces "port web quality rules" — the plan was wrong for this model size). The n_ctx=1024 bump was NOT reverted — it's harmless and future-proof (prompts were 351-405 tokens at the old 512 window, dangerously close with 60 new tokens; the bump to 1024 costs ~nothing for CPU inference).

### 2026-07-31 (late night, later) — Task 2 complete: EGCG retired from the production pipeline

**Pass 2 (EGCG generation) removed from `engine.rs`.** The pipeline is now Qwen (Pass 1) → curated retrieval (Pass 2, instant fallback + batch top-up). Rationale: EGCG measured 20-24% usable on the corrected metric (mean ~37) — it produced output 98% of the time and garbage 80% of the time. Qwen now fires 50/50 at ~96% usable, so EGCG's only reason for existing (batch fill) is gone.

- **`title_gen.rs` NOT deleted** — it holds `retrieve_similar()` (Qwen few-shot + curated retrieval) and the EGCG machinery that benchmark tests still compare against (`bench_judge.rs`, `bench_path_a.rs`, `egcg_sanity.rs`). EGCG stays as a benchmark column, not a production engine.
- **Verification:** `cargo test --release --lib` 19/19. `egcg_sanity` still passes (196 titles, 0 placeholder leaks — title_gen.rs intact). Batch measurement confirms the new pipeline: 25/25 unique, 100% local-llm, zero EGCG/curated fallback needed.
- **New pipeline:** `engine.rs` = LLM pass + curated fallback only. Benchmarks unchanged (EGCG column retained for regression comparison).

### 2026-07-31 (night, later) — Qwen's sampler is deterministic. Batch generation cannot work.

**Found while assessing readiness to bundle the model. This blocks bundling.**

**The problem:** `generate_chat_raw` samples with pure argmax — no temperature, no top-k, no top-p:
```rust
if cd.logit() > best_logit { best_logit = cd.logit(); best_tok = tok_id; }
```
Same prompt → same title, deterministically, for every user and every call.

**Evidence:** two independent full benchmark runs produced byte-identical titles.

| Keyword | Both runs |
|---|---|
| laptop | "Revolutionize Your Workstation with the Ultimate Laptop" |
| fitness | "Unlock Your Fitness Potential: The Ultimate Guide to Achieving Your Goals" |
| tennis | "Mastering the Game: The Ultimate Guide to Tennis" |
| bitcoin | "Unleashing the Future: Bitcoin Revolutionizes Finance" |

**What this means:** the **100% usable figure is 100% usable at k=1.** The product sells 25 / 100 / 500 titles per batch. A user requesting 25 titles for "coffee" receives the same title 25 times, which dedup in `engine.rs` collapses to one. This is the same structural failure already documented for curated retrieval (§6.2) — it was simply not suspected of a language model.

**Compounding issue — batch generation has never been measured.** [engine.rs:35](titleforge-desktop/src-tauri/src/engine.rs:35) loops `target_per_cat * 2` LLM calls to allow for dedup. At the measured 3.5 s/title:

| Tier | Titles | Worst-case calls | Wall clock |
|---|---|---|---|
| Core | 25 | 50 | ~3 min |
| Pro | 100 | 200 | ~12 min |
| Studio | 500 | 1000 | **~58 min** |

Every benchmark to date is k=1. No real batch has ever been run.

**Secondary inefficiency:** `generate_chat_raw` calls `self.model.new_context()` on every invocation, so a 25-title batch allocates and destroys 25 KV caches. Reusing one context across a batch is a straightforward win once batching is addressed.

**Required before bundling the model (see AI-WORK-BRIEF §4 Task 0):**
1. Temperature + top-k sampling, then re-benchmark — quality at temperature ≠ quality at argmax
2. Measure a real 25-title batch for uniqueness and wall-clock time
3. Cross-platform verification — Qwen has only ever been built and run on this Windows machine; macOS and Linux are unverified
4. Confirm redistribution terms for the specific GGUF quant (Qwen2.5 base is Apache 2.0; the quantised file is third-party)
5. Then choose delivery: 986 MB in-installer (22 MB → ~1 GB) vs first-launch download

**Do not bundle before item 1 lands.** A deterministic generator cannot fulfil the core product promise regardless of per-title quality.

### 2026-07-31 (night) — Qwen's "68% empty output" was a buffer bug. Offline is now 100% usable at k=1.

**Root cause found and fixed. Every prior conclusion about Qwen's capability was measuring a defect, not the model.**

**The bug:** [local_llm.rs](titleforge-desktop/src-tauri/src/local_llm.rs) ended `generate_chat_raw` with a single deprecated call:
```rust
#[allow(deprecated)]
let result = self.model.tokens_to_str(&gen_tokens, Special::Tokenize).ok()?;
```
`tokens_to_str` sizes its internal buffer too small and returns `InsufficientBufferSpace`. `.ok()?` swallowed it and returned `None`. **The model generated the title successfully every time; the string conversion threw it away.**

**Evidence (`TF_LLM_DIAG=1` trace over the 23 silent keywords):** 13 of 14 traced calls bailed at that line with `InsufficientBufferSpace(-9)` / `(-10)`. Prompts were 100–166 tokens against a 512 window — no overflow. First-token sampling was healthy: 151,936 candidates, only 114 banned, best_logit ≈ 25. Generation was fine end-to-end until the final conversion.

**The fix:** decode token-by-token into a byte buffer, then convert once.
```rust
let mut buf: Vec<u8> = Vec::with_capacity(gen_tokens.len() * 4);
for &t in &gen_tokens {
    if let Ok(b) = self.model.token_to_bytes(t, Special::Tokenize) { buf.extend_from_slice(&b); }
}
let result = String::from_utf8_lossy(&buf).to_string();
```
Bytes rather than per-token `String`s: BPE tokens can split a multi-byte UTF-8 character, so per-token decoding would corrupt non-ASCII output.

**Also fixed:** both `batch.add()` calls were discarding their `Result` (live compiler warnings). Now handled and traced.

**Result — full 4-engine re-benchmark:**

| Engine | Fires | Mean | **Usable ≥70** | Change |
|---|---|---|---|---|
| Cloud (DeepSeek) | 50/50 | 89.5 | 98% | — |
| **Qwen2.5-1.5B** | **50/50** | **83.7** | **100%** | **was 52%** |
| Curated | 37/50 | 76.8 | 62% | — |
| EGCG | 49/50 | 38.5 | 24% | — |

Qwen distribution: min 72, p25 78, median 85, max 92. **Zero titles below the 70 threshold.** 23/23 previously-silent keywords recovered.

**Qwen now matches cloud on usable rate (100% vs 98%) and trails by ~6 points on mean.** Offline is a defensible product claim, with no GPU, no larger model, no fine-tune, and no install-size change.

**Superseded claims — do not cite:**
- "1.5B cannot respond to most inputs" — it responded to all of them
- "68% empty-output rate is the model ceiling" — plumbing defect
- "Fire rate is the constraint; needs Qwen-3B or a LoRA fine-tune" — neither is needed
- Every Qwen percentage recorded before this entry

**Remaining quality gap (next work, not a blocker):** the desktop prompt is one sentence and lacks the web prompt's six QUALITY RULES — notably the cliché blocklist. 25/50 Qwen titles use banned vocabulary ("Ultimate", "Unlock", "Unleash", "Revolutionize"). Measured impact is small (cliché titles mean 83.3 vs clean 84.0) but they cluster in the bottom quartile (72–75). Porting `generate.js`'s quality rules into `local_llm.rs` should lift the floor.

**Diagnostics added (read-only, no production behaviour change):** `tests/diag_qwen_silence.rs` attributes each failure to a specific filter; `tests/diag_prompt_len.rs` checks context-window overflow without generating; `TF_LLM_DIAG=1` traces all bail-out points A–L in `generate_chat_raw`.

### 2026-07-31 (late) — Benchmark metric fixed. ALL PRIOR ENGINE NUMBERS WERE WRONG.

**Two bugs found in the benchmark harness. Both fixed. Benchmark re-run. The strategic picture changed.**

**Bug 1 — `keyword_present` was gating the judge.** `mech_pass = is_readable && keyword_present` meant any title lacking the *literal* keyword token was scored 0 without ever reaching the judge. A string match cannot know that "VR" ≈ "virtual reality", "100 Workouts" ≈ fitness, "Meditate" ≈ meditation, or "Freelancer" ≈ freelancing. Fixed: gate on readability only; keyword relevance is left to the judge, whose rubric already penalises off-topic titles. Literal presence is retained as a new advisory `kw_literal` CSV column.

**Bug 2 — `.bench-key` was never actually read.** The file is UTF-16LE with a BOM (PowerShell's `echo key > file` default on Windows). `read_to_string` requires UTF-8, returned `Err`, and the code silently fell through to "API key not set." The reader now decodes UTF-16 LE/BE, UTF-8 BOM, and plain UTF-8, and reports loudly when a present file can't be decoded. The banner instructions that caused this were corrected to `Set-Content -Encoding utf8 -NoNewline`.

**Bug 3 — the benchmark's cloud prompt did not match production.** It carried an 8-example few-shot block that the web app had already reverted, so "cloud = our ceiling" measured a prompt we do not ship. Replaced with the production QUALITY RULES from `generate.js`.

**Corrected results (50 keywords × 4 engines, all re-judged):**

| Engine | Fires | Mean (judged) | **Usable ≥70** | Literal keyword present |
|---|---|---|---|---|
| **Cloud (DeepSeek)** | 50/50 | **~90** (σ 4.9) | **98-100%** | 35/50 |
| Curated | 37/50 | 76.8 | 62% (31/50) | 37/50 |
| Qwen2.5-1.5B | 27/50 | 81.8 | 52% (26/50) | 23/50 |
| EGCG | 49/50 | ~37 | 20% (10/50) | 49/50 |

Reproduced across two independent full runs. Cloud scored 100% (mean 89.8) and 98% (mean 90.4); curated and Qwen were identical both times (deterministic retrieval / cached judge scores); EGCG held at 20% with mean 36.7–37.5 (it samples stochastically). `cargo test --release` — 23/23 pass.

**The finding that matters: literal keyword presence is inversely correlated with quality.** Titles *without* the literal keyword (n=19, all engines) average **88.4** and are **100% usable** — the best titles in the entire dataset. Examples the old gate scored 0: "The 2-Hour Rule: How to Do Less and Achieve More" (productivity, 92), "The $10,000 Photo Mistake Beginners Make" (photography, 92), "The Silent Song That Screams" (music, 92). Forcing literal keyword inclusion actively degrades output.

**What this changes:**

1. **Cloud is at 100% usable, mean 89.8, σ 4.9.** The web app is performing essentially at ceiling and is far stronger than any prior entry in this log claimed. The previously-recorded "62%" and "68%" ceilings were pure metric artefact.
2. **The offline gap is much wider than believed** — 100% cloud vs 62% curated vs 52% Qwen. Cloud-first positioning for desktop Pro/Studio is better supported than "close the gap with a fine-tune."
3. **The Task 3 few-shot revert must be re-tested.** It was reverted because it "poisoned keyword compliance" — measured by the broken gate. Few-shot teaches natural, creative titles, which is exactly what the broken metric punished. The revert may have removed a genuine improvement.
4. **Qwen improved to 52%** (logit biasing + gate fix), 96% usable when it fires. Fire rate 27/50 remains the constraint.
5. **EGCG confirmed at 20%** with the highest literal-keyword rate (49/50) and the worst quality. Retire decision stands and is strengthened.

**Superseded:** every engine percentage recorded in this log before this entry. Do not cite them.

### 2026-07-31 — Sprint complete: 7 tasks, all engines benchmarked, final decision
> **SUPERSEDED by the entry above** — the numbers in this entry were produced with the broken keyword gate and an unshipped cloud prompt. Retained for history only.


**Task 1 (web fix):** `frequency_penalty 0.6→0.15`, `presence_penalty 0.4→0` in `generate.js`. The old penalties were suppressing the required keyword in large batches. Also unified Anthropic temperature to `0.85` (was `0.7`). Strengthened variety rule: "no two titles may share opening 3 words or structural template."

**Task 2 (cloud benchmark):** Added `generate_title_cloud()` to `bench_judge.rs` — DeepSeek with web app's prompt format. First measurement of our quality ceiling.

**Task 3 (few-shot attempt):** Injected 8 curated titles (score 100 each) into web prompt's standard mode. Beautiful titles, taught model to write better — but **poisoned keyword compliance.** Reverted same session.

**Task 4 (A/B verify):** Mirrored benchmark cloud prompt to web app. Enabled before/after comparison.

**Task 5 (punctuation fix):** `keyword_present()` in `bench_judge.rs` stripped punctuation before matching. Rescued ~7-10 wrongly-rejected titles (colon-glued words, apostrophes). All engine numbers were understated.

**Task 6 (logit biasing):** First-token echo suppression on Qwen via `ctx.candidates()` + `token_to_str()`. Bans tokens starting with: here, sure, cert, please, write, note, based, using, ```. Smoke test confirmed: "Revolutionize Your Morning with Coffee" instead of "Here is a title..."

**Task 7 (final benchmark + decision):**

Full 4-engine comparison with all fixes applied:

| Engine | Usable (≥70) | Mean | Mechanical | Verdict |
|--------|-------------|------|------------|---------|
| Cloud (DeepSeek) | **62%** (31/50) | 83.2 | 33/50 | Quality ceiling (was 68% before few-shot poisoned it) |
| Curated | **62%** (31/50) | 76.8 | 37/50 | Best offline — but batch-limited (median 2/keyword) |
| Qwen2.5-1.5B | **44%** (22/50) | 81.1 | 23/50 | Logit biasing: +37.5% fire rate. 96% usable when fires |
| EGCG | **6%** (3/50) | 44.8 | 49/50 | **Dead. Produces output 98% of time, garbage 94%. Retire.** |

**Key findings from the sprint:**
- Cloud at 68% (pre-few-shot) = proven quality ceiling with quality rules only
- Few-shot examples: better writing, worse instruction-following. **Reverted from production.** Category error — examples that don't contain a user keyword teach the wrong lesson
- Logit biasing on Qwen: **proven win** — 32%→44% usable, 46% fire rate, 96% of fired titles publishable
- EGCG confirmed irredeemable at 6% — should be removed from the pipeline
- Qwen fire rate gap (54% silent) is the remaining desktop engine problem

**Sprint decisions:**
1. **Revert few-shot from web** — keyword compliance > creative quality. The quality rules alone deliver 68%.
2. **Keep logit biasing** — proven +37.5% improvement on Qwen fire rate.
3. **Retire EGCG** — 6% usable is unacceptable for any tier, let alone a fallback.
4. **Keep Qwen as primary local generator** — 44% usable, excellent when it fires. Next step: Qwen2.5-3B or more aggressive biasing to close the 54% silent gap.
5. **Curated as quality fallback** — 62% usable, best offline quality, but single-title only (batch = repeat).

**What was NOT done:**
- Qwen2.5-3B trial — the obvious next step to improve fire rate
- Keyword-swap on curated titles — would push 62%→~90% usable but still single-title
- Fully retire local LLM for cloud-first — Qwen at 44% is good enough to ship, not good enough to be the only engine

**New sprint order (original July 31 brief):** Web first (revenue-generating surface) → desktop engine. Tasks: fix sampling penalties ✅ → benchmark cloud AI ✅ → add few-shot to web prompt ✅ (then reverted) → A/B verify ✅ → fix benchmark punctuation ✅ → logit biasing on Qwen ✅ → re-benchmark and decide ✅.

### 2026-07-31 — Web prompt fix + cloud AI benchmark + sprint reorder (earlier session)

### 2026-07-31 — Benchmark audit, engine roles corrected, web prompt reviewed

**Benchmark v2 audit (verified against raw CSV, not agent summary):**
- Reported numbers are accurate. Qwen 32% usable, EGCG 20%, Curated 58%. Confirmed row-by-row.
- **BUG FOUND — punctuation-blind keyword check.** [bench_judge.rs:58-63](titleforge-desktop/src-tauri/tests/bench_judge.rs:58) matches `" keyword "` with literal spaces, so `"Versatile Shirt: Perfect for Any Occasion"` fails the check for `shirt` (title has `"shirt:"`). **9 titles wrongly rejected and never judged — 5 of them Qwen.** Qwen favours the `"Keyword: Subtitle"` format, which is a *good* format. Corrected estimates: Qwen ~42%, Curated ~62%, EGCG ~24%.
- **WRONG DIAGNOSIS CORRECTED.** Prior entry claimed curated's gap was "keyword absence — many curated titles are simply not about the user's keyword." False. Keyword presence in returned curated titles is **37/37 = 100%**. The gap is **13 empty retrievals** (62% of curated failures), 2 punctuation-bug rejections, 6 genuine quality misses. `retrieve_similar()` requires literal token overlap (`overlap > 0`) and returns nothing when the corpus has no match. Keyword-swap cannot fix a title that was never retrieved.

**Curated cannot be the primary engine — corpus depth measured:**
- Median **2 titles available per keyword**. `fitness` → 1. `travel` → 6. `coffee` → 9. `laptop`/`shirt`/`bitcoin`/`tennis`/`jazz`/`cooking` → **0**.
- Of 50 benchmark keywords: 37 can fill 1 title, **8 can fill 10**, **1 can fill 25** (Core promise), **0 can fill 100** (Pro), **0 can fill 500** (Studio).
- The 58% usability score was measured at `k=1`. It does not survive contact with a real batch request.
- `retrieve_similar()` is fully deterministic — sorts by score, takes top-k, no randomness. **Two users typing the same keyword get identical titles. The same user regenerating gets identical titles.** It is a search index over 2,623 fixed strings, not a generator.

**Engine roles reassessed:**
- **Qwen is the only true generator.** 16/16 = **100% usable when it fires**, fires 32%. That is a *recall* problem, not a *quality* problem — and recall is the cheaper fix. Logit biasing, not fine-tuning, is the lever.
- **Curated** is excellent as few-shot examples for Qwen and as a "top pick" garnish. Not viable for batch.
- **EGCG** is the only engine that can currently fill a 25-title batch, and it is 20% usable. That is the actual product problem.
- **Negative result:** tested whether Qwen's empty outputs correlate with missing few-shot examples. **They do not** — Qwen fired 61% when curated had no examples vs 51% when it did. Corpus expansion will NOT fix Qwen's fire rate. Theory rejected.

**Fine-tune reality check:**
- Dev machine has **Intel Iris Xe integrated graphics, no CUDA GPU**. Local LoRA training is not viable.
- Path B is ~85% agent-automatable; the training step is a hard human gate (GPU provisioning + payment).
- **Nothing reaches 100% usable.** Realistic ceilings: logit-biased Qwen ~55-70%, fine-tuned Qwen ~70-85%, cloud AI ~85-95%. Target 80%, not 100%.
- **Fine-tuning is not the biggest available win.** Qwen's quality is already excellent; only its fire rate is broken.

**Web app prompt reviewed ([generate.js](titleforge/netlify/functions/generate.js)) — 7 findings:**
1. **`frequency_penalty: 0.6` + `presence_penalty: 0.4` actively fight the keyword requirement.** These penalise tokens the more they appear. Generating 25-100 titles that must all contain one keyword means that keyword gets progressively suppressed. Directly contradicts the prompt's own "every title must be about this keyword." Worsens with batch size.
2. **No few-shot examples.** The prompt describes good titles but never shows one. `titleforge/seed-data.json` holds the same 2,623-title corpus the desktop app already uses for RAG few-shot. Unused on web.
3. **Self-scoring is inflated.** Model writes and scores in one pass. Direct evidence: EGCG self-scored 60-100 on titles the independent judge scored 15-30. The 0-100 appeal score is a headline Pro feature and is likely 15-25 points optimistic.
4. **Style descriptions circular and example-free.** `shout: 'high-impact words that shout'` defines shout with shout. 9 styles, 5 gated behind Pro, zero examples.
5. **No per-category length targets in standard mode.** Cross-medium has them; standard doesn't. `seo.rs` scores length-fit that generation never targets.
6. **Temperature inconsistent across providers** — 0.85 OpenAI-compatible, 0.7 Anthropic.
7. **Breakdown fields compete for attention** — 5 fields × N titles, produced in the same pass as the titles themselves.

**Strategic reprioritisation:** web app work now leads. Users are paying for the web product today; the desktop offline engine is not yet shipping quality. Fixing the sampling penalties is a two-line change with the largest expected quality gain in the codebase.

**Tier promise problem:** No local engine can deliver 500 titles. Even a fixed Qwen at 3.5s/title makes 500 a ~30-minute operation. The Studio claim at [desktop.html:453](titleforge/desktop.html:453) needs to change to a non-numeric promise. Same class of issue as the previously-fixed "unlimited batch" claim.

### 2026-07-30 (end of day) — Benchmark v2 complete: LLM-judge usability scores
- **`bench-usability.csv` produced** — 150 titles (50 keywords × 3 engines) scored 0-100 by DeepSeek V4 Flash on "would a real creator publish this without editing?" Results:

| Engine | Mean | Non-Zero | Usable (≥70) |
|--------|------|----------|---------------|
| Qwen2.5-1.5B | 81.9 | 16/50 (32%) | **16/50 (32%)** |
| EGCG | 47.6 | 47/50 (94%) | **10/50 (20%)** |
| Curated retrieval | 76.3 | 35/50 (70%) | **29/50 (58%)** |

- **Qwen assessment:** When Qwen fires, it's genuinely good (mean 81.9, samples like "Raising Emotions: A Guide to Parenting Through the Ages" scored 75). But 34/50 keywords produce nothing — the model fundamentally cannot respond to most inputs. 32% usable rate is above the 25% "walk away" threshold in the brief, but 68% empty-output rate makes it unreliable as a primary engine.
- **EGCG assessment:** Produces output 94% of the time (only 3 completely empty), but only 20% is usable. The mean score of 47.6 reflects the gibberish rate — scores range from 15 ("The Hitchhiker's Parenting Woe and Kibble") to 85 ("10 Things Nobody Tells You About Bitcoin"). The results-pool fragment problem is now confirmed by data, not anecdote.
- **Curated assessment:** Best engine at 58% usable (mean 76.3). All titles are human-written quality. The 42% gap is from keyword absence — many curated titles are simply not about the user's keyword. Keyword-swap would close this gap.
- **Core finding:** No single engine clears 60% usable. The brief says "both below 60% = local generation at this size is a dead end." Three paths: try Qwen2.5-3B (~2 GB), keyword-swap on curated titles to push 58%→90%+, or retire local LLM and go cloud-first.
- **Technical note:** DeepSeek V4 defaults to thinking mode. First benchmark run silently returned all zeros because `max_tokens:10` was spent on chain-of-thought with no score output. Fixed by adding `"thinking": {"type": "disabled"}` and `max_tokens:64`.

### 2026-07-30 (evening) — Grammar attempt failed, priorities reordered
- GBNF via `LlamaSampler` failed cleanly (see prior entry, kept for the record). Root cause: `llama-cpp-2 0.1.153`'s `sampled_token_ith(i)` requires the batch position `i` to have been decoded with `logits: true`, but the prefill batch in `generate_chat_raw` only marks the last token. The sampler API can't reach the constrained candidates. Reverted, no regression.
- **New priority order (mandatory):** Task B (LLM-judge benchmark, §7.2) MUST run before any more Task A work. The current mechanical bench cannot distinguish "48% Qwen good" from "48% Qwen garbage." Making a keep/retire decision on that metric is flying blind. This overrides §7.2's prior ordering.
- **Preferred grammar approach going forward:** logit biasing via `ctx.candidates()` — ban the specific token sequences that spell "Here is", "Sure!", "As a", markdown fences, etc. Uses working APIs, kills the exact failure mode we see, avoids the sampler compatibility wall. Full GBNF via `LlamaSampler` is not the path.
- **What was preserved from the attempt:** stronger system prompt (keyword as CRITICAL RULE), relaxed retry QC (strict attempt 1, accept coherent on retries), greedy argmax via `ctx.candidates()`. Left in `local_llm.rs`.

### 2026-07-30 (later) — GBNF grammar attempted, bottleneck identified
- **GBNF grammar attempted via `LlamaSampler::grammar()`** — `sampler` feature enabled on llama-cpp-2, grammar sampler attached via `new_context_with_samplers()`. Reverted after finding the `sampled_token_ith()` API unreliable for reading grammar-constrained tokens. Error: "batch.logits[i] != true" regardless of index approach (output index 0, batch position, context position).
- **Bottleneck confirmed:** llama-cpp-2's `LlamaSampler` backend-sampler mode doesn't play well with `sampled_token_ith()`. GBNF grammar would need either: (a) `json_schema_to_grammar()` + passing the grammar string differently, (b) llama.cpp C FFI directly to create grammar objects, or (c) a different Rust crate (`llama-gguf` which has simpler grammar API).
- **What was preserved:** stronger system prompt (keyword as CRITICAL RULE), relaxed retry QC (strict on attempt 1, accepts any coherent output on retries), greedy argmax via `ctx.candidates()`.
- **`sampler` feature removed from Cargo.toml** — not needed without grammar. `cargo check` clean.

### 2026-07-30 — Benchmark evidence + usability directive
- 50-keyword benchmark ran (`titleforge-desktop/bench-results.csv`, [tests/bench_path_a.rs](titleforge-desktop/src-tauri/tests/bench_path_a.rs)). Results: **Qwen 48% "Good" (24/50), EGCG 98% (49/50)**. Twenty-three Qwen outputs are empty strings — retry loop in `local_llm.rs::generate_one_clean` gives up when Qwen echoes instructions ("Here is a title:...") 3 times.
- **The current benchmark is not fit for the decision.** Its "Good" metric only checks format + keyword presence — a template-garbage title like "The Peak Truth About Laptop" scores "Good." Users are paying for **usable** titles (grammatically clean, on-topic, clickable, publish-worthy), not titles that mechanically pass a heuristic. Confirmed by §6.2 #1 self-review: EGCG's 98% mechanical pass rate hides ~75% garbled or nonsensical output. Benchmark v2 spec added in §7.2.
- **Usability bar (canonical):** A title is usable if a human creator would publish it without editing. Format-conformant garbage does not count.
- **New sprint plan:** GBNF grammar constraints first (jumps Qwen 48%→~90% by killing empty-output/echo failures), then LLM-judge quality eval (turns pass/fail into 0-100 quality score), then decide Qwen vs EGCG on real quality. See §7.2.

### 2026-07-30 — Audit corrections to CONTEXT.md
- §3.2 line counts corrected against source (`local_llm.rs` 179→184, `engine.rs` 293→256, `title_gen.rs` 1533→1577).
- §6.3 Path A status softened from "SHIPPED" → "Implemented — pending benchmark." Path A code is done and works; it is not yet the primary engine and hasn't been quality-benchmarked.
- §7.1 corrected: `candle-*`/`tokenizers` crates are dead deps (kept in `Cargo.toml`, imported by nothing). SmolLM2 GGUF files are kept as fallbacks.
- New §6.2 items #13 (unguarded `eprintln!` in release), #14 (dead candle deps), #15 (Qwen bundling decision).
- Prior 2026-07-25 change-log entry noting PostHog is historically anachronistic — Plausible was chosen 07-25, swapped to PostHog later. Leaving as-is (not rewriting history further); mentioned here for the record.

### 2026-07-30 — Qwen benchmark + honest quality assessment
- **50-keyword benchmark completed** — Qwen vs EGCG vs Curated, 50 keywords across 16 categories. Results in `bench-results.csv`.
- **Qwen2.5-1.5B pass rate: 46%** (23/50) on keyword-match QC. When it succeeds, output is excellent: "Revolutionize Your Morning with Coffee: 7-Day Coffee Detox Plan." The 27 failures are empty output — Qwen produces nothing for those keywords. This is the 1.5B model ceiling, not a prompt engineering issue.
- **EGCG pass rate: 98%** on technical checks, but ~75% of titles are semantically garbled ("From Dawn to That move the needle: My Journey to Imagineing Negotiation", "Where the Light Minimalism Death of Romance", "Beyond Meditation: The Next of"). The `{placeholder}` leak is fixed but the `results` pool entries ("That move the needle", "That make a difference") produce gibberish when used as standalone slot fills.
- **Curated pass rate: 74%** — always human-written quality, but titles don't contain user keywords (no keyword substitution implemented).
- **No engine produces 100% usable titles.** Combined pipeline (Qwen creative + EGCG fill + curated quality) is the current architecture. Quality engine work remains open.
- **Security audit fixed:** CSP enabled in `tauri.conf.json` (was null), `crypto.randomBytes()` for license key generation, `crypto.timingSafeEqual()` for secret comparison, graceful LLM init failure (no panic), `eprintln!` guarded behind `#[cfg(debug_assertions)]`.
- **Performance fixed:** DB mutex scoped — released before LLM inference (was held 90s+ blocking all IPC). Batched prefill optimization. Reusable decode batch.
- **Code review fixed:** Extra `</div>` breaking download page grid, duplicate `action` param breaking license verification URL, category filter uses IDs not labels, `isPro` defaults to `false`, tier defaults `basic`→`core`, `urlencoding()` handles multi-byte UTF-8, slider max reapplied after stats load.
- **19/19 tests pass.** `cargo check` clean.

### 2026-07-29 (later) — Path A LLM COMPLETE + batch prefill optimization
- **llama-cpp-2** compiled on Windows with LLVM + CMake + MSVC Build Tools installed.
- **Qwen2.5-1.5B-Instruct** (Q4_K_M, 940 MB) replaces SmolLM2 as the local LLM. Candle-rs (candle-core, candle-transformers, candle-nn, tokenizers) **still present** in Cargo.toml for the SEO engine and EGCG.
- **Batched prefill** optimizes generation: 3.5s/title (was 5.4s one-at-a-time, 35% faster).
- First generated Qwen title: "Revitalize Your Day with Coffee: 7 Minute Coffee Cure" — keyword match, creative, 1st attempt.
- RAG few-shot + retry + post-cleaning pipeline preserved from SmolLM2 work.
- `retrieve_similar()` in `title_gen.rs` feeds curated titles as few-shot examples.
- Model preference: Qwen2.5-1.5B → SmolLM2-360M → SmolLM2-135M (first found wins).
- Built on this machine with CMake 4.4.0, LLVM 22.1.8 (clang-cl), Ninja 1.13.2.

### 2026-07-29 (later) — Bug fixes + housekeeping + updater
- **Tier badge fixed** — sidebar, stats bar, and settings now read `currentTier` from `get_usage_stats`. No longer falsely shows "PRO" to Core buyers.
- **Studio batch cap** raised from 100 to 500 in `lib.rs`. Slider max tier-aware. Sales page copy: "Up to 500 titles per batch".
- **Version unification** — activation screen, sidebar, and settings now read version from `get_app_info` (single source: `CARGO_PKG_VERSION`). No hardcoded strings.
- **`site/` folder deleted** — legacy TitleSmith prototype fully removed (all content already ported to `desktop.html`/`desktop.css`).
- **`package-lock.json` regenerated** — `titlesmith-desktop` → `titleforge-desktop`.
- **Analytics: Plausible → PostHog** — free tier (1M events/month), session recordings, heatmaps. Switched on all 3 web pages.
- **Download page: Windows SHA256** published (`b887fabb...`). Mac/Linux pending CI build.
- **Updater: public key regenerated**, endpoint fixed to Netlify, `updates.json` created. `TAURI_SIGNING_PRIVATE_KEY` set as GitHub secret.

### 2026-07-29 — CONTEXT.md consolidation + Path A adoption
- Root `paul/CONTEXT.md` consolidated as single source of truth. `titleforge-desktop/CONTEXT.md` is now a read-only mirror.
- Local LLM direction locked: **Path A** (llama.cpp + Qwen2.5-1.5B-Instruct + GBNF grammar + RAG few-shot). See §7. Path B (LoRA fine-tune) queued for later.
- SmolLM2 + candle-rs approach deprecated but not yet removed — Path A implementation is next work.

### 2026-07-28 — Code Review Fixes
- Claude UI dropdown value `claude` → `anthropic` (users picking Claude previously got API errors)
- `background_verify` no longer revokes — refresh-only on success
- Machine tracking: `validate` and `background_verify` both send `&machine=<hostname>`
- Tier renamed `"basic"` → `"core"` in Rust + JS. License prefix `TF-BASIC` → `TF-CORE`.
- Remaining `titlesmith-desktop` / `com.titlesmith.desktop` references in `lib.rs` renamed
- Fake SHA256 hashes removed from download page
- EGCG `{placeholder}` leak fixed via `strip_placeholders()` in `assemble_title()`. Sanity test: 0 leaks in 196 titles, 100% keyword presence.

### 2026-07-27 — SEO Engine + Keyring
- New `src-tauri/src/seo.rs` (368 lines, 9 signals, 9 tests). See §3.5.
- Frontend SEO badge + breakdown panel + dashboard mini-badge
- XOR API-key obfuscation replaced with `keyring 3` (OS credential store). Dual-write fallback preserved.

### 2026-07-25 — Rebrand + Licensing + Desktop Site
- **TitleSmith → TitleForge:** crate rename, `tauri.conf.json` productName + identifier, UI labels, prompts, docs. Leftovers in `site/` folder + `package-lock.json` (see §6.2).
- **License overhaul:** email-based validation (no Supabase account required for buyers). `generate_from_purchase` endpoint. `stripe-webhook.js` detects `metadata.product == "desktop"`, generates key, emails via Resend.
- **Background verification:** `background_verify` command + `startBackgroundTasks()` 30-min interval
- **Tier gating (backend):** Core capped at 25 titles/request, no cloud AI, no projects. Pro/Studio: 100 titles, full access.
- **Provider cascade:** `generate.js` — DeepSeek → OpenAI → Anthropic. `AI_PROVIDER` env var deprecated.
- **Desktop sales pages:** `desktop.html`, `desktop-download.html`, `desktop.css`. Redirects in `netlify.toml`.
- **Plausible tags → PostHog** switched on all 3 web pages. Account live.
- **Web content honesty pass:** fake "247,000+ titles" stat removed, fabricated testimonials replaced with product-benefit cards, footer placeholder copy removed, vidIQ/SEMrush comparison tightened.

### 2026-07-25 → 07-28 — Local LLM (SmolLM2) Attempt
- `local_llm.rs` compiled and tested with `candle-rs 0.11` on Windows
- SmolLM2-135M: 4–14 s/title. SmolLM2-360M: 12–16 s/title.
- Fixed `sample_token()` for 2D logits from prefill pass
- **Quality verdict:** insufficient as sole engine — 135M ignores title-only instructions ~36% of runs even with few-shot. 360M no meaningful improvement.
- **Decision (2026-07-29):** Adopt Path A. Being replaced.

### 2026-07-15 — EGCG Algorithm
- Replaced old Markov chain with 3-mode EGCG (`title_gen.rs`, then 1270 lines, now 1533)
- Deleted `markov.rs`
- 11 documented bugs surfaced over 4 audit rounds → EGCG became fallback engine, not the winner

### Earlier
- **v0.2.0** — Full desktop UI redesign: activation split-panel, left sidebar, single-page layout, `dashboard.html`/`dashboard.js` (desktop) removed
- **Font rebrand:** Clash Display + Satoshi
- **Logo redesign:** amber forge palette

---

## 6. Current Status (2026-07-31)

### 6.1 Done (Compiled + Tested)
- `cargo check` — 0 errors, 0 warnings
- `cargo test` — 19/19 pass (10 EGCG + 9 SEO)
- `cargo build --release` — 22.74 MB binary on Windows
- `npm run dev` — app launches, EGCG generator builds (2,112 words), LLM lazy-loads
- **Path A LLM implemented** — llama-cpp-2 + Qwen2.5-1.5B. Compiles, runs, produces titles. Batched prefill at 3.5s/title. RAG few-shot + retry + post-cleaning pipeline works. **Not production-quality: 46% pass rate on 50-keyword benchmark.** Remaining model produces empty output on 27/50 keywords.
- **50-keyword benchmark written** (`src-tauri/tests/bench_path_a.rs`) and run. Results at `bench-results.csv`.
- **First generated Qwen title:** "Revitalize Your Day with Coffee: 7 Minute Coffee Cure" — keyword match, creative, 1st attempt.
- **Security audit fixes applied:** CSP enabled in `tauri.conf.json`, `crypto.randomBytes()` for license key generation, `crypto.timingSafeEqual()` for secret comparison, graceful LLM init failure (no panic), `eprintln!` guarded behind `#[cfg(debug_assertions)]`.
- **Performance fixed:** DB mutex scoped — released before LLM inference (was held 90s+ blocking all IPC). Batched prefill optimization. Reusable decode batch.
- **Code review fixed:** Extra `</div>` breaking download page grid, duplicate `action` param breaking license verification, category filter uses IDs not labels, `isPro` defaults to `false`, tier defaults `core`, `urlencoding()` handles multi-byte UTF-8, slider max reapplied after stats load.
- Desktop pages live at `titleforge-tool.netlify.app/desktop` and `/desktop/download`
- License system overhaul (email-based, Stripe webhook, Resend email delivery)
- Web deploy with nav, pricing teaser, honest testimonials, factual comparisons
- SEO scoring integrated end-to-end
- Provider cascade active on web AI generation
- PostHog analytics live on all 3 web pages with project key
- Updater public key regenerated, endpoint fixed to Netlify, `TAURI_SIGNING_PRIVATE_KEY` set as GitHub secret
- 19/19 tests pass. `cargo check` clean.

### 6.2 Known Issues (Priority Order)

0. **DESKTOP: Qwen sampler is deterministic — batch generation produces one unique title.** Pure argmax in `generate_chat_raw`; identical output across runs, users, and calls. The 100% usable figure holds only at k=1. **✅ FIXED 2026-07-31 (late night): temperature + top-k sampling (T=0.8, top-k 40, inverse-CDF via `rand::thread_rng()`). Same keyword now yields different titles every call (5/5 unique verified). Qwen usable 96% at T=0.8, mean 81.0. See §5 change log for the full temperature sweep.**

0b. **DESKTOP: batch generation measured — uniqueness FIXED, timing & diversity still open.** Real 25-title batch (2026-07-31, Task 0b): **25/25 unique** (was 1), 169.6s (6.79s/title), 100% LLM. Core tier is fine. **Pro (100) ≈ 22 min and Studio (500) ≈ 110 min are still product problems** — needs parallelism or context reuse (reuse one KV cache across a batch), or lower per-tier caps. **Template diversity within a batch is weak** (7/25 share "From X to Y") — **Task 1 (port web quality rules) TESTED AND REVERTED: rules made it worse (77.6-75.2 mean vs 81.0 baseline). Qwen 1.5B can't hold multi-constraint prompts. Fix requires a bigger model, not a prompt tweak.**

1. **WEB: sampling penalties suppress the keyword.** `frequency_penalty: 0.6` + `presence_penalty: 0.4` in [generate.js:293-294](titleforge/netlify/functions/generate.js:293) penalise tokens the more often they appear. Every title in a batch must contain the same keyword, so the keyword is progressively suppressed — worse the larger the batch. Contradicts the prompt's own instruction. **✅ FIXED July 31: freq reduced to 0.15, presence removed entirely. Anthropic temperature unified to 0.85.**

2. **WEB: few-shot revert is UNSAFE and must be re-tested.** Few-shot was added, appeared to drop "keyword compliance" 68%→62%, and was reverted July 31. **That measurement came from the broken keyword gate.** Few-shot teaches the model to write natural, creative titles — which is exactly what the old metric scored 0. Corrected data shows titles lacking the literal keyword average 88.4 and are 100% usable. The "lesson" recorded earlier (*"few-shot only helps when examples contain the target keyword"*) is **not supported** by valid evidence. Re-run the A/B against the fixed metric before treating the revert as final.

3. **WEB: appeal score is self-graded and inflated.** Model writes and scores in one pass. Evidence: EGCG self-scored 60-100 on titles the independent judge scored 15-30. The 0-100 score is a headline Pro feature; if users learn to distrust it the feature is worthless. Fix: separate scoring pass, or force "identify your weakest title and score it below 60."

4. **Offline engines are ~40 points behind cloud.** Corrected benchmark (metric fixed 2026-07-31): **Cloud 100% usable, mean 89.8, σ 4.9.** Curated 62%. Qwen 52% (96% usable when it fires; 27/50 fire rate is the constraint). EGCG 20%. Cloud is not "the ceiling we're chasing" — it is already essentially perfect on this rubric. The strategic question is no longer "how do we reach cloud quality offline" but "what is offline actually for."

5. ~~Benchmark keyword check is punctuation-blind.~~ **FIXED 2026-07-31, then superseded by a deeper fix.** `keyword_present` no longer gates the judge at all — it wrongly scored 0 on any title lacking the literal keyword token. Titles *without* the literal keyword average **88.4** and are **100% usable** (n=19); they were the best output in the dataset and were all being discarded. Literal presence is now an advisory `kw_literal` CSV column. See §5 change log.

6. **Curated cannot fill a batch.** Median 2 titles per keyword. 1/50 keywords can fill 25 titles; 0/50 can fill 100 or 500. `retrieve_similar()` is fully deterministic — same keyword always yields the same titles, for every user, every time. It is a lookup table over a fixed corpus, not a generator. Do not plan around it as a primary engine.

7. **EGCG confirmed dead — 20% usable on the corrected metric.** Produces output 98% of the time (49/50) with the *highest* literal-keyword rate of any engine (49/50) and the *lowest* quality (mean 37.5). It is the clearest demonstration that keyword presence and usability are inversely related. Results-pool fragments like "That move the needle" produce "From Dawn to That move the needle" — `strip_placeholders` only handles `{word}` brackets. **✅ RETIRED July 31 (Task 2): removed from the production pipeline (`engine.rs` Pass 2 deleted). Pipeline is now Qwen → curated. `title_gen.rs` kept for `retrieve_similar()` + benchmark comparison.**

8. **Studio "500 titles" is not deliverable by any local engine.** At 3.5s/title a 500-title batch is ~30 minutes. [desktop.html:453](titleforge/desktop.html:453) needs a non-numeric promise. Same class of issue as the previously-fixed "unlimited batch" claim.

9. **WEB: style descriptions are circular and example-free.** `shout: 'high-impact words that shout'`. 9 styles, 5 gated behind Pro, zero example titles. Thin for a paid feature.

10. **WEB: no per-category length targets in standard mode.** Cross-medium mode specifies char ranges; standard mode doesn't. `seo.rs` scores length-fit that generation never targets.

11. **WEB: temperature inconsistent across providers.** 0.85 on OpenAI-compatible ([generate.js:291](titleforge/netlify/functions/generate.js:291)), 0.7 on Anthropic ([:323](titleforge/netlify/functions/generate.js:323)). **✅ FIXED July 31: Anthropic now 0.85.**

12. **Cloud AI benchmarked — corrected 2026-07-31 (late).** DeepSeek V4 Flash with the production prompt: **100% usable (50/50), mean 89.8, σ 4.9.** The earlier "68%" and "62%" figures were artefacts of the broken keyword gate, not real quality differences. **Open follow-up: the few-shot revert (issue #2) was decided on that broken measure and must be re-tested.** Few-shot produces natural, creative titles — precisely what the old metric scored 0.

13. **Qwen model not bundled in production builds.** `tauri.conf.json` bundles SmolLM2 but not Qwen2.5-1.5B (~940 MB). Production users fall back to SmolLM2 — worse than EGCG.

14. **GBNF grammars blocked on llama-cpp-2 API** — `sampled_token_ith()` doesn't work with backend samplers. **✅ RESOLVED via logit biasing July 31:** Instead of GBNF, use `ctx.candidates()` + `token_to_str()` to ban echo-prefix tokens at the first autoregressive position. Works, no sampler API needed. Qwen fire rate improved 32%→46%.**

15. **Download page: Mac & Linux SHA256s pending.** Need production CI builds. Windows SHA256 published.

16. **Updater signature pipeline wired but untested.** Next `v*` tag push will be the first test.

17. **CORS wildcard on POST endpoints.** `licenses.js` uses `Access-Control-Allow-Origin: *`. Low risk. Should be restricted.

18. **License key validation endpoint has no rate limiting.** Public endpoint, no per-IP throttle.

19. **Web Pro → free Core desktop license** not implemented.

20. **Upgrade pricing (pay the difference) between desktop tiers** not implemented.

21. **Annual update renewal / major version upgrade pricing** not implemented.

22. **Admin dashboard for support staff** — planned, not started. Deferred for post-launch.

### 6.3 Strategic Decisions (Active)

| # | Decision | Status |
|---|---|---|
| 1 | Local LLM: **Path A** (llama.cpp + Qwen2.5-1.5B + GBNF + RAG few-shot) | **Benchmarked** — 32% usable, 68% empty-output. Larger model or retire decision pending (§7.2). |
| 2 | Path B (LoRA fine-tune on synthetic titles) as future upgrade after Path A ships | Planned — only if Qwen path continues |
| 3 | EGCG demoted to Pass 3 fallback once benchmark confirms Qwen superiority | **Reversed** — EGCG (20% usable) is worse than Qwen (32%) and Curated (58%). Neither is primary-engine-ready. |
| 4 | Three pricing tiers: $29 Core / $59 Pro / $89 Studio | Deployed |
| 5 | One-time purchase + optional update renewal | Planned, not implemented |
| 6 | Unify brand under TitleForge | **Done** |
| 7 | Web Pro → free Core desktop license | Planned, not implemented |
| 8 | Background license verification every 30 min | Done |
| 9 | License by email, not user_id | Done |
| 10 | Recurring revenue via annual updates + major version upgrades | Not implemented |
| 11 | Updater signed builds via CI | **Done** (key set, untested) |
| 12 | Analytics (PostHog) | **Done** (live on all 3 web pages) |
| 13 | Admin dashboard for support staff | Planned, not started |
| 14 | Security hardening (CSP, crypto keygen, safe compare) | **Done** (July 29 audit fixes) |

---

## 7. Local LLM Roadmap

### 7.1 Where we are (July 29, 2026)

`local_llm.rs` uses **`llama-cpp-2 0.1.153`** (Rust bindings for llama.cpp) to run **Qwen2.5-1.5B-Instruct** (Q4_K_M, 940 MB). Compiles on Windows with LLVM+CMake+MSVC Build Tools. Generates titles in ~3.5 seconds with batched prefill. RAG few-shot from curated corpus via `retrieve_similar()` in `title_gen.rs`. Fallback order: Qwen2.5-1.5B → SmolLM2-360M → SmolLM2-135M (first found wins).

SmolLM2 model files (135M and 360M GGUF) are kept in `models/` as fallbacks if Qwen fails to load. **`candle-core`, `candle-transformers`, and `tokenizers` crates are still in `Cargo.toml` but are no longer imported by any file** — leftover from the SmolLM2/candle-rs attempt. Safe to remove in a cleanup pass (§6.2 #7).

### 7.2 Path A — Implemented, not yet primary (benchmarked 2026-07-30, reassessed 2026-07-31)

**Stack:**
- **Runtime:** `llama-cpp-2 0.1.153` (Rust bindings for llama.cpp) — faster CPU inference with batched prefill
- **Model:** Qwen2.5-1.5B-Instruct (Q4_K_M quant, ~940 MB) — per GGUF chat template
- **Prompting:** `retrieve_similar()` in `title_gen.rs` — token-overlap retrieval of top-k curated titles as few-shot examples. Injected into the prompt before generation.
- **Post-processing:** 3-retry loop with instruction-echo cleaning and colon salvage.

**Performance:** 3.5 seconds per title on i7-1185G7 (4-core, AVX2). Batched prefill feeds all prompt tokens in one `LlamaBatch::decode()` call. Autoregressive decode feeds one token per step at correct KV cache positions.

**What remains for Path A — REVISED 2026-07-31 after audit:**

The 2026-07-30 benchmark data, re-read against the raw CSV:

- **Qwen:** 32% usable (≈42% after fixing the punctuation bug, §6.2 #5). **100% usable when it fires** (16/16). Fires 32% of the time. This is a **recall problem, not a quality problem.**
- **EGCG:** 20% usable (≈24%). Produces output 94% of the time; most of it is gibberish. It is the only engine that can currently fill a 25-title batch, which is why offline batches are poor.
- **Curated:** 58% usable (≈62%) **at k=1 only.** Median 2 titles available per keyword; 1/50 keywords can fill a 25-title request, 0/50 can fill 100. Deterministic — same input always yields the same output for every user. **Not a generator. Cannot be the primary engine.**

**Corrected reading:** the earlier "both below 60% = local generation is a dead end" framing assumed one of these would be primary. That was the wrong frame. Qwen's quality is already excellent; only its fire rate is broken, and fire rate is the cheap fix. Fine-tuning is a quality tool aimed at a problem that isn't the bottleneck.

**Revised order (see AI-WORK-BRIEF §4 for step-by-step):**

1. **Fix the benchmark punctuation bug** (~15 min) — all engine numbers are understated until this lands.
2. **Benchmark cloud AI as a 4th engine** (~1 hr, cents) — establishes the actual quality ceiling. Every downstream decision depends on it and we have never measured it.
3. **Logit biasing on Qwen** via `ctx.candidates()` (1-2 days, no GPU, $0) — kills the instruction-echo failures that cause the 68% empty rate. Expected 32% → 55-70%. **This is the biggest available offline win.**
4. **Re-benchmark, then decide** whether Qwen2.5-3B, a fine-tune, or retirement is warranted — with numbers, not estimates.

**On fine-tuning (Path B):** not the biggest win, and not currently possible on this hardware. Dev machine has Intel Iris Xe integrated graphics, no CUDA GPU. Training requires rented compute (Kaggle free tier / Colab / Runpod ~$2-8). ~85% of the work is agent-automatable; the training step is a hard human gate. Revisit only after step 3 above.

**Realistic ceilings — nothing reaches 100%:** logit-biased Qwen ~55-70%, fine-tuned Qwen ~70-85%, cloud AI ~85-95%. **Target 80%.**

**Usability bar (canonical for this project):** A title is usable if a human creator would publish it without editing. Grammatically clean, contains the keyword (or a clear stem of it), fits the category's conventions, provokes curiosity or names something concrete. Format-conformant garbage does not count. Users are paying for titles they can ship, not titles that pass a heuristic.

### 7.3 Path B — Planned future upgrade (after Path A ships)

**Goal:** A small model specialized for title generation. Higher quality ceiling than any general-purpose small model.

**Approach:**
1. **Synthetic training data:** Use DeepSeek or GPT-4o to generate 20–50k `(keyword, category, style) → titles[10]` pairs. Cost: ~$20–50 in API. Include the existing 2,623 curated titles as gold examples.
2. **Data curation:** Filter for length, non-repetition, category-appropriateness. Dedup. Split 90/5/5 train/val/test.
3. **LoRA fine-tune** Qwen2.5-1.5B (the Path A base). Rank 16–32 adapter. 3–5 epochs. Runpod A100 ~4–8 hours. Cost: ~$20–40.
4. **Merge or side-load** the adapter. Ship as ~50 MB file separate from base model, hot-swappable.
5. **Bench Path B vs Path A** on the same 50-keyword test set. If Path B doesn't clearly win, don't ship it.

**Retriggers:** Every ~6 months regenerate training data with the latest strong AI, re-train, re-ship adapter. Base model stays put.

**Cost model:** ~$50–100 per training cycle. Adapter file is tiny to ship. Real cost is engineering time (~1–2 weeks per cycle).

**Not urgent:** Do this after Path A is in users' hands and you have real-world quality signal.

### 7.4 What we're deliberately not doing

- **Cloud-only (Path C).** Offline must work. Rejected.
- **Custom transformer / from-scratch training.** Diminishing returns vs LoRA on a proven base.
- **7B+ models locally.** Install size and inference speed become punishing on non-GPU machines.
- **ONNX / DirectML / MLX per-platform.** llama.cpp is uniformly good across CPU targets and Metal/CUDA when present.

---

## 8. Key Decisions & Conventions

- **No framework.** Vanilla HTML/CSS/JS. No React, Vue, Svelte, Tailwind. Don't introduce one.
- **Desktop is tiered.** Core/Pro/Studio with backend gating in `lib.rs`. Tier stored in SQLite, verified by server every 30 min.
- **Offline-first, online-verified.** App works fully offline. Silent license refresh + update check every 30 min when online.
- **License by email, not user_id.** Desktop buyers don't need a Supabase account.
- **One brand, two products.** "TitleForge" (web SaaS) and "TitleForge Desktop" (downloadable). Same palette, fonts, logo.
- **Local LLM path is Path A** (§7.2). SmolLM2 + candle-rs is being replaced.
- **EGCG is not trusted.** Stays as fallback only. If curated retrieval + Path A LLM cover a case, EGCG is not needed.
- **License key prefixes:** `TF-CORE-XXXX` ($29), `TF-PRO-XXXX` ($59), `TF-STUDIO-XXXX` ($89).
- **Upgrade pricing = difference** (once implemented).
- **JSON repair is critical for cloud AI.** Kept for web `generate.js`. Not needed for Path A local LLM (grammar-constrained).
- **Seed data generated by AI.** ~$15 total (DeepSeek V4 Pro + Flash). 2,623 curated titles across 16 categories × 9 tones.

---

## 9. Quick Reference

### 9.1 Build Commands
```bash
cd titleforge-desktop && npm run dev              # dev server + app
cd titleforge-desktop && npx tauri build          # production bundles
cd titleforge-desktop/src-tauri && cargo test     # 19 tests (10 EGCG + 9 SEO)
cd titleforge && npx netlify deploy --prod        # web deploy
```

### 9.2 License Key Formats
| Prefix | Tier | Price | Source |
|---|---|---|---|
| `TF-CORE-XXXX` | Core | $29 | Standalone; free with Web Pro (planned) |
| `TF-PRO-XXXX` | Pro | $59 | Standalone; upgrade from Core (planned) |
| `TF-STUDIO-XXXX` | Studio | $89 | Standalone |

### 9.3 Web Routes
| URL | File | Purpose |
|---|---|---|
| `/` | `index.html` | Web app landing + tool |
| `/dashboard` | `dashboard.html` | User dashboard |
| `/desktop` | `desktop.html` | Desktop sales page |
| `/desktop/download` (or `/download`) | `desktop-download.html` | Download + license activation |

### 9.4 Database URLs
- **Web:** Supabase project → `titleforge` schema (6 tables)
- **Desktop:**
  - Windows: `%APPDATA%/titleforge-desktop/titles.db`
  - macOS: `~/Library/Application Support/titleforge-desktop/titles.db`
  - Linux: `~/.local/share/titleforge-desktop/titles.db`

### 9.5 Endpoints (Netlify Functions)
Base: `https://titleforge-tool.netlify.app/.netlify/functions/`
| Path | Method | Notes |
|---|---|---|
| `config` | GET | Public bootstrap config |
| `generate` | POST | AI title generation (auth required) |
| `usage` | GET/POST | Dashboard data + increments |
| `verify-subscription` | GET | Post-Stripe redirect verify |
| `licenses?action=validate` | GET | Desktop license validation (public) |
| `licenses` | POST | License CRUD (auth) + `generate_from_purchase` (Stripe internal) |
| `stripe-webhook` | POST | Stripe events |
| `waitlist` | POST | Waitlist signup |

---

## 10. CONTEXT.md Maintenance

This file is the **single source of truth**. Update it whenever:

1. A significant implementation is completed (3+ files changed, new feature, architecture change)
2. Git log has 5+ new commits
3. A blocker is resolved or a new one appears
4. A strategic decision or convention is established or reversed
5. Version numbers change

**How to update:** Read the current file, read the diff, rewrite affected sections. Add a dated entry to §5 (Change Log). Keep §6.2 (Known Issues) ranked by priority. Never let §6 drift more than a few commits from reality.

`titleforge-desktop/CONTEXT.md` should be treated as a **read-only mirror** of §3 + §6 of this file. If they disagree, this file wins.
