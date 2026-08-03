# Migration brief — evaluate Phi-3.5-mini as the offline engine

> **Status:** NOT STARTED. This is a spec for the next agent, written 2026-08-03.
> **Owner:** desktop app (`titleforge-desktop`). Do not touch the web app.
> **Decision gate:** this is an EVALUATION, not a foregone swap. It ships only if
> it beats Qwen on measured category fit *and* the speed cost is acceptable.

---

## 0. Why this exists

Three independent lines of work have now hit the same wall: **Qwen2.5-1.5B cannot
hold instructions that a 3B+ model handles easily.**

1. Multi-constraint quality rules — measured 75.2 and 77.6 vs an 81.0 baseline (2026-07-31)
2. The six-rule block, re-tested — same result, closed as "model capacity, not prompt craft"
3. **Category conventions (2026-08-03)** — `book`, `song` and `poem` still return
   headline-shaped output. Hard-enforcing the convention cost 18% of all output
   (fire rate 100% → 82%, song and poem halved). Soft-enforcing restores fire
   rate and book goes straight back to 75% colons.

See `CONTEXT.md` §5 (2026-08-03 category entry) for the full 3-run measurement.

**What a bigger model is expected to fix:** instruction-following on the mood-based
categories (song, poem, album, movie, book). It raises the average candidate.

**What it will NOT fix:** selection. There is still no working local ranker — the
judge failed calibration against the user (51.6% agreement in the usable band,
`CONTEXT.md` §6.2 item 6c). Do not let a model upgrade get re-sold as a quality
fix for ranking. They are different problems.

---

## 1. Why Phi-3.5-mini specifically

| | Qwen2.5-1.5B (current) | Phi-3.5-mini |
|---|---|---|
| Params | 1.5B | 3.8B |
| Q4_K_M size | 986 MB | ~2.2-2.4 GB |
| Licence | Apache 2.0 | **MIT** |
| Commercial use | yes | yes |

**Qwen2.5-3B is DISQUALIFIED and must not be reconsidered** — its licence is
`qwen-research`, not Apache 2.0. Alibaba carved out the 3B and 72B tiers
specifically. 1.5B and 7B are Apache 2.0. This is settled; see `CONTEXT.md` §5.

Phi-3.5-mini is the commercial-safe upgrade candidate because MIT permits
redistribution in a paid product.

**Verify the licence yourself before shipping.** Check the licence card on the
*GGUF quantiser's* repo, not just the base model — that is exactly how the Qwen
bundling gate was cleared (`bartowski/Qwen2.5-1.5B-Instruct-GGUF` → `apache-2.0`).
The base model being MIT does not automatically make a third-party quant MIT.

---

## 2. The thing that will kill this if you ignore it: SPEED

Phi-3.5-mini is **~2.5× the parameters**. Expect roughly **2-3× the generation
time per title**, CPU-bound, on the same hardware.

Current measured offline batch times (1× multiplier, `CONTEXT.md` §6.2 item 5):

| Tier | Titles | Qwen 1.5B | Phi-3.5 (projected 2.5×) |
|---|---|---|---|
| Core | 25 | ~1.4 min | **~3.5 min** |
| Pro | 50 | ~2.8 min | **~7 min** |
| Studio | 200 | ~11 min | **~28 min** |

**Studio at ~28 minutes is probably unshippable.** Before writing any migration
code, measure real tokens/sec on the target hardware and decide whether the tier
caps have to come down again. If they do, that is a product decision — escalate
it, do not quietly change the caps.

The user has explicitly accepted a longer **download** ("one time download thing,
anybody can wait for that"). That is NOT the same as accepting a longer
**generation** time, which is paid on every single batch. Do not conflate them.

---

## 3. Exact code changes

### 3.1 Model constants — `src-tauri/src/lib.rs` (~line 921)

```rust
const QWEN_URL: &str = "https://huggingface.co/bartowski/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf";
const QWEN_FILENAME: &str = "qwen2.5-1.5b-instruct-q4_k_m.gguf";
const QWEN_EXPECTED_SHA256: &str = "1adf0b11065d8ad2e8123ea110d1ec956dab4ab038eab665614adba04b6c3370";
const QWEN_EXPECTED_SIZE: u64 = 986_048_768;
```

Rename these to `MODEL_*` (they are no longer Qwen-specific) and repoint.

**Do NOT guess the SHA256 or the byte size.** Download the file once, then:

```powershell
(Get-FileHash <file> -Algorithm SHA256).Hash.ToLower()
(Get-Item <file>).Length
```

Paste the real values in. The download path verifies the hash before the model is
considered present (`qwen_present()`), so a wrong constant means the app downloads
986 MB+ and then refuses to use it, with no useful error.

### 3.2 Fallback chain — `src-tauri/src/lib.rs` `lazy_load_llm()` (~line 1091)

```rust
let model_names = vec![
    "qwen2.5-1.5b-instruct-q4_k_m.gguf",
    "SmolLM2-360M-Instruct-Q4_K_M.gguf",
    "SmolLM2-135M-Instruct-Q4_K_M.gguf",
];
```

Put Phi first, **keep Qwen in the chain**. Existing beta installs already have the
986 MB Qwen file on disk; do not strand them or force a silent 2.4 GB re-download.
`AI-WORK-BRIEF.md` §8.2 explicitly forbids removing the fallback chain.

### 3.3 Chat template — verify, do not assume

`generate_chat_raw()` uses `model.chat_template(None)` + `apply_chat_template()`,
which reads the template from GGUF metadata, so Phi's `<|system|>/<|user|>/
<|assistant|>` format *should* work with no code change.

**Verify it before trusting any quality number.** Run with `TF_LLM_DIAG=1` and
confirm the prompt is assembled correctly. If the template is missing from the
GGUF, `chat_template()` returns `Err` and generation silently produces nothing —
which is precisely the failure mode that wasted weeks on this project before
(`AI-WORK-BRIEF.md` hard rule #1: suspect the harness before the model).

Also re-check `build_banned_first()` — it scans the whole vocab for
instruction-echo prefixes. Different tokeniser, so log the banned-token count and
sanity-check it is not 0 or absurdly large.

### 3.4 Context window

`n_ctx` is 1024 (`local_llm.rs:94`). Phi-3.5 supports 128k but **do not raise it** —
prompts here are ~100-400 tokens and a larger KV cache costs memory and speed for
nothing. Leave it.

### 3.5 Disk / UI copy

- Download page system requirements currently say 500 MB / 1 GB free — must go up
- `desktop.html` and `desktop-download.html` name the engine as Qwen2.5 in sales
  copy. **Every sales-page claim must match measured reality** (§6.4b item 5).
  Coordinate with whoever owns the web app; do not edit it unilaterally.
- Installer stays ~5.9 MB either way — the model is a first-launch download.

---

## 4. How to decide whether it wins

**Use the existing harness. Do not invent a new metric.**

```bash
cargo test --release --test category_fit -- --nocapture
```

`tests/category_fit.rs` reports per-category word-band conformance, colon/digit
rates, and fire rate. It makes **no judge API call** by design — category fit is
objective, and the DeepSeek judge failed calibration against the user
(`CONTEXT.md` §6.2 item 6c). Do not "improve" it by adding judge scoring.

Baselines to beat (Qwen 1.5B, 3 runs, committed CSVs `category-fit-run*.csv`):

| Metric | Qwen baseline |
|---|---|
| fire rate | 93-100% |
| cross-category word range | 6.4 - 7.3 |
| inside word band | 89-92% |
| colons — book | 75% |
| colons — song | 33-50% |
| `product` correctness | 24/24 |

**GO if:** song/poem/book stop being headline-shaped (read them yourself — this is
the whole point), `product` stays at ~100%, fire rate stays ≥90%, AND the measured
batch time is acceptable at Core and Pro.

**NO-GO if:** quality gain is marginal but generation time doubles. A 2× slower
engine for a few percent is a bad trade on a product where Studio is already
11 minutes.

**Rules that still apply:**
- Run it **twice** — one run is an anecdote (brief rule #7). Sampled decoders vary.
- **One variable per run** (rule #4). Do not swap the model and change the prompt
  in the same measurement.
- n is 2-4 per category in the current harness. **Raise `PER_CASE` to at least 8
  before drawing conclusions about anything except `product`.**
- Read 10 outputs with your own eyes. If it looks bad to you, it is bad, whatever
  the metric says.

---

## 5. Rollback

Every change above is a constant or a list entry. Rollback is: repoint the three
`MODEL_*` constants, reorder `model_names`, revert the UI copy. Keep it that way —
do not thread model-specific branching through `local_llm.rs` or `engine.rs`.

If Phi wins, the Qwen file stays a valid fallback for existing installs, so there
is no migration event for users already running the beta.

---

## 6. Explicitly out of scope

- **The ranker.** Dead until a trustworthy label source exists. A bigger model
  does not fix selection.
- **Prompt-rule experiments.** Tested twice on the 1.5B, measured worse both
  times. If Phi ships, the constraint ceiling should be **re-measured from
  scratch** — a rule measured on one engine is only a hypothesis about another
  (brief rule #3 meta-lesson) — but that is a separate task, after the swap.
- **Qwen2.5-3B.** Licence-disqualified. Do not revisit.
