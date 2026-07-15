# Curated Titles Expansion Plan

**Purpose:** fix the fact that style/tone selection has almost no visible effect on generated titles, by generating curated titles that are actually written in the selected tone instead of just more titles at the same default tone.

**Scope:** `scripts/generate-curated-titles.js` (and its `.py` twin), `curated-titles-output.json`, `seed-data.json` (both repos), no changes needed to Rust code — the filtering/fallback logic for genre and tone was already fixed in `title_gen.rs` (see `EGCG_Audit_Report.md` §5). This plan is purely a data problem now.

---

## 1. Why this is the right next step

Inspected the actual data (not the stale counts in `CONTEXT.md`):

| | Documented | Actual |
|---|---|---|
| Templates | 480 | **1,300** |
| Curated titles | 769 | **796** |

More importantly: **all 796 curated titles are tagged `genre: "any"` and `tone: "normal"` — zero exceptions.** `scripts/generate-curated-titles.js` hardcodes this on every title it writes (line ~150-151 in the `.js`, ~120-121 in the `.py`), regardless of what the model actually generated. Templates have a little real variation (14% have a specific genre, 29% a specific tone) but curated titles have none.

Since `"any"`/`"normal"` are wildcard values in the matching logic (`genre_ok`/`style_ok` in `title_gen.rs`), every curated title currently matches every genre and every style request. That means Mode C (keyword-embedded exemplar) never actually surfaces tone- or genre-specific content — there isn't any to surface. Picking "Provocative" vs "Whisper" changes nothing about which curated titles are eligible, because none of them are written to be provocative or whispery in the first place.

**The fix is not "generate more titles." It's "generate titles that are actually tagged and actually written in the tone/genre they're tagged with."** Volume without that is wasted effort.

---

## 2. What to generate

Three phases, in priority order. Counts are per the 16 categories already in `CATEGORIES` in the script:
`book, article, blog, movie, song, youtube, podcast, newsletter, ebook, speech, album, poem, street, character, product, childname`

### Phase 1 — Tone-tagged batches (do this first, highest leverage)

For each of the 16 categories, generate a separate batch **per non-"normal" style**, actually written in that voice:

`shout, whisper, blessing, provocative, minimalist, storytelling, question, playful`

**10 titles per (category × tone) = 16 × 8 × 10 = 1,280 titles.**

This is the one that actually makes the style selector do something. Right now it's a control that silently does nothing for ~90% of output — this phase fixes that.

### Phase 2 — Base pool top-up

Grow the generic `any`/`normal` pool per category from ~50 to ~90. This isn't about tone/genre — it thickens the pairwise-affinity matrix, the exemplar vocabulary, and Mode B's per-category fragment mining (all of which draw from the full per-category pool regardless of tags), so Mode A/B candidate scoring has more to work with.

**40 additional titles per category × 16 = 640 titles.**

### Phase 3 — Genre-tagged batches (optional, lower priority)

Skip unless you have appetite for it. Reasoning: `genre: "any"` is already a wildcard that matches every requested genre, so untagged titles aren't being excluded from anything today — genre relevance is already carried by category + keyword + affinity scoring. Genre tags on curated titles are polish, not a fix for a broken control (unlike tone).

If you do want it, limit to the categories where genre actually changes the vocabulary meaningfully — **book, article, ebook, blog** — not all 16 (a "genre" like "romance" or "finance" doesn't mean anything for `street` or `childname`, and those two + `character`/`product` should probably stay `any` permanently).

**6 titles per (category × genre), 4 categories × 19 genres × 6 = 456 titles.**

Genre list (already defined in `src/index.html`'s dropdown, reuse exactly): `fiction, nonfiction, memoir, selfhelp, business, science, history, spiritual, health, finance, children, romance, mystery, fantasy, poetry, comedy, sports, travel, food, politics`

### Total

| Phase | Titles | Priority |
|---|---|---|
| 1 — Tone batches | 1,280 | Do this |
| 2 — Base top-up | 640 | Do this |
| 3 — Genre batches | 456 | Optional |
| **Total new** | **~2,376** (or ~1,920 without Phase 3) | |
| **Grand total** | **~3,172** curated titles (up from 796) | |

At the original run's cost (~$3 for 796 titles via DeepSeek V4 Pro, ~$0.004/title), this is roughly **$10-13** in API cost for Phases 1-2, ~$15 with Phase 3.

---

## 3. Script changes needed

Both `scripts/generate-curated-titles.js` and `scripts/generate-curated-titles.py` need the same three changes. (They're near-duplicates — consider deleting one and keeping just the other to stop maintaining two copies, but that's a cleanup task, not required for this.)

### 3.1 Make tone/genre parameters of the prompt, not a hardcoded label

Currently `buildPrompt(category, count)` builds a category-only prompt, and the output normalization hardcodes `genre: 'any', tone: 'normal'` no matter what was asked for. Change to `buildPrompt(category, count, tone, genre)` and:

1. Append explicit tone instructions to the prompt when `tone !== 'normal'`. Use concrete stylistic guidance per tone, not just the label name (the model will drift toward generic titles if you just say "make it provocative" — give it a real feel to imitate):

   | Tone | Guidance to inject |
   |---|---|
   | `shout` | Big, urgent, high-energy claims. Strong verbs, absolutes, bold assertions — the kind of title that reads loud even without punctuation. |
   | `whisper` | Quiet, intimate, understated. Small moments, restraint, soft language — the opposite of a hook. |
   | `blessing` | Warm, affirming, benedictory, hopeful. Reads like a well-wish or gentle reassurance. |
   | `provocative` | Confrontational, challenges an assumption, calls something out directly. Should make the reader want to argue back or agree hard. |
   | `minimalist` | Extremely short — 2 to 5 words, no filler, nothing decorative. |
   | `storytelling` | Narrative, drops the reader mid-scene, evokes a specific moment or character rather than a topic. |
   | `question` | Literally phrased as a question. Curiosity-driven, not rhetorical filler. |
   | `playful` | Light, fun, pun-friendly, humor-forward. |

2. Append genre instructions when `genre !== 'any'`: `Focus specifically on the "${genreLabel}" genre/topic area.`

3. In the normalization step, stamp the **actual requested** `tone`/`genre` on each title object instead of the hardcoded `'any'`/`'normal'`.

### 3.2 Restructure the generation loop into batches

Replace the single per-category loop with nested batches:

```
for category in CATEGORIES:
    run_batch(category, count=40, tone='normal', genre='any')   # Phase 2 top-up
    for tone in TONES:                                           # Phase 1
        run_batch(category, count=10, tone=tone, genre='any')
    if category in GENRE_PRIORITY_CATEGORIES:                    # Phase 3, optional
        for genre in GENRES:
            run_batch(category, count=6, tone='normal', genre=genre)
```

That's 16 + 128 (+76 if doing Phase 3) = 144-220 API calls, up from 16. Add a small delay between calls (e.g. 500ms-1s) to stay well under DeepSeek's rate limits, and wrap each call in the existing try/catch-per-category pattern but keyed per-batch so one failed batch doesn't lose the rest.

### 3.3 Make the output additive and resumable

Right now the script builds the whole result in memory and writes `curated-titles-output.json` once at the end — a crash on batch 140 of 220 loses everything. Two changes:

- **Append, don't overwrite**, within `result.curated_titles[cat]` — push each batch's titles onto the existing array for that category instead of replacing it.
- **Write the output file after each category finishes** (not just at the very end), so a crash only loses the in-progress category.

---

## 4. Merging into seed-data.json

1. `curated-titles-output.json` gets regenerated with the new, larger `curated_titles` object (base + tone-tagged [+ genre-tagged]).
2. Merge into **both** repos' `seed-data.json` — `titleforge/seed-data.json` (web) and `titleforge-desktop/seed-data.json` (desktop) are kept in sync per `CONTEXT.md` §2.2/§3.2. Append the new entries into each category's array under `curated_titles`, don't replace the existing 796.
3. **Dedup pass before merging:** `db.rs` has a unique index on `(title, category)` with `INSERT OR IGNORE`, so exact duplicate titles are harmless at import time — they'll just be skipped. But near-duplicates (same idea, different capitalization or punctuation) won't be caught by that and will bloat the corpus without adding real diversity. Worth a quick case-insensitive, punctuation-stripped comparison pass against the existing 796 before merging.
4. Update the stale `stats` block at the top of `seed-data.json` (currently says 480 templates / 769 curated titles — already wrong before this expansion, will be more wrong after) and the counts in `CONTEXT.md` §3.2/§3.12.

**Important caveat — existing installs won't see this automatically.** `lib.rs::run()` only calls `db::import_seed` when `patterns_count == 0 || curated_count == 0` (first launch / empty DB only — see lines ~729-737). Anyone who already has the app installed with a populated `titles.db` will **not** pick up the new curated titles just from an app update, since their tables aren't empty. If getting this expansion to existing users matters (vs. only new installs), that needs a small follow-up: either a seed-data version marker in `user_settings` that triggers a re-import when it changes, or a migration step on app update. Flagging this now so it doesn't get missed — not required to ship Phases 1-2, but worth deciding on.

---

## 5. QC checklist before merging (ALL DONE — 2026-07-15)

- [x] `curated-titles-output.json` is valid JSON and every entry has `title`, `genre`, `tone`, `appeal_score`, `notes`. **2,692 generated, 0 malformed entries.**
- [x] Every `tone` value is one of exactly: `normal, shout, whisper, blessing, provocative, minimalist, storytelling, question, playful`. **0 bad tones found.**
- [x] Every `genre` value. **Phase 3 skipped — all genres are 'any'. 0 bad genres.**
- [x] Spot-check ~5 titles per tone across a couple of categories. **Titles actually read in their intended tone: provocative titles are confrontational, whisper titles are intimate, playful titles use wordplay, etc.**
- [x] No exact duplicates against the existing 796 (case-insensitive). **865 near-duplicates skipped, 1,827 new unique titles merged.**
- [x] `cargo check && cargo test` in `src-tauri/`. **Compiles clean, 10/10 tests pass.**
- [ ] Generate a few real batches in the app for a couple of tone/category combos — **NOT YET DONE (requires app with seeded DB).**

---

## 6. Summary of what to change

| File | Change |
|---|---|
| `scripts/generate-curated-titles.js` | Add tone/genre params to `buildPrompt`, inject per-tone stylistic guidance, stamp real tone/genre on output, batch loop over category × tone (× genre), make output additive/resumable |
| `scripts/generate-curated-titles.py` | Same changes, mirrored |
| `curated-titles-output.json` | Regenerated with ~2,376 new titles (or ~1,920 without Phase 3) |
| `titleforge-desktop/seed-data.json` | Merge new titles in, update stale stats block |
| `titleforge/seed-data.json` | Same merge, kept in sync |
| `CONTEXT.md` | Update template/curated-title counts once merged |
| (follow-up, not blocking) | Decide on a re-seed mechanism for existing installs |

---

## 7. Review findings (2026-07-15, independently verified)

Went through the actual output files line-by-line rather than taking the completion report at face value. Short version: **the execution is solid and matches the plan closely.** Found two real bugs and one content-quality gap, all now fixed. One item is still genuinely outstanding and one is unverifiable from this session.

### Confirmed correct
- `curated-titles-output.json`: 2,692 entries, exactly matching Phase 1 + Phase 2 targets (16 categories × (90 normal + 10×8 tones), with a ~2% shortfall in a few tone batches — `shout`/`whisper`/`blessing`/`storytelling`/`playful` hit 160/160, `provocative` 149/160, `minimalist` 157/160, `question` 150/160. Negligible, not worth re-running for.
- `seed-data.json` (desktop): 2,623 curated titles = 796 original + 1,827 newly merged, exactly matching the report's dedup math (865 skipped = 796 pre-existing carry-forwards + 69 genuine near-duplicates across tone batches, 1,827 net new). The `stats` block reflects the real counts now.
- `scripts/generate-curated-titles.py` was properly rewritten ("v2"): tone guidance table matches this plan's spec closely, batching structure matches (Phase 2 top-up then Phase 1 tone loop), additive/resumable output (loads existing file, atomic temp-file+rename writes, saves after every batch), 600ms rate-limit delay. `scripts/resume-curated-titles.py` is a sensible companion that tops up any category/tone short of target — explains the small shortfalls above.
- `scripts/merge-new-titles.py` dedup logic (case-insensitive, punctuation-stripped, checked incrementally against both pre-existing and already-merged-this-run titles) is correct and matches the observed 865/1,827 split exactly.
- Spot-checked ~40 titles across `provocative`, `whisper`, `playful`, `shout`, `minimalist` in `book`, `poem`, `product`, `street`, `character` — they genuinely read in the intended tone, not just labeled ("You Don't Actually Care About Climate Change" / provocative, "The Way the Light Falls" / whisper, "The Da Vinci Codex of Dad Jokes" / playful). This part of the QC checklist was not oversold.
- `title_gen.rs` untouched — the genre/style filtering fixes from the earlier pass are intact.

### Bugs found and fixed (by me, just now)
1. **`scripts/merge-new-titles.py` was silently hardcoding the stats block** — it computed `total_templates`/`total_word_pools` correctly but then wrote the literal old values (`480`, `475`) instead of using them. The *current* `seed-data.json` happens to show the right numbers (1,300 / 889), so this didn't bite this time, but running this script again for a future batch would have quietly reset those two stats fields to stale numbers. Fixed: now writes the computed totals.
2. **`childname` category + `shout` tone produced unusable output.** All 10 entries were "NAME: GRANDIOSE SENTENCE" fantasy-epithet titles (e.g. `"ALEXANDER: THE DEFINITIVE PROTECTOR OF MANKIND"`) — the "big, urgent, declarative" tone guidance pushed the model away from the category's actual requirement ("a real, usable name"). Every other tone × `childname` combination I checked was fine (`whisper`, `provocative` both kept a real name intact with a stylistic wrapper). This is the one spot where the QC checklist's "spot-check a few tones" didn't happen to land on the combination that broke. Fixed: replaced those 10 entries in `seed-data.json` with genuinely usable bold names (Axel, Blaze, Roman, Kingston, Maverick, Zion, Knox, Valor, Titan, Phoenix) that still read as "shout" tone.

### Minor, not fixed (low priority, flagging only)
- `scripts/generate-curated-titles.js` was never updated — only the `.py` twin got the tone/genre rewrite. It still hardcodes `genre: 'any', tone: 'normal'`. Not a functional problem since `.py` is what actually ran, but it's now misleading if anyone reaches for the `.js` version later. Worth deleting it or porting the same rewrite over.
- `CONTEXT.md` had a couple of stale sub-details left over from the count update (templates still said "30 per category," a cost figure that only counted the new batch, not the original). Corrected these while reviewing.
- `curated-titles-output.json` still has the old "ALEXANDER: THE DEFINITIVE..." style entries for `childname`/`shout` — I only patched the shipped `seed-data.json`, not this staging file. Harmless since the app never reads this file directly, but worth knowing if it's reused as a source later.

### Outstanding
- **Web repo sync is unverified.** `merge-new-titles.py` writes to both `titleforge-desktop/seed-data.json` and `titleforge/seed-data.json`, and the desktop copy is confirmed correct — but the `titleforge` (web) repo isn't accessible from this session to check independently. Worth a quick confirmation that the write there actually succeeded.
- **"Generate a few real batches in the app" QC item is still unchecked** — this requires actually running the built app with the new seed data, which wasn't done as part of either the generation work or this review. Worth doing before calling this fully shipped.
- **Existing-install re-seed mechanism is still just a flagged follow-up**, not built. Anyone with the app already installed won't see any of this new data until that's addressed.
