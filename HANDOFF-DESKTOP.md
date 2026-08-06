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

Baselines (Qwen, **7** runs, CSVs committed as `category-fit-run*.csv`). Recomputed
from the CSVs 2026-08-06 — the previous band excluded run7:

| metric | value | note |
|---|---|---|
| fire rate | 93–100% | run7 = 55/56 = 98% |
| cross-category range | 6.4–**7.62** | run6 7.62, run7 7.50 |
| inside word band | **83–92%** | was stated 85–92%; **run7 measured 83.6%** |
| `product` correctness | 8/8 every run | run6 8/8, run7 16/16 ✅ reproduced |

**On the word band:** 83.6% (46/55) sits just outside the old floor. At n=55 that is
**inside sampling noise — it is NOT a demonstrated regression.** But the post-eog-fix
check recorded in `CONTEXT.md` looked only at fire rate, range and stdev; nobody
checked the band. Widen the band as written here and check all four next time.

**Watch fire rate on any QC change.** Adding rejection reasons to a fixed
3-attempt budget is how the cliché filter cut output 50→34 on 2026-08-02 and how
the hard guards cost 18% on 2026-08-03. It is the standard failure mode here.

~~**n is 2–4 per category.** Raise `PER_CASE` to at least 8~~ — **STALE, corrected
2026-08-06.** `category_fit.rs:40` has been `const PER_CASE: u32 = 8` since `27eb6e4`.
Run twice — one run is an anecdote (brief rule #7).

---

## 5. Task status (historical — **current priorities are §7, not this section**)

> **Renumbered 2026-08-06.** These were `5a`/`5b`/`5c`, which collided with the
> later `§5b Track A` and `§5c Phi+Qwen` — and `CONTEXT.md:1895`/`:1905` and
> `AI-WORK-BRIEF.md:183`/`:205` all cite the *later* pair. Renumbered `S1`-`S3`
> so every external reference still resolves. Do not renumber the later ones.

### S1. Revealed-preference position logging — ✅ SHIPPED

`revealed_preference` now records `chosen_rank`, `batch_size`, and
`display_randomized`. Approximately 50% of batches with at least two titles are
shuffled before rendering, and logging uses the actual displayed order. This is
local-only and shipped in commit `8219a19`.

Position bias can now be measured and corrected in revealed-preference data.

Revealed preference remains the primary taste signal because it reflects the
user's actual choice. No telemetry or upload was added.

### S2. Phi-3.5-mini evaluation — ⚠️ **THE NO-GO WAS RETRACTED. THIS SECTION WAS WRONG.**

> **DELETED 2026-08-06.** Everything that stood here — *"76.2s"*, *"1/3 successful"*,
> *"about 12x slower"*, *"No-Go for replacing Qwen"* — measured a **stop-token bug**
> (`token_eos()` vs `is_eog_token()`) compounded by an **unoptimised debug build**.
> Fixed in `1dd414f`. Corrected figure: **~2.5× Qwen, not ~12×.**
>
> **This file argued both sides of Phi for two days.** §7 D5 said *"Phi is unevaluated,
> not rejected — do not carry '12× slower / produces garbage' forward"* while this
> section, which a reader hits **first** under a heading that said "next tasks, in
> order", still delivered the dead verdict. That is how a retracted number gets
> re-cited as doctrine.
>
> **The live position is §7 D5. There is no Phi verdict in this section any more.**
> The honest state: *Phi costs ~2.5× the time for untested quality benefit.* It
> produced clean output whenever it completed (`BrewBliss`, `LapTech Pro`,
> `CaffeineCraft`). Its quality has **never** been fairly measured.
>
> Harness commits `b52f3a5`, `27eb6e4`; the eog fix is `1dd414f`. No production
> model constants were changed at any point — that part was always correct.

### S3. Release/updater — ✅ BETA CYCLE SHIPPED; CLEAN TEST COMPLETE

Beta.2 through beta.5 were built and released. CI passed across Windows,
macOS, and Linux; Qwen smoke tests and release/signing jobs passed. The Sandbox
confirmed beta.4 installation and beta.5 updater behavior. The updater now uses
check → download → explicit install/restart, with green reserved for up-to-date
and amber for an available/downloaded update.

Remaining release hardening: the updater endpoint migration (Netlify → GitHub
Releases) is DONE at the code level (`tauri.conf.json` endpoint + CSP
`github.com`/`objects.githubusercontent.com`, release workflow Netlify step →
documented no-op; commit `cfa5d11`) and dead `candle-*`/`tokenizers` deps are
removed (same commit). ⛔ The beta tag + live in-app updater validation against
the new GitHub endpoint still awaits the owner (needs a release tag and a
Sandbox run).

---

## 5a-PRIORITY. Read this first — the order changed on 2026-08-04

**Context:** the web app shipped dual-provider generation and the desktop beta
release/updater cycle is now complete through beta.5. Desktop's next substantive
quality task is Track A judge calibration.

Do these in this order:

| # | Task | Why this order |
|---|---|---|
| **A** | **Tag beta and prove updater** — ✅ COMPLETE through `v1.0.0-beta.5` | Beta.2–beta.5 releases passed CI; Sandbox installation and updater flow were verified. |
| **B** | **Track A — judge calibration** — ❌ **NO-GO** | A0 completed: full self-agreement **42.9%** (was recorded 45.7% — corrected 2026-08-05, see §5b), below the preregistered 70% gate. Do not continue judge bake-off or build a ranker. |
| **C** | **Phi-3.5-mini evaluation** — ⚠️ **UNEVALUATED, NOT REJECTED** | ~~"~12x slower, 2/3 smoke cases failed"~~ — **both figures retracted 2026-08-05.** They measured a stop-token bug plus a debug build (`1dd414f`). Corrected: **~2.5×**. Keep Qwen in production, but the reason is cost, not failure. Live position: §7 D5. |

**Do NOT pair Phi with Qwen** — see §5c. But **the old reason was wrong**: *"desktop has no measured distinctness problem"* rested on a null metric (`engine.rs:158-161` rejects duplicates before they are recorded, so 0 duplicates is guaranteed). The surviving reason is **time**, which is now the binding gate on its own. If a second distribution is wanted, it **replaces** Qwen rather than supplementing it.

**Everything the old §6.5 asked for is done** — VC++ fix committed, clean-machine test passed end to end, CI green on three platforms, `release` job dry-run succeeded, Mac/Linux SHA256s published. Do not redo any of it.

---

## 5b. Track A — ❌ CLOSED 2026-08-05. Do not run any of the below.

**A0 measured the owner's noise ceiling and it failed the gate.** Everything from A1 onward (pre-registration, rubric v2, the provider bake-off, ensembling) is **cancelled**. The spec is kept below only as a record of what was designed and why it is moot.

**The numbers, recomputed by the reviewing agent from the raw retest data** (the originally-recorded figures were not reproducible — `gen_retest_pairs.py` writes the *swapped* display order, so comparison must be by title text, not slot letter):

| | value |
|---|---|
| owner vs himself, decided pairs | **5/8 = 62.5%** (recorded as 16/16 = 100% — not reproducible) |
| owner vs himself, all 35 | **15/35 = 42.9%** |
| preregistered gate | `c ≥ 0.70` |

**The reframe that matters more than the No-Go:** the DeepSeek judge agrees with the owner **55.3%** on decided pairs; the owner agrees with **himself 62.5%**. The judge was at ~89% of the achievable ceiling. **It was never the broken component — the target is unstable.**

That is why A1-A4 are cancelled rather than deferred: a better rubric, a better provider, or an ensemble would all be chasing a ceiling that is roughly where the current judge already sits. **Revealed preference is the only remaining taste signal** — behaviour rather than stated preference, already accruing with randomised display order since `8219a19`.

**Caveat, stated because it should not be over-read:** n=8 decided-both-times is very small. The honest claim is "the ceiling is low and we do not know precisely how low", not "the ceiling is 62.5%". It also means every figure derived from the original 123 labels — including `tools/feature_bias.py` — is **directional, not precise**.

---

<details>
<summary>Original Track A spec (superseded — kept for the record)</summary>

### Track A: find a judge that matches the owner's taste

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

</details>

## 5c. Phi-3.5-mini + Qwen paired — NOT recommended

The owner asked whether the web's dual-provider idea ports to desktop by pairing Phi with Qwen. Clear answer: **it solves a problem desktop does not measurably have.**

> **⚠️ REWRITTEN 2026-08-06. The conclusion survives; the argument for it did not.**
> Both the distinctness premise and the timing arithmetic below were wrong.

| | Web | Desktop |
|---|---|---|
| distinctness | **70 distinct per 100** — a real defect | ~~25/25 unique; 0 duplicates~~ → **NULL METRIC, see below.** On yield: **106-115/160** (yield curve), **124/200** (Studio). Desktop has the same defect. |
| actual bottleneck | variety | **wall-clock time** (now the hardest gate, §6.4b item 4) + category-fit ceiling (song/poem/book) |

**The "no distinctness problem" premise was never established.** `engine.rs:158-161`
rejects exact matches *and* `shares_opening(n=2)` **before anything enters the pool**,
so any CSV of engine output shows 0 duplicates by construction. Verified across all
seven category-fit runs: 0 duplicates in 211 titles — because the metric cannot show
one. A premise that cannot be falsified by its own evidence is not a finding. The
metric that *can* show it is **yield**, and every run that used it found a deficit.

**The timing arithmetic was wrong by 2×.** The old text said *"Studio 200 goes ~11 min
→ ~28 min for Phi alone"*. The ~11 min came from a phantom 3.4 s/title; the measured
baseline is 6.79 s/title (169.6s for 25) and the Studio run measured 7.87 s/attempt.
Corrected: **Qwen Studio 200 ≈ 23-26 min; Phi alone at ~2.5× ≈ 50-67 min; paired,
worse still.** Plus ~3.4 GB resident across two models with two KV caches,
**unmeasured on an 8 GB machine**, and a single `Mutex<Option<LocalLlm>>` slot with
~15 call sites assuming one model.

**Status of the two checks that could have flipped this:**
1. ~~**Studio-scale distinctness has never been measured**~~ — **IT WAS, on 2026-08-06
   (`5940dd2`): 200 requested → 124 delivered.** The flip condition named here was met.
   **Nobody came back to this section to say so.** It also did not use `category_fit`
   as instructed; it used a new harness that writes no CSV, so the result must be
   re-taken before it counts. See §7 D2.
2. **Try the free version first** — still the right first move, still not tried. The
   same model with partitioned frames and varied sampling behaves like two mildly
   different models. Constraint rotation already works here. Zero RAM, zero download,
   zero extra time.

**Net: still do not pair.** But the reason is time and RAM, not an absent defect —
and if a second distribution is ever wanted here, it **replaces** Qwen.

## 6a. How review works

Flag me through the user when something is ready. I check claims against the
code, the git log and the measurements — not against commit messages. Several
change-log entries in this repo have been wrong before (a claimed `n_ctx=1024`
that was never applied, a "SHIPPED" that wasn't primary, a benchmark measuring a
path users never hit), which is why the standing instruction is to verify.

Useful things to make review fast: say plainly what is **measured** vs
**implemented**, put the numbers in the commit message, and record results that
went the wrong way. A failed variant is a result — it closes off a direction.
Three of the six runs behind `a8e49f7` were failures and they are all written up.

---

## 6. NEXT MEASUREMENT — "how many titles can we honestly promise?"

**The question:** what is the largest number of titles per (keyword, category) that are *all* distinct and *all* usable? That number is the honest product promise, and right now nobody knows it.

### ⚠️ Read first: the previous answer to this was wrong

CONTEXT.md §5 (2026-08-03) records a depth curve — *"ranks 1-5 mean 81.2 → 21-25 mean 64.4"* — and it has been cited as evidence that quality decays with batch depth. **Do not build on it.** Re-audited 2026-08-05:

1. **It is 1 of 2 keywords.** The same run measured `remote work` and found *no ordering at all*: ranks 6-10 averaged 50.6, ranks 11-15 averaged 85.2.
2. **"Rank" was `calculate_score` order, not generation order.** `engine.rs:186` sorts the pool by score before returning, and `calculate_score` correlates **r = −0.04** with quality. Those buckets are positions in a noise-sorted list. A clean decline under a noise sort is probably coincidence; the flat second keyword is what a noise sort predicts.
3. **The stated mechanism does not exist.** "Depth exhaustion" implies the model runs out of ideas as a batch progresses. But **every title is an independent call** — `generate_one_clean` per title on desktop, independent 5-title chunks on web. No title sees any other. There is nothing to exhaust.

**The real mechanism is dedup pressure on a fixed distribution.** Sampling repeatedly from one model for one keyword produces increasing collisions; dedup rejects them; later slots keep whatever survived, which skews weaker. This predicts the web result exactly — one provider yielded ~70 distinct per 100, two providers yielded 100/100, because a second distribution adds distinct mass rather than depth.

**Consequence for the design: the limit is DISTINCT MASS per distribution, not depth.** So the measurement must count *distinct usable yield*, not score-by-position.

### What to measure

For a single (keyword, category), generate progressively and record, **in acceptance order**:

- `attempt_index` — how many generations were requested so far
- `accepted_index` — position among titles that survived dedup
- `title`, `judge_score`, `usable` (≥70)
- `rejected_reason` when dropped (duplicate / QC / drift)

Then report:

1. **Distinct-usable yield curve:** cumulative count of accepted titles scoring ≥70, against attempts. **The number where this curve flattens is the answer.**
2. **Marginal quality:** mean judge score of accepted titles in each successive block of 10, in acceptance order. Detects whether late survivors are genuinely weaker.
3. **Rejection mix by block** — if late rejections are mostly `duplicate`, the ceiling is distinct mass (fixable with a second provider/model). If mostly QC/drift, it is model quality (not fixable that way). **This distinction decides whether dual-provider helps.**

### Method requirements — these are what make it valid

- **Preserve acceptance order. Do NOT sort by score.** On desktop that means capturing order *before* `engine.rs:186`, or temporarily disabling the sort in the harness. Sorting by `calculate_score` is what invalidated the previous attempt.
- **Judge for AGGREGATE MEANS ONLY.** This is consistent with §6.2 6c — *"Use it for pass-rate/drift/floor gating only. Every historical mean and tail number remains meaningful."* Block means over ~10 titles average out per-title noise. **Do not** use the judge to order individual titles; that is the use that failed calibration.
- **Reuse `call_judge()` from `bench_batch_quality.rs` verbatim.** A new rubric makes the numbers incomparable with every prior measurement.
- **≥3 keywords, ≥2 categories.** The previous attempt used 2 keywords and the two disagreed. One keyword is an anecdote.
- **Run twice** (hard rule #7).
- Target **N = 60 attempts** per (keyword, category) — comfortably past any plausible ceiling without unbounded runtime.

### Cost / runtime

- **Web:** ~60 titles per cell is well within the existing cloud harness. Judging ~360 titles is a few dollars at most. Use `scripts/measure-batch-uniqueness.js` conventions (Pro token, 65s backoff on 429).
- **Desktop:** Qwen at ~6.8s/title ⇒ 60 titles ≈ 7 min per cell; 6 cells ≈ 40 min per run, ×2 runs. Acceptable unattended.

### What the answer changes

The current promise is *"up to 100 titles"* (web Pro) and 25/50/200 (desktop tiers). Those are **capacity** claims with no evidence behind them.

Once the yield curve exists, the honest promise becomes a per-category number — e.g. *"up to N per category"* — and the request cap can be `min(requested, N × categories_selected)`, which is both defensible and reads as a quality claim. **§6.4b item 5 requires every sales-page claim to match measured reality before payments switch on; this is the measurement that satisfies it for batch size.**

### What NOT to conclude

- Do not report a "cliff" unless the yield curve actually shows one. A gentle slope is a different product answer from a cliff.
- Do not compare desktop and cloud numbers to each other — different models, different pipelines.
- If late rejections are dominated by `duplicate`, that is **not** a quality ceiling and must not be reported as one.

---

## 7. NEXT ACTIONS — prioritised 2026-08-05 (read this before picking work)

**Owner confirmed:** `TF_DUAL_ENABLED=1` **is set in Netlify**, so production serves the dual-provider path. Cross-provider overlap **was measured** — but the result is lost (see W1). Both items are struck from the open list.

### ✅ Already done — do not redo

| item | evidence |
|---|---|
| Release pipeline end to end | 4 real releases, beta.2→beta.5, full signed artifact sets |
| Auto-updater install→update cycle | beta.4 installed in Sandbox, beta.5 as target |
| First-launch download on clean machine | verified 2026-08-02 |
| Cloud batch behaviour measured | dual-provider 100/100 distinct, 11.5–15.8s |
| Timeout budget hierarchy (old "C1") | `PROVIDER_TIMEOUT_MS`=11000 + `deadlineAt`, `generate.js:19,213` |
| SEO port tested | `check-seo.js`, 9 tests ported |
| Desktop yield measured | 2 runs, `yield-curve-run*.csv` |

**§6.4b gates 1, 2, 3, 6 and now 8 are closed. Three remain: 4, 5, 7.** Those three are the critical path to switching payments on.

---

## ⛔ READ THIS BEFORE ANYTHING ELSE — corrections from the 2026-08-06 audit

**Audited: `5940dd2`, `ebfad93`, `cfa5d11`, `dbee1f1`. Full working in `CONTEXT.md` §5, entry dated 2026-08-06 (review).**

**First, what was right.** The updater migration, the dead-dep removal and the CORS/rate-limit change are all correct in code and I reproduced them. `product` 8/8 and 16/16, cross-category range 7.62/7.50, the 33 lib tests, the position-logging schema, the ~50% shuffle — all recomputed from the CSVs and the source, all hold. **And the Track A rewrite in §5b is the standard I want copied everywhere:** it states the corrected number, names the superseded one, quarantines the dead spec in `<details>`, and volunteers its own n=8 caveat. That is how a retraction should read.

**Now the five things to fix. Each one broke a rule that is already written down in this file.**

**1. The Studio measurement did not produce evidence. Re-take it.**
`studio_batch_measure.rs` is `eprintln!`-only and `5940dd2` committed no artifact. **124, 26.2 min and 121+3 exist only in a terminal that is gone.** Standing rule #1, below, written one day earlier: *"A measurement that only prints to stdout has not been taken."* Every other harness in this repo writes a CSV; `yield_curve.rs:83,172` writes one with a per-attempt `outcome` column. This is W1 repeating at 26 minutes a run.

**2. Two of the three headline numbers cannot fail, so they are not findings.**
`engine.rs:158-161` rejects exact matches *and* `shares_opening(n=2)` before anything enters the pool. **"124/124 distinct exact" is guaranteed by the engine.** So is "0 duplicates across N titles" — the premise §5c was built on. I checked all seven category-fit runs: 0 duplicates in 211 titles, because observing one is impossible. **Before reporting a metric, ask what value would falsify the claim. If none can, it is not a measurement.** The metric that works is yield.

**3. You measured with a weaker rule than the engine's, and `engine.rs:13-15` tells you not to.**
The harness scores distinctness on a raw 4-word signature — no punctuation stripping, no function-word filter — while the engine dedups on 2 content words. The comment in `engine.rs` says it plainly: *"A measurement that reimplements the rule measures its own reimplementation."* `yield_curve.rs` gets this right by calling `engine::shares_opening` directly. Do the same.

**4. You reported a cause the harness cannot see.**
*"Dedup collisions consumed the iteration budget; QC rejected ~nothing"* — nothing in `studio_batch_measure.rs` records a rejection outcome. That claim was imported from `yield_curve.rs`. The wall clock weakly supports it (7.87 s/attempt ≈ one successful call), but that is my inference, not your measurement. **And "QC rejected ~nothing" is partly true by design:** `local_llm.rs:288-291` returns the first soft-rejected candidate when the 3-attempt budget runs out, so soft QC failures structurally cannot appear as losses. **Separate what the harness observed from what you concluded, in the commit message and in the entry.**

**5. You reported against a baseline you had already contradicted in the same commit.**
The entry called 26.2 min *"worse than the ~11 min estimate"*. That estimate was never real — it assumed ~3.4 s/title against a measured **6.79 s/title** recorded at `CONTEXT.md:1284` and `:1295`. **Your own harness header says "Expected ~23 min at ~6.8s/title."** You had the right prior in the file you were writing. Measured 26.2 against a correct prior of 23 is a **14% miss, not a 2.4× surprise**. The finding is real; the framing inflated it, and the same phantom number has now corrupted the Phi tier maths (`CONTEXT.md:457` says Studio 200 ≈ 28 min while `:455` says Phi is 15-20 s/title — 200 × 15s is 50 min; the two cannot both be true). **When a result surprises you, check the baseline before you write the word "worse".**

**Also:** n=1, against hard rule #7. And `dbee1f1` closed a payment gate with **nothing written down anywhere** — §6.2 #13, §6.4b item 8, §6.5, `AI-WORK-BRIEF.md` §6 and D4 in both handoffs all still said it was open. I have corrected all of them. **Shipping is half the task; the record is the other half.**

### Do these in this order. Do not reorder without saying why.

1. **D1 first — raise the multiplier.** Everything downstream depends on it. Do not re-measure Studio before this lands; `iteration_budget = target_per_cat × mult`, so the 26.2 min number roughly doubles the moment D1 ships and any decision taken on it now gets taken twice.
2. **Rebuild `studio_batch_measure.rs` in `yield_curve.rs`'s shape** — per-attempt CSV with an `outcome` column, `engine::shares_opening` rather than a reimplemented rule, and report **yield** as the headline, not exact-uniqueness.
3. **Run it twice** (rule #7). ~2 × 52 min unattended after D1. Commit both CSVs.
4. **Then, and only then, bring the Studio cap to the owner** with: yield at `mult=2`, wall clock, and the duplicate:QC split. Do not change caps unattended — you were right about that.
5. Then D3 (Stripe licence flow) and W2 (cloud yield), which are the last two gates.

**Do not touch Phi until step 4 is done.** At the corrected figures it is ~50-67 min for Studio 200, and the tier maths that made it look borderline was wrong by 2×.

---

### CRITICAL PATH — everything below feeds §6.4b

#### W1. Make `measure-provider-overlap.js` persist its result, then re-run *(small, do first)*

The overlap measurement **was run**, but `scripts/measure-provider-overlap.js:130` only does `console.log` — no file is written. The number went to a terminal and is gone. **Every other measurement script in this project writes a CSV** (`category-fit-run*.csv`, `batch-uniqueness.csv`, `yield-curve*.csv`); this one is the exception and that is why the result was lost.

Add CSV output matching those conventions, re-run once, and record the number in `CONTEXT.md` §5. It is what tells you whether **1.5× overgeneration is correct or merely lucky** — low overlap means you could drop to ~1.2× and save tokens; high overlap means 1.5× is barely enough.

**Rule going forward: a measurement that only prints to stdout has not been taken.**

#### W2. Cloud yield measurement *(the big one — sets the web promise)*

Full spec in §6 above. Desktop is now measured; **cloud is not**, and the numbers do not transfer — different model, different pipeline.

Desktop found: **53:1 and 44:1 duplicate-to-QC**, i.e. the ceiling is repetition, not quality, and **10 per category** was the only figure delivered in 8/8 cell-runs. Run the equivalent on cloud. Needs a Pro token (`TF_PRO_TOKEN`); guest is capped at 10 titles so it cannot measure itself.

**This blocks §6.4b item 5** — you cannot make the sales page match reality until you know what the web engine actually delivers per category.

#### W3. Sales copy ← blocked on W2 *(§6.4b item 5)*

Current claim is *"up to 100 titles"*. Once W2 lands, replace with a per-category number and cap requests at `min(requested, N × categories_selected)`.

Desktop's evidenced number is **10 per category** (8/8), target 20 internally with ~2× headroom. With 16 categories that is 160 at full spread, so the headline is unaffected. Do not copy desktop's 10 to the web page — use W2's number.

#### D1. Raise the desktop multiplier from 1× *(measured need)*

`engine.rs:77` is `let mult: usize = 1`. The yield data says that under-delivers **by construction**: a 20-title request is 20 attempts, which produced as few as **9** distinct. ~2× is the floor and still misses on a bad draw.

The small-request retry headroom already added is the right idea; this extends it to the general case. **Re-run `yield_curve` and `category_fit` after changing it** — more attempts per slot changes wall-clock, and Studio time is already a gate (§6.4b item 4).

**⚠️ 2026-08-06: THIS WAS THE FIRST TASK, ahead of D2 — DONE.** ✅ Implemented as **three changes** (commit `7017702`): (1) flat **2× fill budget** for every request (removed the `mult` tier-ladder); (2) **early exit** so 2× is a ceiling not a floor; (3) **no noise sort** (removed `sort_by(score)`; acceptance order via `take(target_per_cat)`). Order-preserving final dedup added (commit `839a705`). Core-25 through `engine::generate` PASSES post-D1: **25/25 unique in 181.9s**. after-D1 `category_fit` no regression (fire rate 96% (54/56), range 7.06, product 16/16; CSVs `category-fit-after-d1.csv`, `yield-curve-after-d1.csv`). Studio harness rebuilt in `yield_curve.rs` shape (`bc485f0`). **Measured Studio 200 post-D1 = 26-31 min** (NOT ~52 min — that was a 2× baseline error; see D2).

#### D2. Studio batch-time honesty *(§6.4b item 4)*

Recorded as 22.6 min best / 45.3 min worst against a softened "up to 200" claim. Those figures predate the 1× multiplier and are stale. Re-measure, and re-measure again after D1 since it directly increases attempts.

**⚠️ 2026-08-06 — first attempt (`5940dd2`) did not count. **Re-take DONE** post-D1, two reproducible runs.** coffee × youtube × 200 (400 attempts, flat 2×):
- **Run 1** (`studio-batch-run1.csv`): delivered **199/200 (99.5%)**, 200 dup : 1 QC, **30.7 min**.
- **Run 2** (`studio-batch-run2.csv`): delivered **200/200 (100%)**, 187 dup : 1 QC, **26.2 min**.
- **Timing note (rule #1):** the wall-clock figures above are printed to stdout only — the CSVs carry the per-attempt yield/outcome but not per-attempt latency. Timing is the part that matters for the cap decision; add a `wall_ms` (or `latency_ms`) column to a future run if precise reproducibility of the time value is required.

**Result:** post-D1 Studio fills to ~100% yield; the duplicate:QC split (~200:1) is now MEASURED — distinct-mass is the ceiling, quality is not the constraint. The binding gate is **time (~30 min for Studio 200)**. Owner decision pending: is that acceptable, or should the cap drop? Requirements met: per-attempt CSV with `outcome` column ✓, `engine::shares_opening` used ✓, yield headline ✓, run twice ✓, `qc_fail` undercount-by-design noted ✓.

**Requirements that were mandatory (now met by the re-take):**

- Rebuild the harness in **`yield_curve.rs`'s shape** — per-attempt CSV, `outcome` column (`accepted` / `duplicate` / `qc_fail`), committed as `studio-batch-run1.csv` / `run2.csv`.
- Call **`engine::shares_opening`** directly. Do not reimplement the dedup rule (`engine.rs:13-15`).
- Headline **yield (delivered ÷ requested)**, not exact-uniqueness — the latter is guaranteed by `engine.rs:158-161` and cannot fail.
- **After D1**, at the shipping multiplier. **Twice** (rule #7).
- Note in the write-up that `local_llm.rs:288-291` soft-returns rejected candidates, so `qc_fail` undercounts by design.

#### D3. Stripe licence flow end to end *(§6.4b item 7)*

Never tested with a real Stripe test purchase. Mocks lie — this needs the real checkout → webhook → key generation → email → activation path.

#### D4. CORS + rate limiting on the licence endpoint — ✅ **DONE 2026-08-06 (`dbee1f1`), code verified** *(§6.4b item 8 CLOSED)*

`cors.js:21-31` gates localhost behind `NETLIFY_DEV`; `licenses.js:65-77` throttles per IP, checked at `:111` before the Supabase lookup, mirroring `waitlist.js:8-27`. All 12 functions grepped — no `Access-Control-Allow-Origin: *` anywhere.

**Two things to record honestly rather than leave implied:**
- The throttle is **best-effort per-instance**. Netlify Functions are Lambda; the `Map` is per-instance, so concurrent requests land on fresh counters. It stops a sequential loop, not a parallel one. Fine for the real threat — a 64-bit key space is not enumerable — but do not write "rate limited" without the qualifier.
- `validateHits` is never evicted. Small, warm-instance-scoped; worth one line of cleanup.

**Note for next time: this shipped with the record untouched.** §6.2 #13, §6.4b item 8, §6.5, `AI-WORK-BRIEF.md` §6 and D4 in both handoffs all still said it was open. A closed gate nobody can see is not closed — the owner plans from these files.

---

### SECONDARY — worth doing, not blocking payments

#### W4. Cross-medium dedup *(confirmed still open)*

`generate.js:1141` returns `crossMediumData` before the dedup block at `:1203`, so cross-medium output is entirely un-deduped.

**The naive fix is wrong.** Cross-medium exists to express *the same idea* across media — a book title and its YouTube counterpart *should* rhyme. Apply the existing signature dedup **within each medium bucket independently**, never across buckets.

#### W5. Confirm `gemini-3.5-flash-lite` is stable and generally available

Now a hard production dependency for the primary generation path, selected on an unusual availability pattern (2.0-flash, 2.5-flash and 2.5-flash-lite all reported unavailable while 3.5-flash-lite worked). If that id shifts, the web app loses a provider. Record where the id came from.

#### D5. Phi-3.5 — properly evaluate, or formally park

The 2026-08-04 No-Go was invalid: it measured a stop-token bug (`token_eos` vs `is_eog_token`, fixed in `1dd414f`) plus an unoptimised build. Corrected figure is **~2.5× Qwen**, matching the original projection — *not* the "~12×" recorded.

**Phi is unevaluated, not rejected.** It produced clean output whenever it completed. `phi_smoke` now has a chat-template check and a debug/release assertion, so a fair run is possible. Either measure it properly with `category_fit` under `TF_MODEL_PATH`, or park it explicitly — but do not carry "12× slower / produces garbage" forward.

**⚠️ CORRECTED 2026-08-06 — "Studio 200 ≈ 28 min at ~2.5×" was wrong by ~2.3×.** That figure came from the phantom ~11 min Qwen baseline. It is also inconsistent with the same entry's own Phi rate: `CONTEXT.md:455` states **15-20 s/title**, and 200 × 15s = 50 min. Corrected: **Core 25 ≈ 6-8 min, Pro 50 ≈ 13-17 min, Studio 200 ≈ 50-67 min.** If 28 minutes was what made Phi look borderline-defensible for Studio, that judgement rested on a number that was never right. **Park Phi until the Studio cap is settled** — a model that triples the hardest gate cannot be evaluated sensibly while that gate is unresolved.

#### D6. Housekeeping

- ✅ **Removed dead `candle-core` / `candle-transformers` / `tokenizers` deps** (+ removed the `cuda` feature), verified `cargo check` + 33/33. Commit `cfa5d11`.
- ✅ **Migrated the updater endpoint** Netlify → GitHub Releases at the code level (`tauri.conf.json` endpoint + CSP `github.com`/`objects.githubusercontent.com`; workflow Netlify step → documented no-op). Commit `cfa5d11`. ⛔ Beta tag + live update validation deferred to the owner.

**Studio-scale finding (`5940dd2`) — ⚠️ SUPERSEDED 2026-08-06, see D2.** Recorded as *"coffee × youtube × 200 returned 124 unique in 26.2 min; the gap is a distinct-mass ceiling; worse than the ~11 min estimate."* On audit: the numbers are unreproducible (no CSV), "124/124 unique" is guaranteed by `engine.rs:158-161` and cannot fail, the stated cause was never instrumented, and the ~11 min baseline was itself wrong by 2× — the correct prior was **~23 min**, and the harness's own header says so. **What survives: 200 requested → 124 delivered is a real yield deficit, and "up to 200" is not defensible.** What does not survive: the framing, the cause, and the comparison. Re-take per D2, after D1. Caps NOT changed unattended — that part was right.

---

### ⛔ Do not start these

- **A ranker, a judge bake-off, or a labelled dataset.** Closed 2026-08-05. The user agrees with *himself* only 62.5% on decided pairs, so the judge at 55.3% is near the achievable ceiling. The target is unstable — a better judge cannot fix that.
- **Qwen2.5-3B** — `qwen-research` licence, not usable in a paid product.
- **Prompt-rule experiments on the 1.5B**, **colon-proportion caps**, **`LlamaSampler::grammar()`** — all measured and closed. See §2.
- **Pairing Phi with Qwen** — desktop's ceiling is distinct mass, but pairing doubles wall-clock, which is already a gate. If a second distribution is wanted on desktop, it replaces rather than supplements. See §5c.

---

### Standing rules that earned their place this week

1. **A measurement that only prints to stdout has not been taken.** Write the CSV (W1 is the cautionary tale).
2. **Run it twice.** Rule #7 caught two wrong conclusions on 2026-08-05 alone — a "thin keyword" finding that swung 14→23 on re-run, and the Phi verdict.
3. **Suspect the harness before the model.** Four "model limitations" in this project's history have been plumbing, most recently the stop-token bug.
4. **Never pass `&[]` examples to `generate_one_clean` in a measurement.** Production always supplies few-shot; an empty slice biases the result and produced the bogus Phi numbers.
5. **Never sort by `calculate_score` before measuring order-dependent things.** It is r = −0.04 against quality; sorting by it destroyed the previous yield attempt.

**Added 2026-08-06, from the Studio audit:**

6. **Before reporting a metric, ask what value would falsify the claim.** If no value can, it is not a measurement. "124/124 distinct" and "0 duplicates across N titles" are both guaranteed by `engine.rs:158-161` — they were cited for two weeks as evidence desktop has no distinctness problem, and they cannot show one. Yield can.
7. **Measure with the engine's own rule, by calling it.** `engine::shares_opening` is `pub` precisely so harnesses can use it (`engine.rs:13-15`). A reimplemented rule measures the reimplementation.
8. **Separate what the harness observed from what you concluded.** Say it explicitly in the commit message and the change-log entry. "Dedup consumed the budget" was imported from a different harness and presented as this run's result.
9. **When a result surprises you, check the baseline before you write "worse".** The ~11 min Studio estimate was never real, and the correct figure was in the header comment of the very file being written. A 14% miss was reported as a 2.4× surprise, and the same phantom number then corrupted the Phi tier maths.
10. **Shipping is half the task; the record is the other half.** `dbee1f1` closed a payment gate and left five documents saying it was open.
