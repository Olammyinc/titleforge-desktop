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
