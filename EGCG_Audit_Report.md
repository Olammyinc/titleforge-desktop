# EGCG Title Generator — Code Audit & Fix Report

**Subject:** `title_gen.rs` (Exemplar-Guided Coherence Generator)
**Reviewed against:** live output samples for keyword "laptop," genre "Science and Technology," categories Book/Article/Blog, Database engine

---

## 1. Summary

The output quality problem is not in the core idea of EGCG (affinity-scored, softmax-sampled slot filling over a curated corpus is a reasonable design). It's in four specific, isolated defects in the surrounding pipeline. Three are outright bugs; one is a structural gap in a single generation mode. None require redesigning the scoring model — they're fixable inside the existing architecture.

| # | Issue | Location | Severity | Effort |
|---|-------|----------|----------|--------|
| 1 | Genre and communication style are accepted but never used | `generate()`, line 303–312 | **Critical** | Medium |
| 2 | Only the first word of a title is ever capitalized | `assemble_title()`, line 606–638 | High | Low |
| 3 | No check prevents the same word filling two slots in one title | `fill_template_mode()` / `get_candidates()`, line 418–516 | Medium | Low |
| 4 | Phrase-stitch mode glues unrelated fragments with no seam check | `stitch_mode()`, line 644–674 | Medium | Medium |

---

## 2. Findings

### 2.1 Genre and style parameters are dead code (Critical)

```rust
pub fn generate(
    &self,
    keyword: &str,
    categories: &[String],
    _style: &str,
    _genre: &str,
    quantity: u32,
) -> Vec<TitleResult> {
```

The leading underscore is Rust's own convention for "this parameter is intentionally unused." Neither `_genre` nor `_style` is read anywhere downstream — not in `fill_template_mode`, not in `get_candidates`, not in `candidate_coherence`, not in `score_title`. The UI collects both (genre dropdown, communication-style buttons) and passes them in, and they're discarded on arrival.

It goes one level deeper: the database schema has a `genre` column on both `curated_titles` and `patterns` (visible in the test schema, lines 1234–1236), but `Generator::build()` never selects it:

```rust
conn.prepare("SELECT title, category FROM curated_titles")   // no genre
conn.prepare("SELECT category, template, slots FROM patterns") // no genre
```

So genre isn't just unused in generation — it's never loaded out of the database into memory at all. This is the direct cause of the off-topic slot fills observed in testing: "algorithm," "shield," "library," and "Technicolor" showing up for the keyword "laptop" under "Science and Technology," because candidate scoring draws from the entire 796-title corpus regardless of the genre the user selected. **This is the single highest-leverage fix** — it's the difference between "coherent with the corpus" and "coherent with what the user actually asked for."

**Fix:**
- Load `genre` alongside `title`/`category` in `build()`, and alongside `category`/`template`/`slots` for patterns.
- Filter or reweight `exemplar_vocab`, `unigram_cat`, and `templates` by genre before they're used as candidate sources.
- Actually consume `genre` (and `style`) inside `generate()` — at minimum as a hard filter on the templates and curated titles considered; ideally as an additional weighted term in `candidate_coherence`.
- If style (Provocative/Minimalist/Storytelling/etc.) isn't going to be implemented soon, remove it from the UI rather than leave a control that silently does nothing — a no-op setting is worse than no setting, because it teaches the user to distrust the other controls too.

### 2.2 Capitalization only applies to the title's first word (High)

```rust
let replacement = if result.starts_with(&placeholder) {
    // capitalize
} else {
    word.clone()   // stays exactly as stored — lowercase
};
```

Pool words are lowercased at load time (`word.to_lowercase()`), and this check only capitalizes a filled word when its placeholder happens to sit at position 0 of the *entire template string*. Every other slot lands lowercase next to template text that's already correctly capitalized, producing:

- "The **hidden** and **hidden** Sides of laptop"
- "Are You Making This **secret** Mistake"
- "Laptop Will **capitalize** the Way We **library**"

**Fix:** capitalize every filled word at insertion time (title-case rules, with a small exception list for articles/prepositions like "a," "of," "the," "in," "to" when not leading the title), not just whichever slot happens to open the template.

### 2.3 No duplicate-word constraint across slots (Medium)

`fill_template_mode` fills each slot independently. `filled_words` is passed into `candidate_coherence` only to compute an affinity *bonus* against prior context — it's never used as an exclusion list. Nothing stops the same word being selected for two different slots in one title, which produced "The hidden and **hidden** Sides of laptop."

**Fix:** before scoring, filter `candidates` (or the post-score `scored` list) to exclude any word already present in `filled_words` for the current title.

### 2.4 Phrase-stitch mode has no seam coherence check (Medium)

```rust
let title = format!("{} {} {}", intro_clean, keyword, closer_clean);
```

`intro_fragments` and `closer_fragments` are mined independently from the first/last 2–3 words of *unrelated* curated titles, then concatenated with the keyword in between. There's no check that the intro's final word and the closer's opening word form a grammatical join — no dangling-preposition filter, no POS boundary check. This is the only one of the three generation modes with zero affinity scoring at its critical seam, and it produced "How to engineer laptop **in at** breakneck speed."

**Fix, in order of effort:**
- Cheapest: at mining time (lines 251–280), drop intro fragments ending in a preposition/conjunction and closer fragments starting with one.
- Better: score the seam (intro-last-word ↔ keyword ↔ closer-first-word) using the existing affinity table before accepting a stitch, same as Mode A already does for template slots.

---

## 3. What to keep as-is

- The core affinity model (window-5 pairwise co-occurrence + softmax sampling) is sound in principle — it just needs genre-conditioning (2.1) to be scoring the *right* candidate pool.
- Mode C (keyword-embedded exemplar) has real seam scoring already (`variant_affinity`) — it's the most structurally solid of the three modes and doesn't need changes.
- The `MIN_COHERENCE` / `TOP_K` / `SOFTMAX_TEMP` constants are reasonable starting values; not worth tuning until 2.1–2.4 are fixed, since right now they're partly compensating for a mis-scoped candidate pool rather than reflecting true output quality.

## 4. Recommended fix order

1. **Genre wiring** (2.1) — highest impact, fixes relevance, requires schema query + filtering changes.
2. **Capitalization** (2.2) — highest visual impact for lowest effort, contained to `assemble_title`.
3. **Duplicate-word exclusion** (2.3) — small, contained, `fill_template_mode`/`get_candidates`.
4. **Stitch seam check** (2.4) — start with the cheap mining-time filter; revisit scoring if Mode B quality still lags after 1–3.

Fixing 1 and 2 alone should visibly change how the output reads even before touching the coherence math further — most of what currently looks like "the algorithm doesn't work" is actually "the algorithm is scoring against the wrong pool and displaying the result without capitalizing it."

---

## 5. Status update (2026-07-15, follow-up pass)

Issues 2.2, 2.3, and 2.4 (capitalization, duplicate-word exclusion, stitch seam stop-words) were already fixed in `title_gen.rs` by the time of this pass. Issue 2.1 (genre) was also fixed for `fill_template_mode` and `embed_mode`, but two gaps remained and have now been closed:

- **`style` was still fully unused inside EGCG.** The UI collects 9 tone/style options (Normal, Bold/Shout, Whisper, Blessing, Provocative, Minimalist, Storytelling, Question, Playful) and the `patterns`/`curated_titles` tables both carry a `tone` column, but `Generator::generate` took `_style: &str` and never read it — only the legacy template-engine fallback in `engine.rs` respected tone. Since EGCG produces the majority of output (80% Mode A + 10% Mode C), selecting a style had almost no visible effect. Fixed: `TemplateInfo` now carries `tone`, and `fill_template_mode`/`embed_mode` filter by genre+style with a fallback ladder (genre+style → genre-only → unfiltered), mirroring the existing genre-fix pattern.
- **Mode B (phrase stitching) ignored category entirely.** `intro_fragments`/`closer_fragments` were mined once globally across all curated titles with no category segmentation, so a stitched title could combine an intro mined from an "article" title with a closer mined from a "childname" title. Fixed: fragments are now mined and stored per-category (`HashMap<String, Vec<String>>`), and `stitch_mode` only combines fragments from the requested category.

Note: bash/cargo was unavailable in the session that made this pass, so these changes were verified by manual type-tracing rather than a live `cargo build` — worth running `cargo test` / `cargo build` before shipping.

---

## 6. Status update (2026-07-15, second follow-up — real output review)

Live output for keyword "shirt" (categories: article/blog/YouTube, style Normal, engine Auto) still showed clearly broken titles: lowercase keyword mid-title ("9 shirt Secrets Factory Need to Know", "The Girl shirt Follow My Passion"), semantically random nouns for a mundane keyword ("Why Algorithm Choose Shirt Over Map", "Unleash Shirt Like a Algorithm"), and a bare verb where a gerund was needed ("The Secret Journey of Navigate Shirt"). Traced these to four remaining gaps, all now fixed:

- **The legacy fallback engine (`engine.rs::fill_template`) never received any of the §2 fixes.** It only capitalizes the first character of the whole assembled title (not per-slot), does zero coherence scoring (pure uniform-random pool pick — explains the "Algorithm"/"Map"/"Factory" non-sequiturs), and has no duplicate-word exclusion. `Generator::generate` in `engine.rs` calls EGCG first but falls back to this function whenever EGCG can't fill enough slots, so a meaningful fraction of "Auto"/"Database" engine output was still coming from the pre-fix code path. Fixed: ported the same per-slot title-casing + duplicate-word-exclusion pattern from `title_gen.rs` into `fill_template`.
- **Mode B (`stitch_mode`) and Mode C (`embed_from_relevant`) both still only capitalized the first character of the final string**, not per-word. In Mode B the keyword is structurally always mid-string (`intro + keyword + closer`), so it was lowercase in *every* Mode B result, not just some. In Mode C the keyword is spliced into an existing curated title with no re-casing pass at all, keeping the original title's casing for every other word. Fixed: added a module-level `title_case_string()` helper and applied it to both modes' final output.
- **Gerund slots (`gerund`, `verb_ing`, etc.) all resolve to the `action_verbs` pool, which stores base-form verbs only** ("Master", "Navigate", "Build") — nothing in the codebase ever converted them to "-ing" form, so any template expecting a gerund got a grammatically bare verb instead. Fixed: added a heuristic `to_gerund()` (drops trailing "e" before appending "-ing", e.g. "Navigate" → "Navigating") applied at render time in both `title_gen.rs::assemble_title` and `engine.rs::fill_template`, gated on the slot's original name so scoring (which uses the base form for vocabulary lookups) is unaffected.
- **`MIN_COHERENCE` was a hard cutoff that discarded low-scoring candidates entirely**, falling back to a fully uniform-random pick from the whole pool when nothing cleared the threshold. This is exactly what happens for a keyword with little affinity data in the curated corpus (e.g. "shirt" against a literary/business-skewed title set) — everything scores near zero, so the "smart" scoring silently gave way to randomness. Fixed: `score_candidates` now always ranks whatever scores exist and softmax-samples from the top K, rather than abandoning scoring for a separate random branch. Degrades gracefully to near-random only in the genuine worst case (every candidate equally unrecognized); never worse than the old behavior, often much better.

As before, verified by manual type-tracing rather than `cargo build`/`cargo test` — bash was unavailable in this session too. This should be compiled and run against a real keyword like "shirt" before being considered done.
