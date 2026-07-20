# Execution Instructions — Compile, Verify, and Fix the Local LLM Path

**Read this whole file before starting.** It supersedes ad-hoc instructions given
elsewhere in chat. Two supporting documents have the full history and reasoning
if you need it, but you should not need to read them end-to-end to execute this:

- `LOCAL_LLM_ENGINE_PLAN.md` — the original design + every review pass on the
  local-LLM feature (candle-rs, SmolLM2-360M-Instruct, bundling, QC gate).
- `EGCG_Audit_Report.md` — the template-engine bug history, most relevant is
  §7 (2026-07-17), which diagnoses the exact bugs behind a live "television"
  test that produced bad output.

## Why this exists

The offline title generator has two engines: EGCG (a ~1,300-template word-filler)
and a newly-added local LLM (SmolLM2-360M-Instruct, running fully offline via
candle-rs, no API key, no internet call). EGCG has been patched three separate
times across this project and keeps surfacing new grammar/coherence bugs — it's
a combinatorial system (templates × word pools) that's very hard to make
airtight by hand. The local LLM is meant to be the better long-term answer,
but **none of its code has ever been compiled or run.** Every fix described in
`LOCAL_LLM_ENGINE_PLAN.md` was written and reviewed via manual code reading
only, because the reviewing session never had working shell access. Your job
is to actually run it, confirm it works, and fix what doesn't — the owner
wants this **completely right**, not "probably fine."

## Step 0 — Sanity check before touching anything

```
cd src-tauri
cargo build 2>&1 | tee /tmp/build.log
```

This is the single most important command in this whole file. Nothing below
matters if this doesn't pass. Known risk areas to check first if it fails
(these were all edited via manual code-reading, never compiled):

- `engine.rs` — the LLM generation pass (calls `calculate_score(&title, keyword, cat)`
  where `cat: &String` — relies on deref coercion to `&str`; should work but
  verify), and every `TitleResult { .. }` construction (8 total across
  `engine.rs`, `title_gen.rs`, `lib.rs`) needs the new `source: Option<String>`
  field — a missing one will fail to compile with a clear "missing field" error,
  just fill it in with the right string (see the table in Step 3).
- `title_gen.rs` and `engine.rs` — `assemble_title`/`fill_template` were changed
  from `.replace()` to `.replacen(.., 1)`. Should be a drop-in signature match
  but verify.
- `local_llm.rs` — the decode loop's `mut next` reassignment (was previously a
  duplicate-token bug from `let` shadowing — already fixed, just confirm it
  still compiles and, more importantly, **works** — see Step 4).
- `Cargo.toml` — candle-core/candle-transformers/candle-nn/tokenizers/hf-hub
  versions were pinned to `0.11`/`0.21`/`0.4` without ever resolving them
  against the real dependency graph. If `cargo build` fails on version
  conflicts, resolve them and note what you changed.

Fix whatever `cargo build` finds. Then:

```
cargo test 2>&1 | tee /tmp/test.log
```

Report both logs' pass/fail status back verbatim — not "mostly passed" or
"looked fine," the actual output.

## Step 1 — Get the model file onto disk

```
bash scripts/download-model.sh
```

This downloads `SmolLM2-360M-Instruct-Q4_K_M.gguf` (~258MB) into `models/` and
now verifies it against a pinned SHA256
(`2fa3f013dcdd7b99f9b237717fa0b12d75bbb89984cc1274be1471a465bac9c2`, size
270590880 bytes). If this checksum step fails, **stop and report it** — do not
comment it out or bypass it; that hash was pulled directly from HuggingFace's
LFS pointer for the exact file this app depends on, so a mismatch means either
a corrupted download or the upstream file actually changed, both of which
matter.

Confirm `models/tokenizer.json` is also present (it's committed to git, should
already be there).

## Step 2 — Run the app and generate for real

Run the dev build (`npm run tauri dev` or however this repo normally runs
locally — check `package.json`/README if unsure) and generate titles for:

- Keyword `shirt`, categories Article + Blog + YouTube, style Normal, then
  repeat with style Provocative and style Whisper. Engine: Auto.
- Keyword `television`, same categories, style Normal. Engine: Auto.

These are not arbitrary — `shirt` and `television` are the two keywords that
already produced confirmed-bad output earlier in this project (see the audit
report for exact examples), so they're the regression check.

## Step 3 — Confirm the LLM actually ran (don't assume it did)

Two things were just added specifically so this step is possible instead of
guesswork:

1. Every generated title now has a `source` field. In the running app, each
   result card shows a small badge next to its category tags:
   - **"AI · offline"** = the local LLM produced it (`source: "local-llm"`)
   - **"Database"** = EGCG produced it (`source: "egcg-a"`, `"egcg-b"`, or `"egcg-c"`)
   - **"Database (basic)"** = the legacy template fallback produced it (`source: "template"`)
   - **"AI · cloud"** = the cloud API-key path (`source: "ai"`) — shouldn't
     appear unless an API key is configured
2. Settings → "Offline AI engine" row reads **Active** if the LLM loaded, or a
   "not loaded yet" message if it hasn't (check this *after* generating at
   least once, since it lazy-loads on first use, not at app startup).

**If every result shows "Database" / "Database (basic)" and Settings shows
"not loaded," the LLM did not run.** Check the terminal/dev console for lines
starting with `[local_llm]` — `local_llm.rs::load()` logs every failure reason
(`Model file not found`, `Tokenizer not found`, `Failed to read GGUF content`,
etc.) rather than panicking silently. `lib.rs::lazy_load_llm()` checks 6
candidate paths for the model file — if none of them match where your dev
build actually puts resources, that's the fix: add or correct the path, don't
just move the model file around until one accidentally works, since that
won't hold for other developers or the packaged build.

## Step 4 — If the LLM ran, judge its actual output quality

This is the part that hasn't been validated even in concept — nobody has ever
read real output from this model in this app. For each `shirt` and
`television` result tagged "AI · offline":

- Does it contain the keyword (or a close variant)?
- Is it grammatically well-formed?
- Does it plausibly fit the category it's tagged for (article/blog/YouTube)?
- Does it sound like a real title, not a generic sentence?

If quality is poor even though the LLM is definitely running (confirmed via
Step 3), the likely culprits, roughly in order of how easy they are to check:

- **Few-shot examples aren't relevant.** `fetch_curated_examples()` in
  `engine.rs` pulls from the curated-title corpus with a relax-tone-then-genre
  fallback ladder — log what examples it's actually passing into the prompt
  for a couple of test cases and eyeball whether they're on-topic.
- **Sampling is too random.** `sample_token()` in `local_llm.rs` uses
  temperature 0.7 / top-p 0.9. If output is incoherent, try lowering
  temperature (e.g. 0.4–0.5) before concluding the model itself is the
  problem.
- **The model is just too small for this task.** SmolLM2-360M is 360 million
  parameters — genuinely tiny. If output is still bad after checking the two
  items above, this is a real possibility. `LOCAL_LLM_ENGINE_PLAN.md` §1
  named Gemma 3 270M-it as a fallback candidate if SmolLM2 didn't pan out —
  but also consider a slightly larger model in the same family (SmolLM2-1.7B-Instruct
  exists and would still run on CPU, just slower and a bigger download) if
  360M proves too weak. Don't silently swap the model — report back what you
  tried and why.
- **Chat template mismatch.** Already checked and confirmed correct for this
  specific model (`<|im_start|>system\n...<|im_end|>\n<|im_start|>user\n...<|im_end|>\n<|im_start|>assistant\n`
  is SmolLM2-360M-Instruct's actual ChatML format) — shouldn't be the issue,
  but if output looks like the model doesn't understand it's being asked a
  question at all, double check this against the tokenizer_config.json's
  `chat_template` field for the exact bundled model file.

## Step 5 — Measure latency at realistic quantities

Generate 100 titles across 3 categories (max quantity slider value) with the
LLM active. Record wall-clock time. The QC gate retries up to 3x the
per-category target on rejected output, so this could mean several hundred
real inference calls — if it takes more than ~30-60 seconds, that's a UX
problem worth flagging (options: lower the quantity cap for the LLM path,
add a progress indicator, or reduce the retry multiplier).

## Step 6 — Report back

Do not summarize away specifics. Include:

- Actual `cargo build`/`cargo test` output (pass/fail, and any fixes you had
  to make to get there).
- Whether the LLM loaded (yes/no, and if no, the exact `[local_llm]` error).
- The actual generated titles for `shirt` and `television` (all styles
  tested), with each one's `source` badge.
- Actual latency numbers from Step 5.
- Anything you changed that deviates from what's described here or in
  `LOCAL_LLM_ENGINE_PLAN.md` — model swap, prompt changes, hyperparameter
  changes, path fixes. Flag deviations explicitly rather than silently
  deciding; that's caused problems earlier in this project when it wasn't
  done.

## Explicitly out of scope for this pass

`EGCG_Audit_Report.md` §7 documents two unresolved EGCG data issues (a missing
`audience` word pool affecting 35 templates, and the `results` word pool being
misused as a standalone noun phrase in at least one template). Don't fix these
unless you finish everything above with time to spare — the point of this pass
is proving the LLM path works, since that's the path meant to make EGCG's
template-combinatorics problems mostly moot going forward.
