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

### 5a. Revealed-preference position logging — ✅ SHIPPED

`revealed_preference` now records `chosen_rank`, `batch_size`, and
`display_randomized`. Approximately 50% of batches with at least two titles are
shuffled before rendering, and logging uses the actual displayed order. This is
local-only and shipped in commit `8219a19`.

Position bias can now be measured and corrected in revealed-preference data.

Revealed preference remains the primary taste signal because it reflects the
user's actual choice. No telemetry or upload was added.

### 5b. Phi-3.5-mini evaluation — ❌ NO-GO (current CPU)

The isolated bake-off followed `PHI-3.5-MIGRATION.md` without changing
production. The MIT Q4_K_M quant was verified at 2,393,232,672 bytes with the
SHA256 recorded in `CONTEXT.md`. On CPU it loaded in 17.2s; product/laptop
returned valid `SkyBook` in 76.2s, while book/self-help failed after 127.2s and
song/heartbreak failed after 143.2s. Total: 346.6s and 1/3 successful. Qwen is
approximately 7s/title on the same machine, so Phi was about 12x slower.

The required `PER_CASE=8` category-fit run was stopped after 15 minutes without
completion. Verdict: **No-Go for replacing Qwen or shipping Phi**. No production
model constants changed. Harness commits: `b52f3a5`, `27eb6e4`.

A longer one-time download was accepted by the user; the measured per-batch
generation cost was not. Do not pair Phi with Qwen.

### 5c. Release/updater — ✅ BETA CYCLE SHIPPED; CLEAN TEST COMPLETE

Beta.2 through beta.5 were built and released. CI passed across Windows,
macOS, and Linux; Qwen smoke tests and release/signing jobs passed. The Sandbox
confirmed beta.4 installation and beta.5 updater behavior. The updater now uses
check → download → explicit install/restart, with green reserved for up-to-date
and amber for an available/downloaded update.

Remaining release hardening: migrate the updater endpoint from the manually
maintained Netlify `updates.json` to GitHub Releases, and remove dead
`candle-core` / `candle-transformers` / `tokenizers` dependencies during a
separate cleanup pass.

---

## 5a-PRIORITY. Read this first — the order changed on 2026-08-04

**Context:** the web app shipped dual-provider generation and the desktop beta
release/updater cycle is now complete through beta.5. Desktop's next substantive
quality task is Track A judge calibration.

Do these in this order:

| # | Task | Why this order |
|---|---|---|
| **A** | **Tag beta and prove updater** — ✅ COMPLETE through `v1.0.0-beta.5` | Beta.2–beta.5 releases passed CI; Sandbox installation and updater flow were verified. |
| **B** | **Track A — judge calibration** — ❌ **NO-GO** | A0 completed: full self-agreement was 45.7%, below the preregistered 70% gate. Do not continue judge bake-off or build a ranker. |
| **C** | **Phi-3.5-mini evaluation** — ❌ **NO-GO** | Isolated CPU bake-off completed; ~12x slower than Qwen and 2/3 smoke cases failed. Keep Qwen in production. |

**Do NOT pair Phi with Qwen** — see §5c. Desktop has no measured distinctness problem; pairing doubles the binding constraint (time) to solve a problem that is not there.

**Everything the old §6.5 asked for is done** — VC++ fix committed, clean-machine test passed end to end, CI green on three platforms, `release` job dry-run succeeded, Mac/Linux SHA256s published. Do not redo any of it.

---

## 5b. Track A: find a judge that matches the owner's taste

### Track A0 result — ❌ NO-GO (2026-08-04)

The 35-pair swapped retest produced:

- Overall self-agreement: **16/35 = 45.7%**
- Decided-pair winner consistency: **16/16 = 100%**
- Skip stability: **13/19 = 68.4%**
- Cohen's kappa for decide-vs-skip: **0.31**
- No consistent global left/right position bias

The brief's hard rule evaluates the full self-agreement measure, including
skips. Because `c = 45.7% < 70%`, Track A is closed as a valid negative result.
The perfect decided-pair consistency is useful evidence but does not override
the preregistered gate. Do not build a ranker, run a judge bake-off, or use the
judge for ordering/best-of-N. Revealed preference remains the product taste
signal.

The next independent desktop quality task is Phi-3.5-mini evaluation, not judge
selection.

**Why this is the sprint.** Nothing can rank. `calculate_score` is r = −0.04; the DeepSeek judge agrees with the owner **51.6%** on pairs where both titles are already good. Every "generate more and keep the best" idea — best-of-N, dual-provider over-generation, a local ranker — is blocked on this one thing.

**The asset that makes it cheap:** the owner's 123 usable pairwise labels are a **reusable test set**. Any new judge can be scored against them for the price of an API call and none of his time. Judge iteration went from "30 minutes of the owner per attempt" to "one script run".

### ⚠️ Read this before writing any rubric

`tools/feature_bias.py` (committed `14625c2`) — **run it first.** It corrects a wrong bias profile that was doctrine in three documents. Gap = user% − judge%; positive means the judge UNDER-values the feature:

| feature | n | owner picks | judge picks | gap | action |
|---|---|---|---|---|---|
| **colon** | 67 | 63% | 36% | **+27pp** | **DO NOT SUPPRESS — he likes colons more than the judge** |
| len ≥50 | 57 | 63% | 56% | +7pp | leave alone |
| **digit** | 52 | 44% | 69% | **−25pp** | **neutralise** |
| **starts "The …"** | 37 | 46% | 65% | **−19pp** | **neutralise** |
| `$` / parens | 15 / 12 | — | — | — | **INSUFFICIENT (n<20)** — do not act on |

**Neutralise exactly two things: digits and "The …" openings.** Writing rules against colons or length moves the judge *away* from the owner — that was the original error. `$` and parens were the largest numbers in the old list and are below the evidence threshold entirely.

### A0 — Measure the owner's noise ceiling FIRST (blocks every threshold)

**Build `tools/gen_retest_pairs.py`.** Every GO threshold is currently arbitrary because nobody knows how often the owner agrees with *himself*. If he re-labels at 78%, then a perfect judge tops out near 78% and "65%" means 83% of achievable — a completely different conclusion from "barely above a coin flip."

Resample ~35 already-labelled pairs (draw from chose-A, chose-B **and** skipped), **swap A/B presentation order**, reshuffle, emit `judge-retest.html` reusing `gen_judge_pairs.py`'s render function verbatim so the instrument is identical. ~10 minutes of the owner's time.

Outputs: `c` = self-agreement (the ceiling); **his own position bias** — `judge-calibration.html:100` rendered `titleA` on the left with no shuffle, so if he favours a side, all 123 labels carry it; and skip stability.

**Hard rule: if `c` < 0.70, Track A is dead** — there is no stable target to hit — and everything moves to the web tracks. That is a cheap, valid, publishable outcome.

### A1 — Pre-register before running anything

**Do NOT split train/holdout.** Usable labels are `remote work` 21, `coffee` 16, then 48 keywords with ≤3 each. A by-keyword split leaves ~60 with a ±12.6pp CI — a good judge and a coin flip become indistinguishable. **Splitting destroys this dataset.**

Instead write `tools/judge-preregistration.md` **before** any candidate runs: the exact arms, SHA-256 of each rubric file, the primary metric with exclusions fixed, thresholds, and an iteration budget of **3 rubric versions maximum against these 123 labels, ever**. Pre-registration preserves validity *and* full n. The price is few shots; that is the correct price.

### A2 — Freeze the current rubric before writing a new one

Extract the live rubric verbatim to `tools/rubrics/judge_v1.txt` with a header: *every `judge_score` column in every CSV in this repo came from this text; do not edit, add a new file.* New rubrics are new files; new data gets a **new column**, never an overwrite. Otherwise a rubric change silently invalidates `bench-usability.csv`, `bench-production.csv`, every gate in `AI-WORK-BRIEF.md`, and the `judge_v1` floor-gate use that §6.2 6c explicitly preserves.

### A3 — The bake-off

Ask the judge **pairwise** for validation (it matches how the owner labelled) **and** pointwise (the product needs a scalar), then measure the gap — a win that only exists in the pairwise format does not reach users.

- **Primary arm (carries the GO alone):** strongest Anthropic model + `judge_v2_pairwise`
- **Secondary (exploratory, Holm-corrected, cannot trigger GO):** OpenAI / Gemini / GLM
- **Control — `deepseek` + `judge_v1` must reproduce 55.3% / 51.6%. If it does not, STOP: the harness is broken and nothing else means anything.** (Hard rule #1, cheapest possible instantiation.)
- **Control — `deepseek` + v2** isolates rubric effect from provider effect. Without it a v2 win is un-attributable.

**Order swapping is mandatory** — ask every pair twice, (A,B) and (B,A). Swap consistency <80% disqualifies an arm before agreement is even looked at. Order-disagreements become ties at 0.5, never dropped; dropping them removes the hard pairs and inflates every arm.

Emit results in **exactly the schema `calibrate_judge.py` already consumes**, so that script is reused **unmodified** — the harness that killed the old judge adjudicates its replacement.

### A4 — What the numbers can and cannot settle

At n=123: SE ≈ 4.5pp, CI ±8.8pp, so **beating chance needs ≥57.4%**. But **the paired McNemar test against v1 on the same items is far more powerful — a 10pp improvement over v1 IS detectable, while 10pp above a coin flip is not.** Report McNemar as the primary inferential statistic and the raw rate as descriptive. This roughly doubles what these labels can settle, for free.

**Selection bias:** best-of-4 arms inflates the winner by ≈5pp — the same size as the effect being hunted. Hence one primary arm; a winning secondary earns only the right to be re-tested on fresh labels.

**Run in parallel — highest expected value in this sprint:** collect **200 fresh labels**, sampled **judge-blind**. The existing pairs were binned on DeepSeek's *own* score gaps with the 16-19 band deliberately excluded, so the current test set is DeepSeek-conditioned. Cap per-keyword contribution and floor thin categories (`song` has 2, `podcast` 7).

**GO** = swap consistency ≥80% **and** ≥65% overall **and** ≥62% in the ≥70 band **and** McNemar significant **and** no OVER-rewarded feature gap >20pp at n≥20 **and** pointwise within 5pp of pairwise.
**CONDITIONAL (58-65%)** = usable as a broken-vs-fine floor gate only. **No user-visible ordering.**
**NO-GO (<58%)** = record it in §5 with the rigour of the 2026-08-03 kill entry; revealed preference (now logging `chosen_rank` with randomised display) becomes the plan.

---

## 5c. Phi-3.5-mini + Qwen paired — NOT recommended

The owner asked whether the web's dual-provider idea ports to desktop by pairing Phi with Qwen. Clear answer: **it solves a problem desktop does not measurably have.**

| | Web | Desktop |
|---|---|---|
| distinctness | **70 distinct per 100** — a real defect | **25/25 unique**; 0 duplicates across 27 titles in six category-fit runs |
| actual bottleneck | variety | **category-fit ceiling** (song/poem/book) + wall-clock time |

Desktop's measured defect is that a 1.5B cannot hold category conventions — a **bigger** model fixes that, not a *second* one. **Phi replacing Qwen** targets the real problem (`PHI-3.5-MIGRATION.md` specs it). **Phi plus Qwen** doubles the already-binding constraint: Studio 200 goes ~11 min → ~28 min for Phi alone and worse paired (that figure was judged unshippable once); ~3.4 GB of weights resident plus two KV caches, **unmeasured on an 8 GB machine**; and a single `Mutex<Option<LocalLlm>>` slot with ~15 call sites assuming one model.

**Two cheap checks that could flip this verdict:**
1. **Studio-scale distinctness has never been measured** — 25/25 is Core scale. Raise `PER_CASE` in `tests/category_fit.rs` and measure at 200. If it degrades like the web's 100 did, pairing becomes relevant *for Studio only*.
2. **Try the free version first** — the same model with partitioned frames and varied sampling behaves like two mildly different models. Constraint rotation already works here. Zero RAM, zero download, zero extra time.

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
