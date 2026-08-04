# Handoff — desktop app (`titleforge-desktop/`)

> ## ⚠️ FIRST: history was rewritten and force-pushed on 2026-08-03
>
> **Before you touch anything, reset your clone:**
>
> ```bash
> git fetch origin && git reset --hard origin/master
> ```
>
> **Do NOT commit or push on the old history** — it re-introduces what was
> removed and creates a divergent tree.
>
> Why: commits by the reviewing agent carried a `Co-Authored-By: Claude`
> trailer that put "claude" in the GitHub Contributors panel. Removed at the
> owner's instruction. 5 of 206 commits were rewritten, all from 2026-08-03;
> everything older is untouched. **Trees are byte-identical — no file content
> changed, no work lost.** Your commits are all present with the same messages
> and content, at new hashes. Recovery tag: `backup-before-trailer-strip`.
>
> Commit hashes cited later in this file are the OLD ids. Current equivalents:
>
> | old | new | commit |
> |---|---|---|
> | `3b3c97a` | `734c51d` | category / fine-tune / genre / style plumbed in |
> | `b398ca6` | `9eaf68d` | measured on real Qwen; guards split hard vs soft |
> | `a8e49f7` | `f049974` | near-duplicate dedup; colon cap reverted |
>
> See `CONTEXT.md` §5 (2026-08-03 end of day) for the full record.

> Written 2026-08-03 by the reviewing agent. **You implement; I audit and
> diagnose.** This repo is yours to work in — an earlier draft of
> `titleforge/HANDOFF-WEB.md` implied otherwise, which was an error.
>
> Web-side equivalent: `titleforge/HANDOFF-WEB.md`.

---

## 1. State — three commits landed today

| Commit | What |
|---|---|
| `3b3c97a` | Category conventions, fine-tune / genre / style plumbed into the offline engine |
| `b398ca6` | Measured on the real Qwen model; guards split hard vs soft |
| `a8e49f7` | Near-duplicate dedup; colon-proportion cap tried 3 ways and reverted |

`cargo test --lib` → **33/33**. Full write-up in `CONTEXT.md` §5 (two entries
dated 2026-08-03, the later one supersedes part of the earlier).

### What was broken and is now fixed

- **Category was a LABEL, not a constraint.** The prompt substituted a bare
  `{category}` word. Cross-category word-length range was 2.65 (collapse); it is
  now ~7.0.
- **Name categories were structurally impossible.** `product`, `childname`,
  `character`, `street` could not return a single valid result: the ≥2-word QC
  floor discarded one-word names ("Vivid") and the drift guard required the
  keyword *inside* the title. `product` now returns real names in every measured
  run (8/8 each time) — `Harmony Brew`, `SourdoughNest`, `Zen Brew`.
- **Fine-tune, genre and style were dropped on the floor.** `generate_titles`
  had no `finetune` parameter at all — the UI showed the controls and the engine
  ignored them. Style was passed as a raw token ("whisper") into the prompt text.
- **Dedup was exact-match only**, so a book batch came back as four variations
  on "Remote Revolution".

### The model is installed

`%APPDATA%\titleforge-desktop\models\qwen2.5-1.5b-instruct-q4_k_m.gguf`,
SHA256-verified against the pinned constant in `lib.rs`. `LocalLlm::find_model`
locates it (repo `../models`, then the OS data dir, or `TF_MODEL_PATH`). You do
not need to re-download it.

---

## 2. 🛑 Do not re-attempt these — all measured, all failed

**Read this before planning any prompt work.** Each of these cost real time.

1. **Colon-proportion cap.** Tried three ways on 2026-08-03:
   - *instruction* ("Do not use a colon in this one") → blog colons went **UP**
     50%→75%, poem word-band conformance collapsed 67%→25%. A 1.5B does not
     follow negative instructions, and it displaced the rotated diversity
     constraint, which was doing real work.
   - *soft rejection* → book stayed at 75%; Qwen emits a colon on nearly every
     book attempt, so all 3 attempts get rejected and the fallback returns one
     anyway. Headline metrics regressed.
   - *hard rejection* → works (75%→0%) but costs **18% of all output**.

   There is a do-not-re-attempt block in `engine.rs` with these numbers.
   **Books keep both forms** (user decision: "the book can have both").

2. **Multi-constraint prompt rules on the 1.5B.** Measured 75.2 and 77.6 against
   an 81.0 baseline, twice. Closed.

3. **`song` / `poem` / `book` category fit.** Still headline-shaped. Four
   independent routes have now hit the same 1.5B capacity ceiling. The lever is
   a bigger model, not more prompt engineering.

4. **The ranker / the 5,000-label dataset.** Killed by the judge calibration
   result — DeepSeek-as-judge agrees with the user **51.6%** on pairs where both
   titles are already good (n=91, coin flip), Elo r = +0.019. A structural ranker
   *clears* the brief's `r ≥ 0.35` gate while agreeing with the user at chance,
   because the judge is cheaply predictable from `$`/digits/length.
   `CONTEXT.md` §6.2 item 6c.

5. **`LlamaSampler::grammar()`** on llama-cpp-2 0.1.153 — fails on our prefill
   batch shape, and is no longer needed.

---

## 3. Design rules to preserve

- **`prompt_spec.rs` conventions REPLACE vague instructions, never stack on
  them.** Net instruction count per generation stays flat. This exists *only*
  because a 1.5B cannot hold multi-constraint prompts — it is a desktop rule and
  does not apply to the web app, which keeps its full QUALITY RULES block.
- **Guards are split by severity.** NAME-shape is HARD (a headline returned for
  `product` was the user-reported bug). Mood colon/digit checks and the
  exemplar-echo guard are SOFT: `generate_one_clean` keeps the first
  soft-rejected candidate and returns it if the 3-attempt budget runs out, so a
  stylistic preference can never produce an empty slot. Brief §5 rule 4: *empty
  output is a failure, not a skip.*
- **Name categories are exempt from the keyword drift guard** — a brandable name
  deliberately does not contain the keyword. `passes_name_shape()` carries the
  QC weight instead. The trade-off (names unguarded against topical drift) is
  accepted and documented in the code.
- Name categories get **no SEO score** (scoring "Vivid" against Amazon's 60–100
  char band reported ~15 for a correct answer) and **no curated fallback** (the
  corpus is titles, not names).
- **`CATEGORY_CONVENTIONS` in the web `generate.js` mirrors `prompt_spec.rs`.**
  Change one, say so.

---

## 4. Verification you must run

```bash
cargo test --lib                                        # 33/33
cargo test --release --test category_fit -- --nocapture # ~5 min, real model
```

`tests/category_fit.rs` reports per-category word-band conformance, colon/digit
rates, cross-category range/stdev, and **fire rate**. It makes **no judge API
call** by design — category fit is objective, and the judge failed calibration.
Do not "improve" it by adding judge scoring.

Baselines (Qwen, 6 runs, CSVs committed as `category-fit-run*.csv`):

| metric | value |
|---|---|
| fire rate | 93–100% |
| cross-category range | 6.4–7.6 |
| inside word band | 85–92% |
| `product` correctness | 8/8 every run |

**Watch fire rate on any QC change.** Adding rejection reasons to a fixed
3-attempt budget is how the cliché filter cut output 50→34 on 2026-08-02 and how
the hard guards cost 18% on 2026-08-03. It is the standard failure mode here.

**n is 2–4 per category.** Raise `PER_CASE` to at least 8 before concluding
anything except the `product` result. Run twice — one run is an anecdote
(brief rule #7).

---

## 5. Next tasks, in order

### 5a. Revealed-preference position logging — DO THIS FIRST

`revealed_preference` (`lib.rs:286`) records the chosen title and the passed-over
ones, but **not the position each was displayed at**. When a user favourites the
2nd title of 25, we know *what* they picked and not *that it was shown 2nd*.

Position bias dominates click data — people pick from the top. Without rank,
every label is confounded and **cannot be corrected afterwards**. There is zero
data today, so this is free to fix now and unrecoverable once beta testers start
clicking.

Add: displayed rank, batch size. Ideally randomise display order for a slice of
batches — that turns favourites from correlational into near-experimental data.
Stays local-only; no telemetry, no upload. That promise is what the product is
sold on.

This matters because revealed preference is now the **primary** label source —
it is the only one measured to reflect the user's actual taste.

### 5b. Phi-3.5-mini evaluation

Full spec in `PHI-3.5-MIGRATION.md`. The headline risk: ~2.5× the parameters, so
Studio 200 goes from ~11 min to a projected **~28 min**, which is probably
unshippable. A longer one-time *download* is accepted by the user; a longer
*generation* on every batch is not the same thing and must be measured first.

### 5c. Still queued (unchanged, `CONTEXT.md` §6.5)

Auto-updater has never completed a real install→update cycle. Remove the dead
`candle-core` / `candle-transformers` / `tokenizers` deps. Tag the beta.

---

## 6. How review works

Flag me through the user when something is ready. I check claims against the
code, the git log and the measurements — not against commit messages. Several
change-log entries in this repo have been wrong before (a claimed `n_ctx=1024`
that was never applied, a "SHIPPED" that wasn't primary, a benchmark measuring a
path users never hit), which is why the standing instruction is to verify.

Useful things to make review fast: say plainly what is **measured** vs
**implemented**, put the numbers in the commit message, and record results that
went the wrong way. A failed variant is a result — it closes off a direction.
Three of the six runs behind `a8e49f7` were failures and they are all written up.
