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

## 7. Status update (2026-07-17, third follow-up — "television" test, new bug class found)

Live output for keyword "television" (categories: article/blog/YouTube, genre Any, style Normal, engine Auto) still showed roughly half of a 10-result sample broken, but for entirely new reasons — none of the §6 bug classes (lowercase mid-title keyword, random unrelated nouns, bare-verb-instead-of-gerund) recurred, which confirms those fixes held. The new failures:

- "Breakthrough vs Breakthrough: The Real Difference Between Television Approaches" (score 96) — same word twice across what should be two independent slots.
- "Why You Should Stop Engineer Television Right Now" (score 56) — bare verb after "Stop" where a gerund is grammatically required ("Stop Engineering", not "Stop Engineer").
- "10 Television Secrets Roadmap Need to Know" / "6 Television Secrets Playbook Need to Know" (scores 72, 71) — a person/audience word was needed ("...Secrets You Need to Know" / "...Secrets Marketers Need to Know") but a random object noun landed there instead.
- "Unlock That Changed My Life with This Simple Television Hack" (score 85) — a sentence fragment used as if it were a standalone noun phrase.
- "Television Is the Brutal Library You'll Ever Master" (score 78) — grammatically valid template, semantically unrelated adjective/noun pairing ("brutal" + "library") that scored high anyway.

Root causes, in order of how confidently they're diagnosed:

1. **`assemble_title()` (and the legacy `engine.rs::fill_template()`) used `String::replace()`, which replaces every occurrence of a placeholder in one call — not just the one belonging to the current slot.** Templates that intentionally reuse a slot name for two independent fills (e.g. `"{adjective} vs {adjective}: The Real Difference Between {topic} Approaches"`, confirmed at `seed-data.json:2344` — there are two separate `"adjective"` slot entries in the JSON, each meant to fill independently) had their first fill silently overwrite *both* placeholders, because by the time the second slot's turn came, `.replace()` found nothing left to replace. The per-slot duplicate-word exclusion logic was working correctly the whole time (each slot really did get a different word) — the bug was purely in how the words got written back into the template string. At least 10 templates in the "X vs X" family alone are affected (grep for `"vs {"` in `seed-data.json`), likely more with reused names elsewhere. **Fixed**: both `assemble_title()` and `fill_template()` now use `result.replacen(&placeholder, &replacement, 1)`, consuming exactly one occurrence per slot in template order — correct whether or not slot names repeat.

2. **`resolve_pool_name()` collapses semantically distinct slot types into three oversized buckets** (`action_verbs`, `power_adjectives`, `nouns`), and this destroys correctness for slot types that need a specific *kind* of noun rather than any noun. Confirmed concretely: `"audience"` is one of ~30 aliases mapped to the plain `"nouns"` pool (`title_gen.rs:1322`) — but no dedicated `"audience"` word pool exists in `seed-data.json` at all (confirmed: zero `"audience": [...]` array in `word_pools`), and templates like `"{number} {topic} Secrets {audience} Need to Know"` (`seed-data.json:3707`) need `audience` to resolve to a person/group word ("You", "Marketers", "Beginners") to read correctly — instead it silently draws from the generic object-noun pool, producing "Secrets Roadmap Need to Know." **35 templates reference the `audience` slot** (`grep -c '"name": "audience"' seed-data.json`), all affected. **Not fixed this pass** — needs either a real `audience` word pool authored and wired into `resolve_pool_name`, or those 35 templates retargeted to an existing suitable pool. Scope is comparable to the curated-titles expansion project, not a quick patch.

3. **The `results` word pool (117 template slots reference it) is authored as trailing relative-clause fragments** — every entry starts with "That..." and is meant to modify a preceding plural noun ("5 Strategies **That Will Change Your Life**") — **but at least one template uses it as a standalone direct object**: `"Unlock {result} with This Simple {topic} Hack"` (`seed-data.json:9011`) renders as "Unlock That Changed My Life with This Simple Television Hack," which doesn't parse, because there's no plural noun for "That Changed My Life" to modify. Only this one instance was confirmed by name; whether other `results`-pool templates have the same structural mismatch wasn't checked (117 references is too many to eyeball by hand this pass). **Not fixed this pass** — needs either a scripted audit of all 117 `results`-pool template contexts, or reworking the template to use a standalone-noun pool instead.

4. **Semantic/topical coherence between adjective and noun slots isn't actually verified before scoring.** "Television Is the Brutal Library You'll Ever Master" is grammatically well-formed but "brutal library" has no real relationship to itself or to "television" — the pairwise word-affinity scoring exists to rank *candidates against each other*, but the "appeal" score shown in the UI (`calculate_score`/`score_title`) is a separate, shallow heuristic (keyword presence, numbers, punctuation, word count, power/emotional word lists) that has no concept of whether an adjective and noun actually belong together. This is a design limitation rather than a discrete bug — genuinely fixing it means either real coherence modeling (hard to do well with hand-tuned pairwise affinities) or accepting that template-combinatorial generation will keep producing this class of error at some baseline rate no matter how many individual templates get patched.

**What this round changes about the EGCG-vs-local-LLM decision**: this is the third consecutive round where a fresh keyword test surfaced a genuinely new bug class after the previous round's fixes held. That's the pattern you'd expect from a system built on ~1,300 hand-authored templates × ~900 hand-authored word-pool entries, where correctness depends on every template/pool-type pairing being grammatically and semantically compatible — a combinatorial surface too large to fully audit by hand, and too easy for one mismatched slot alias or one dual-purpose pool to slip through. Items 2 and 3 above are exactly this kind of latent mismatch, invisible until a keyword happens to land on the wrong combination. This doesn't mean EGCG is unfixable — item 1 (the replace-all bug) was a real, clean win that should measurably improve output the next time this is tested — but it's further evidence for the local-LLM path already in progress (see `LOCAL_LLM_ENGINE_PLAN.md`): a model that understands "audience slots need person-words" and "a sentence fragment isn't a noun phrase" without those rules being hand-encoded per template.

As with every prior pass, none of this is compiled — bash access is still blocked by the same Cowork/Windows UNC-path issue. The replace-all fix and the "Stop {verb_ing}" template fix should be included in whatever gets compiled next.
