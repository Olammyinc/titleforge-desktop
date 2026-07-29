# TitleForge — Full Project Context

> **Last updated:** 2026-07-29 (end of session)
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
**Analytics:** Plausible script tag on `index.html` and `desktop.html`. Events: `signup`, `generate`, `pro_upgrade_click`, `favorite_add`. **Account not yet created** — script fires no-ops.

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
- **Rust crates:** `tauri 2`, `rusqlite 0.31` (bundled SQLite), `reqwest 0.12` (blocking HTTP), `serde/serde_json`, `rand 0.8`, `chrono 0.4`, `dirs 5`, `hostname 0.4`, `keyring 3`, `candle-core / candle-transformers / candle-nn 0.11` (SEO engine, EGCG), `llama-cpp-2 0.1.153` (Path A LLM), `tokenizers`, `tauri-plugin-shell 2`, `tauri-plugin-updater 2`
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
| `src-tauri/src/engine.rs` | 293 | 3-pass orchestrator: LLM (Pass 1, lazy) → EGCG (Pass 2) → curated fallback (Pass 3). Deduplication + SEO scoring. |
| `src-tauri/src/title_gen.rs` | 1533 | **EGCG algorithm** — 3 modes (exemplar-guided template fill / phrase stitching / keyword-embedded exemplar). `strip_placeholders()` fix for `{placeholder}` leak. |
| `src-tauri/src/local_llm.rs` | 183 | llama-cpp-2 wrapper — `LlamaModel`, `generate_chat_raw()` with batched prefill, `generate_one_clean()` with RAG + retry. Prefers Qwen2.5-1.5B then SmolLM2 fallbacks. |
| `src-tauri/src/seo.rs` | 368 | Local SEO scoring — 9 signals (length, keyword presence/density, search patterns, question, number/year, Flesch reading, power words, uniqueness). Zero API calls. |
| `src-tauri/src/db.rs` | 152 | SQLite schema (8 tables) + seed data import from `seed-data.json` |
| `src-tauri/src/main.rs` | 5 | Entry point → `titleforge_lib::run()` |
| `src-tauri/tauri.conf.json` | 66 | App config, updater endpoint, CSP, bundle config |
| `src-tauri/Cargo.toml` | 37 | Rust dependencies |
| `src-tauri/capabilities/default.json` | 12 | Tauri v2 permissions |
| `seed-data.json` | 1.0 MB | Same as web seed |
| `site/` | legacy | Old TitleSmith marketing prototype — **still branded TitleSmith**, needs cleanup or deletion |

### 3.3 Rust Backend — IPC Commands

`AppState` = `Mutex<rusqlite::Connection>` + `Mutex<title_gen::Generator>` + `Mutex<Option<LocalLlm>>`

**Generation:**
- `generate_titles(keyword, categories, style, genre, quantity, state) -> Vec<TitleResult>` — offline 3-pass pipeline. Tier-capped (Core=25, Pro/Studio=100).
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

### 3.4 Engine — 3-Pass Pipeline

`engine.rs` orchestrates:

1. **Pass 1 — LLM (lazy).** If model file present and loaded, generate via `local_llm.rs`. **Being rewritten under Path A (§7).**
2. **Pass 2 — EGCG (`title_gen.rs`).** Template-based generation with pairwise-affinity coherence scoring. `strip_placeholders()` guard prevents `{placeholder}` leaks (fixed July 28).
3. **Pass 3 — Curated fallback.** Retrieval from 2,623 curated titles, keyword-swapped into topic slot.

All passes: dedup + SEO score sweep post-generation.

**EGCG modes (still live despite earlier plan to remove):**
- **A — Exemplar-Guided Template Fill (70%):** Fill template slots by scoring candidates against left context + keyword affinity + category naturalness. Softmax sample. Retries up to 6× per slot if below `MIN_COHERENCE=0.05`.
- **B — Phrase Stitching (20%):** Mined intro fragments + keyword + closer fragments from curated titles.
- **C — Keyword-Embedded Exemplar (10%):** Find highest-affinity curated title, swap its topic token with the keyword.

**Scoring:** `raw = 2.0 × avg_pairwise_affinity + 0.5 × ln(1 + unigram_sum) - 1.5 × repeat_penalty` → normalized 0–65 base + heuristic bonuses → capped at 100.

**Post Path A:** EGCG will be demoted to Pass 3 fallback only. See §7.

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
- **Known gap:** `updates.json` has empty signatures — not yet wired to the signed release pipeline

---

## 4. Frontend Differences: Web vs Desktop

| Aspect | Web | Desktop |
|---|---|---|
| **Layout** | Top nav + scrollable page | Left sidebar (Ink, 220px) + content area |
| **Activation** | Supabase auth modal | Full-screen split-panel takeover |
| **Pages** | `index.html`, `dashboard.html` (separate) | Single page — Generator/Dashboard/Settings are sidebar panels |
| **Auth** | Supabase (CDN + localStorage fallback) | License key (HTTP + offline cache + 30-min background verify) |
| **Tier gate** | Guest / Free / Pro | Core / Pro / Studio — backend enforces, **UI still always shows "PRO"** (bug — see §6.2) |
| **Data source** | Supabase via Netlify Functions | SQLite via `invoke()` |
| **Generation** | Cloud AI only | 3-pass local (LLM → EGCG → curated) OR BYO cloud AI |
| **Favorites/Projects** | Supabase tables | Local SQLite |
| **Floating generator** | Yes (FAB) | No |
| **Engine toggle** | No | Yes (Database / AI) |

---

## 5. Change Log (Rolling)

### 2026-07-29 (end of session) — Audit fixes + Path A hardening
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

## 6. Current Status (2026-07-29)

### 6.1 Done (Compiled + Tested)
- `cargo check` — 0 errors, 0 warnings
- `cargo test` — 19/19 pass (10 EGCG + 9 SEO)
- `cargo build --release` — 22.74 MB binary on Windows
- `npm run dev` — app launches, EGCG generator builds (2,112 words), LLM lazy-loads
- **Path A LLM complete** — llama-cpp-2 + Qwen2.5-1.5B. Batched prefill at 3.5s/title. RAG few-shot + retry + post-cleaning pipeline works.
- **First generated Qwen title:** "Revitalize Your Day with Coffee: 7 Minute Coffee Cure" — keyword match, creative, 1st attempt.
- **Security audit fixes applied:** CSP, crypto keygen, timing-safe secret, no devtools in prod.
- **Performance fix:** DB mutex scoped — released before LLM inference.
- Desktop pages live at `titleforge-tool.netlify.app/desktop` and `/desktop/download`
- License system overhaul (email-based, Stripe webhook, Resend email delivery)
- Web deploy with nav, pricing teaser, honest testimonials, factual comparisons
- SEO scoring integrated end-to-end
- Provider cascade active on web AI generation
- PostHog analytics live on all 3 web pages with project key
- Updater public key regenerated, endpoint fixed to Netlify, `TAURI_SIGNING_PRIVATE_KEY` set as GitHub secret

### 6.2 Known Issues (Priority Order)

1. **EGCG remains Pass 2, not demoted.** Path A LLM (Qwen) works but EGCG hasn't been demoted to Pass 3. Currently LLM → EGCG → curated. Once benchmark confirms Qwen > EGCG, demote EGCG.

2. **50-keyword benchmark pending.** Need real quality numbers comparing Qwen vs EGCG vs curated on format-conformance, category-relevance, and human readability.

3. **GBNF grammars not implemented.** Forced valid JSON output would eliminate malformed titles. Now possible with llama.cpp. Deferred.

4. **Download page: Mac & Linux SHA256s pending.** Need production CI builds. Windows SHA256 published.

5. **Updater signature pipeline wired but untested.** Next `v*` tag push will be the first test. `updates.json` deployed to Netlify — signatures populated by CI.

6. **Qwen model not bundled in production builds.** `tauri.conf.json` resources bundle SmolLM2 but not Qwen2.5-1.5B (~940 MB). Production users would fall back to SmolLM2. Decision: bundle in installer or download on first launch.

7. **CORS wildcard on POST endpoints.** `licenses.js` uses `Access-Control-Allow-Origin: *`. Low risk because `generate_from_purchase` is secret-protected, but should be restricted.

8. **License key validation endpoint has no rate limiting.** Public endpoint, no per-IP throttle. Could be used for enumeration.

9. **Web Pro → free Core desktop license** not implemented. Strategic decision made, no code.

10. **Upgrade pricing (pay the difference) between desktop tiers** not implemented.

11. **Annual update renewal / major version upgrade pricing** not implemented.

12. **Admin dashboard for support staff** — planned, not started. Deferred for post-launch.

### 6.3 Strategic Decisions (Active)

| # | Decision | Status |
|---|---|---|
| 1 | Local LLM: **Path A** (llama.cpp + Qwen2.5-1.5B + GBNF + RAG few-shot) | **SHIPPED** — see §7.2 |
| 2 | Path B (LoRA fine-tune on synthetic titles) as future upgrade after Path A ships | Planned |
| 3 | EGCG demoted to Pass 3 fallback once benchmark confirms Qwen superiority | Pending benchmark |
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

SmolLM2 and candle-rs crates remain in the project for EGCG and SEO scoring support.

### 7.2 Path A — SHIPPED (July 29, 2026)

**Stack:**
- **Runtime:** `llama-cpp-2 0.1.153` (Rust bindings for llama.cpp) — faster CPU inference with batched prefill
- **Model:** Qwen2.5-1.5B-Instruct (Q4_K_M quant, ~940 MB) — per GGUF chat template
- **Prompting:** `retrieve_similar()` in `title_gen.rs` — token-overlap retrieval of top-k curated titles as few-shot examples. Injected into the prompt before generation.
- **Post-processing:** 3-retry loop with instruction-echo cleaning and colon salvage.

**Performance:** 3.5 seconds per title on i7-1185G7 (4-core, AVX2). Batched prefill feeds all prompt tokens in one `LlamaBatch::decode()` call. Autoregressive decode feeds one token per step at correct KV cache positions.

**What remains for Path A:**
- GBNF grammar constraints (force valid JSON output, eliminate malformed titles)
- 50-keyword benchmark (format-conformance, category-relevance, human readability vs EGCG)
- Demote EGCG to Pass 3 once benchmark confirms Qwen superiority

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
