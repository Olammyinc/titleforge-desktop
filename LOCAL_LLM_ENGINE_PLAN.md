# Local LLM Engine — Implementation Plan

**Decision:** replace the offline "Database" engine's reliance on template-filling (EGCG + legacy fallback) with a small, bundled, on-device language model as the primary offline generator. Keep the existing EGCG/legacy code as an automatic safety-net fallback only — not as a quantity-padding mechanism.

**Why:** documented at length in `EGCG_Audit_Report.md`. Short version: template-filling with statistical scoring recombines words from a small pool with no real language understanding. Every bug we fixed (capitalization, duplicate words, gerunds, genre/tone wiring, random-fallback) improved things, but the architecture has a hard ceiling — for any keyword that doesn't closely match the curated corpus's vocabulary (e.g. "shirt" against a literary/business-skewed corpus), it produces nonsense no amount of patching fixes. Only a real language model can generate grammatically and semantically coherent text for arbitrary keywords.

---

## 1. Model selection

**Primary pick: SmolLM2-360M-Instruct, GGUF, Q4_K_M quantization.**

- Repo: `HuggingFaceTB/SmolLM2-360M-Instruct-GGUF` (official) — quantized mirrors also exist from `unsloth`, `QuantFactory`, `Mungert` if the official repo doesn't have the exact quant level needed.
- License: Apache 2.0 — clean commercial redistribution, no attribution-notice complexity beyond standard Apache terms. This matters: TitleForge Desktop is a paid product ($29/$49 one-time), so license simplicity is a real factor, not just a checkbox.
- Architecture: Llama-family. `candle-transformers` has mature, well-exercised support for Llama-architecture quantized GGUF inference (`candle_transformers::models::quantized_llama`) — this is the most battle-tested path in candle, lower integration risk than a newer/less common architecture.
- Approximate size: Q4_K_M quantization of a 360M model should land roughly 200-260MB. Verify exact size once downloaded — don't assume, confirm.
- Approximate RAM during inference: model size + a few hundred MB for context/KV cache, so budget ~500MB-800MB free RAM. Fine on any machine from the last decade.

**Fallback candidate if SmolLM2-360M quality is insufficient in testing: Gemma 3 270M-it, GGUF** (`ggml-org/gemma-3-270m-it-GGUF` or `bartowski/google_gemma-3-270m-it-GGUF`). Smaller parameter count but a newer/more modern training recipe. License is Google's Gemma Terms of Use — commercial use is explicitly permitted, but read the actual terms (prohibited-use policy, attribution requirements) before committing, since it's a different license shape than Apache 2.0. Only fall back to this if SmolLM2-360M's output quality doesn't clear the bar in Phase 2 testing below — don't switch preemptively.

**Do not reach for anything bigger** (Phi-4-mini, Gemma 3 4B, Qwen 1.5B+) unless both of the above fail on quality — those cross into multi-GB installer territory and multi-second generation times, which defeats the "instant, lightweight offline engine" value proposition this feature exists to preserve.

---

## 2. Runtime: candle-rs, not llama.cpp bindings

Use `candle-core` + `candle-transformers` (pure Rust, from Hugging Face). Reasons:
- No C++ toolchain dependency — the existing `.github/workflows/build.yml` does 3-platform builds (Linux/Windows/macOS) via plain `cargo`/`tauri build`. llama.cpp Rust bindings wrap a C++ library and need cmake + a C++ compiler configured per-platform in CI, which is extra surface area to get wrong across 3 OSes. Candle avoids that entirely.
- Fits the existing codebase's character — everything in `src-tauri` is already pure Rust with no native C dependencies (`rusqlite` uses the `bundled` feature specifically to avoid needing system SQLite — same philosophy applies here).
- Confirmed current (2026) GGUF quantized inference support, actively maintained, explicit Llama/Gemma/Mistral/Phi architecture support.

Crates to add to `src-tauri/Cargo.toml`: `candle-core`, `candle-transformers`, `candle-nn` (check current versions on crates.io at implementation time — don't hardcode a version here that may already be stale by the time this is executed). CPU-only build for v1 — no CUDA/Metal feature flags. GPU acceleration can be a later enhancement; it adds per-platform build complexity this plan deliberately defers.

---

## 3. Where this plugs into the existing codebase

Mirror the existing patterns already established in this codebase rather than inventing new ones.

### 3.1 New module: `src-tauri/src/local_llm.rs`

Structure it like `title_gen.rs`'s `Generator`:

```rust
pub struct LocalLlm {
    // loaded model weights + tokenizer, wrapped for the candle inference loop
}

impl LocalLlm {
    /// Load the model from the bundled resource path. Returns None (not
    /// Err) on any failure — model-load failure must be non-fatal, the app
    /// falls back to the existing EGCG engine, it does not crash or block
    /// startup.
    pub fn load(model_path: &std::path::Path) -> Option<Self> { ... }

    /// Generate one title. Takes the same shape of context EGCG's Mode C
    /// already gathers (keyword, category, genre, style, a handful of
    /// matching curated titles as few-shot examples) and returns a single
    /// generated line, unvalidated — validation happens in the caller.
    pub fn generate_one(&self, prompt: &str) -> Option<String> { ... }
}
```

### 3.2 `AppState` (in `lib.rs`)

Add `local_llm: Option<Mutex<local_llm::LocalLlm>>` (or similar) alongside the existing `db`/`generator` fields. `Option` because load failure must be representable and handled gracefully — see §6.

Load it once at startup in `run()`, next to where `Generator::build()` is already called, wrapped in the same "never panic, log and continue" style already used for seed import (see `lib.rs` lines ~729-762 — that fallback ladder is the pattern to copy).

### 3.3 Generation entry point

The existing `generate_titles` IPC command (`lib.rs`) currently calls `engine::generate()`, which does EGCG-then-legacy-fallback. Change this ordering:

```
1. If local_llm is loaded: generate via local_llm (see §4 for the loop).
2. If local_llm produced fewer than `quantity` results that pass QC (see §4.3),
   DO NOT backfill with the legacy random-fill engine to hit the count.
   Backfill with EGCG's Mode A/B/C only (already-fixed, already scored) up to
   the shortfall, and only reach for the legacy `fill_template` random engine
   as the very last resort if EGCG also can't fill the gap.
3. If local_llm failed to load at all: fall back to the current
   EGCG-then-legacy chain unchanged, exactly as it works today.
```

This preserves the "quality over hitting an exact count" principle from our earlier discussion even within the new architecture: it's fine to return 6 good titles for a request of 10 if that's genuinely all that clears the bar. Silently padding with the worst-available generator to hit a round number is the behavior that produced the "Algorithm"/"Map"/"Factory" nonsense in the first place — don't reintroduce it at a different layer.

---

## 4. Prompting strategy

### 4.1 Few-shot grounding from the curated corpus

Reuse the existing DB query pattern already in `title_gen.rs`'s `embed_mode`/`embed_from_relevant` (filter `curated_titles` by category + genre + style with the same fallback ladder: exact match → genre-only → category-only). Pull 3-5 matching curated titles as in-context examples. This is the whole point of the tone-tagged 2,623-title corpus we already built — it stops being consumed only by brittle template-slot-filling and instead grounds a real language model in TitleForge's actual voice per category/tone, which is a much higher-leverage use of that data.

### 4.2 Prompt shape

Keep it simple — a 360M model will not reliably follow complex formatting instructions or produce valid JSON. Plain text, one title per generation call, minimal instruction:

```
You are a creative title-writing assistant for {category} titles in a {style} tone.

Examples:
- "{curated_title_1}"
- "{curated_title_2}"
- "{curated_title_3}"

Write ONE new {style} {category} title about "{keyword}". Reply with only the title, nothing else.
```

Call the model once per title needed (or in a small batch loop), not once for the whole batch — asking a 360M model for "10 titles at once" is a much harder instruction-following task than "one title" repeated 10 times with slightly varied sampling (temperature/seed). Verify this assumption empirically in Phase 2 rather than assuming — if batched generation turns out fine in testing, it's simpler and faster; the one-at-a-time approach is the safer default to start from.

### 4.3 Output validation (QC gate, not just scoring)

Every generated line must pass a hard gate before being shown to the user, not just a soft score:
- Non-empty, reasonable length (reuse the `word_count >= 3` type checks already in `title_gen.rs::generate()`).
- Contains the keyword or a close variant (reuse the existing `lower.contains(&kw_lower)` check pattern already used across all three EGCG modes).
- Not a verbatim copy of one of the few-shot examples (the model echoing an example back is a known failure mode for tiny instruct models — dedupe against the prompt's own examples, not just against other results).
- Run through the existing `score_title()` heuristic scoring (already generator-agnostic — it scores any title string, no changes needed there) and apply the same kind of coherence/quality floor discussed for the "quality gate" approach — reject and retry (up to a small attempt cap, e.g. 3x) rather than accept a bad result to hit the count.

---

## 5. Bundling and distribution

Bundle the model file directly in the installer rather than downloading on first launch — the product's value proposition is "instant, fully offline," and a required first-launch download undermines that (and adds a failure mode: what happens offline on first run?).

- Add the `.gguf` file to `src-tauri/` (or a subfolder, e.g. `src-tauri/models/`).
- Wire it into Tauri's bundle resources in `tauri.conf.json` (`bundle.resources`) so it ships inside the NSIS/.dmg/.deb/.AppImage outputs, same conceptual mechanism already used for `seed-data.json` being placed next to the binary (see `db.rs`'s `seed_paths` lookup in `lib.rs` — the model file needs the same kind of "look next to the binary, then in the app data dir" resolution logic).
- **This will grow the installer size by roughly 200-300MB** (SmolLM2-360M Q4_K_M) or somewhat less for Gemma 3 270M. Flag this clearly to whoever is executing this — it's a real, visible change to download size that's worth a heads-up in release notes, not something to slip in silently.
- Update `.github/workflows/build.yml` if the model file needs to be fetched during CI (e.g., via a build step that downloads it from Hugging Face into `src-tauri/models/` before `tauri build` runs, rather than committing a 200MB+ binary file to git). Committing large binaries to the repo directly is generally worth avoiding — prefer a CI-time download step with a pinned URL/checksum, and document the same download step for local dev builds in `README.md`.

---

## 6. Fallback and safety behavior

Model load must never crash the app or block startup:
- `LocalLlm::load()` returns `Option`, not `Result` propagated with `.expect()` — any failure (file missing, corrupted, unsupported CPU instructions, out of memory) results in `None`, logged via `eprintln!` exactly like the existing seed-import fallback pattern, and the app proceeds with `local_llm: None` in `AppState`.
- When `local_llm` is `None`, generation silently uses the existing EGCG-then-legacy chain, unchanged. A user on unusual/old hardware where the model can't load should get the same experience they have today, not a broken app.
- Consider surfacing engine status in `get_app_info` (already returns `{app, version, seeded, templateCount}` — add something like `localLlmLoaded: bool`) so the UI can optionally indicate which engine is actually active, rather than the user having no visibility into a silent fallback.

---

## 7. UI considerations (flag, don't necessarily implement in this pass)

The engine toggle currently shows "Database" / "AI" / "Auto" with the subtitle "AI-first, falls back to local database." Once the local engine is a real language model rather than a template filler, that label undersells it — but renaming UI copy is a smaller, separate decision. Flag it in the PR/report back rather than deciding unilaterally: does "Database" become "Local AI" or similar? That's worth a quick confirmation before touching UI copy.

---

## 8. Testing and validation plan

Use the same reproducible test that surfaced the original problem — don't consider this done without running it:

- Keyword: `shirt`. Categories: article, blog, YouTube. Style: Normal, then repeat for at least 2-3 non-normal styles (Provocative, Whisper, Playful) to verify tone actually comes through. Genre: Any.
- Compare against the screenshots already on record in this conversation (the "Algorithm"/"Map"/"Navigate Shirt" era output) — every title in the new output should be grammatically valid and topically related to "shirt," even if not brilliant.
- Measure and record: cold-start model load time, per-title generation latency, peak RAM during a 10-title batch, and final installer size per platform (before/after comparison).
- Confirm the fallback path still works: temporarily rename/remove the bundled model file and confirm the app still generates via the EGCG/legacy chain without crashing.
- Run `cargo build && cargo test` on all 3 platforms (or at minimum the platform available) before calling this done — this plan's authoring session did not have working shell access to verify compilation; whoever executes this must actually compile it.

---

## 9. Phased execution order

1. **Proof of concept:** get candle loading the chosen GGUF model and generating *any* text output from a hardcoded prompt, outside the Tauri app (a standalone `cargo run` test binary or a `#[test]`). Confirm this works on at least one platform before wiring anything into the app. Confirm architecture support, confirm the crate versions actually compile together.
2. **Prompt/quality validation:** iterate on the few-shot prompt shape (§4.2) against real curated-title examples pulled from the existing DB, manually eyeball output quality across a handful of keywords (include "shirt" specifically) and categories before writing any Tauri integration code. If SmolLM2-360M's output isn't good enough here, switch to the Gemma 3 270M fallback candidate now, before integration work, not after.
3. **Integration:** wire into `AppState`, `local_llm.rs`, the `generate_titles` command ordering (§3.3), and the QC gate (§4.3).
4. **Bundling:** resource bundling in `tauri.conf.json`, CI download step in `build.yml`, path resolution logic matching the seed-data pattern.
5. **Fallback verification + full test pass:** §6 and §8 above.
6. **Report back** with: final model choice + size, measured latency/RAM numbers, installer size delta per platform, and any deviations from this plan (e.g. if Gemma 3 270M was used instead, if batched vs. one-at-a-time generation was chosen, if a different quantization level was needed for quality or size reasons) — flag these explicitly rather than silently deciding, same as the curated-titles work did well last time.

---

## 10. Open questions to flag back rather than assume

- Exact final model file size and installer size delta — estimated above, must be confirmed with the real downloaded file.
- Whether one-at-a-time or batched generation actually works better with this specific model at this specific size — stated as an assumption in §4.2, verify empirically.
- Whether Gemma 3 270M ends up preferred over SmolLM2-360M after quality testing — don't assume the primary pick sticks, that's exactly why Phase 2 exists before integration work.
- Whether the engine-toggle UI copy should change (§7) — flag for a decision, don't just rename things.
- Whether CI needs a pinned download URL + checksum for the model file, and where that URL should live (a repo secret isn't needed since the model is public, but a version-pinned URL avoids silent drift if the upstream HF repo changes).

---

## 11. Review findings (2026-07-16, independently verified)

No completion report was written for this pass, so this review worked entirely from reading the code. Overall: the structural work is genuinely solid — model choice, candle integration, bundling, CI, fallback ordering, and `AppState`/`get_app_info` wiring all match the plan closely, in some places exceeding it (the model-path resolution in `lazy_load_llm()` checks more candidate locations than this plan specified, and lazy-loading on first generation rather than at startup is a reasonable, unflagged improvement on the plan's suggestion). But there was one critical bug and two meaningful gaps versus what this plan asked for, all now fixed.

### Critical bug (fixed)
**The decode loop in `local_llm.rs::generate_one()` never actually used its own generated tokens.** The token sampled after prefill was bound with plain `let next = ...` (immutable, outer scope). Inside the decode `for` loop, `let next = sample_token(&logits).ok()?;` *shadowed* that variable — but a `let` inside a loop body only lives for that one iteration. Every iteration after the first fed the model the exact same first-generated token as input again, while the position index kept advancing. This breaks autoregressive generation outright — the model would never actually be conditioned on what it had just written. Given this is the single most load-bearing piece of code in the whole feature (nothing works if decoding is broken), this had to be fixed before anything else mattered. Fix: made the outer binding `mut` and reassigned in place (`next = sample_token(...)`, no `let`) instead of shadowing.

### Real gaps versus the plan (fixed)
- **§4.1 (few-shot grounding from the curated corpus) was skipped entirely.** The LLM prompt in `engine.rs` was a bare instruction with no examples, and the database connection wasn't even used in that code path. This was the specific, named reason for reusing the 2,623-title tone-tagged corpus rather than treating the LLM as a blank slate — without it, the model has no grounding in TitleForge's actual per-category/tone voice. Fixed: added `fetch_curated_examples()` (same relax-tone-then-genre fallback ladder as EGCG's Mode C) and wired 3-4 matching curated titles into every prompt.
- **§4.3 (hard QC gate) wasn't implemented as a gate.** `calculate_heuristic_score()` existed and was called, but only to compute a *display* score — nothing rejected a result for lacking the keyword, echoing a few-shot example back verbatim, or duplicating another result already in the batch. The plan was explicit that this needed to be a hard reject-and-retry gate, not a soft scoring bonus. Fixed: added keyword-presence, echo-detection, duplicate-detection, and length checks as hard gates, with a capped retry loop (3x the per-category target) rather than accepting a bad result to hit the count.
- **Bonus finding while fixing the above: LLM generation was hardcoded to `categories.first()` only.** If a user selected Article + Blog + YouTube, the LLM pass only ever generated (and only ever prompted for) "article" titles, while every result was still labeled as fitting all three requested categories in the `categories` field. Fixed as part of the same change — now loops per requested category with its own examples and its own share of the quantity, matching how EGCG's Mode A already splits work across categories.

### Minor, not fixed (flagging only)
~~`scripts/download-model.sh`'s comment claims "uses a pinned checksum to prevent silent model drift," but `EXPECTED_HASH` is declared and never used — checksum verification isn't actually implemented, just scaffolded.~~ **Fixed 2026-07-16, see §12.**
~~LLM-generated results have `breakdown: None`, so they won't show a "Why this works" popup the way EGCG results do.~~ **Fixed 2026-07-16, see §12.**

### Outstanding — could not verify from this session
- **No `cargo build`/`cargo test` was run.** Bash access was unavailable in this session (same environment limitation as every prior pass — confirmed to be a known Cowork/Windows bug with `wsl.localhost` UNC-path workspace folders, not something fixable via permissions). All fixes above were verified by manual type-tracing against patterns already proven to compile elsewhere in these same files, but the KV-cache fix in particular needs to be validated against a real model file and real output — this is not optional given how central that bug was.
- **The reproducible "shirt" test from §8 has not been run.** This is the actual proof the feature works; nothing in this review substitutes for it.
- **Latency at high quantities is unverified and likely worth attention.** The quantity slider goes up to 100. Each local-LLM title can take up to ~50 decode steps of CPU inference, and the retry-on-reject logic (correctly, per the QC gate) can attempt up to 3x the per-category target. A request for 100 titles across 3 categories could mean several hundred real inference calls. This may be fine, or it may need a lower quantity cap for the local-LLM path, a per-request time budget, or a "generating..." progress indicator — worth timing before shipping.

## §12. Follow-up fix pass (2026-07-16, same-day)

Closed out both minor gaps flagged in §11. Still no shell access this session (same UNC-path bug), so these are file edits verified by re-reading, not compiled.

**1. `scripts/download-model.sh` checksum verification — implemented, not just scaffolded.**
Fetched the real SHA256 from HuggingFace's LFS pointer for this exact file (`bartowski/SmolLM2-360M-Instruct-GGUF`, `SmolLM2-360M-Instruct-Q4_K_M.gguf`, main branch — retrieved via the raw LFS pointer endpoint, which returns `oid sha256:...` and `size` directly rather than the binary): hash `2fa3f013dcdd7b99f9b237717fa0b12d75bbb89984cc1274be1471a465bac9c2`, size `270590880` bytes. Pinned both as `EXPECTED_HASH`/`EXPECTED_SIZE` in the script. Added a `verify_hash()` function (tries `sha256sum`, falls back to `shasum -a 256`, warns and skips if neither exists rather than failing the build). The script now: checks the cached file's size against the expected size before trusting a cache hit (previously it only checked "> 100MB"); verifies the checksum on a cache hit; checks the downloaded file's size and checksum after a fresh download; deletes and hard-fails on any mismatch so CI doesn't silently bundle a corrupted or drifted model.

**2. LLM-generated titles now get a real `breakdown`, not `None`.**
The LLM pass in `engine.rs` was scoring titles with `calculate_heuristic_score()` (title_gen.rs) but hardcoding `breakdown: None` — meaning the "why this works" score explanation the EGCG and legacy-engine paths already show was silently missing for LLM results, and worse, the *score itself* came from a different heuristic function than the one that would have produced the breakdown, so even a future "just add breakdown" patch risked showing a score/breakdown pair that didn't agree with each other. Fixed by switching the LLM pass to call `engine::calculate_score()` — the same function the legacy template path already uses — which returns `(score, breakdown)` together from one heuristic pass. `calculate_heuristic_score()` in `title_gen.rs` is no longer called anywhere; left in place with `#[allow(dead_code)]` and an explanatory doc comment rather than deleted, in case it's useful standalone later.

**Still true after this pass:** nothing has been compiled. Once shell/build access is available, `cargo build` should be the very first check — it will immediately confirm whether `calculate_score`'s deref-coercion call signature (`&String` → `&str` for the category argument) and the checksum script's shell syntax are actually correct, not just plausible on inspection.

## §13. Engine-source visibility added (2026-07-17)

Prompted by a live test: a "television" keyword run (Auto engine) was shared for review, and every single result matched a literal EGCG/legacy-template pattern in `seed-data.json` word-for-word (see `EGCG_Audit_Report.md` §7) — meaning the local LLM almost certainly never ran for that test, but there was no way to actually confirm that from the app. Two structural gaps made this undiagnosable:

1. `TitleResult` had no field recording which engine produced a given title — EGCG, the legacy template filler, the local LLM, and the cloud AI-key path all returned the exact same shape, so there was no way to tell them apart after the fact.
2. `get_app_info` already returned `localLlmLoaded`, but the frontend (`app.js`) never read or displayed it anywhere — it silently went unused since the field was added.

Both fixed:

- Added `TitleResult.source` (`Option<String>`, `#[serde(default)]` so old history/favorites JSON still deserializes) and set it at all 8 places a `TitleResult` gets constructed: `"local-llm"` (engine.rs LLM pass), `"template"` (engine.rs legacy filler, all 3 call sites), `"egcg-a"`/`"egcg-b"`/`"egcg-c"` (title_gen.rs Modes A/B/C), `"ai"` (lib.rs cloud API-key path).
- `app.js` now renders a small "Database" / "AI · offline" / "AI · cloud" badge next to each result's category tags (new `engineSourceLabel()` helper + `.result-engine-badge` CSS class), and Settings now has an "Offline AI engine" row wired to `get_app_info().localLlmLoaded` (Active / not-yet-loaded, with a hint to check `[local_llm]` console output if it never turns on).

This doesn't fix why the LLM wasn't active in the test that prompted it — that still needs to be diagnosed once this is compiled (see the reply given directly in-conversation: most likely causes are (a) the tested build predates the LLM code being compiled in, or (b) the model `.gguf` file wasn't present at any of the 6 candidate paths `lazy_load_llm()` checks for that particular install, so it silently fell through to EGCG). What this change does is make that diagnosis possible from inside the running app next time, instead of requiring line-by-line template matching against `seed-data.json` to infer it after the fact.

Not compiled — same environment limitation as everything else this session.
