use rusqlite::Connection;
use serde_json;

use crate::local_llm::LocalLlm;
use crate::seo;
use crate::title_gen::Generator;
use crate::TitleResult;

/// True if two titles open with the same `n` words (case-insensitive,
/// punctuation-stripped). Catches the near-duplicate family that exact-match
/// dedup misses — measured 2026-08-03, a 4-title book batch came back as four
/// variations on "Remote Revolution".
pub(crate) fn shares_opening(a: &str, b: &str, n: usize) -> bool {
    // Function-word openings ("how to", "the best", "why you") are common to
    // many perfectly distinct titles. Flagging those would reject legitimate
    // variety and cost fire rate, so a shared opening only counts when it
    // carries at least one content word.
    const FUNCTION: &[&str] = &[
        "the", "a", "an", "of", "in", "on", "at", "to", "for", "and", "or",
        "my", "i", "you", "your", "is", "it", "with", "how", "what", "why",
        "this", "that", "from", "best", "top",
    ];
    let head = |s: &str| -> Vec<String> {
        s.split_whitespace()
            .take(n)
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
            .filter(|w| !w.is_empty())
            .collect()
    };
    let (ha, hb) = (head(a), head(b));
    if ha.len() != n || ha != hb {
        return false;
    }
    ha.iter().any(|w| !FUNCTION.contains(&w.as_str()))
}

/// Orchestrate title generation: local LLM first, then curated-title
/// retrieval as the quality fallback. EGCG generation was retired from the
/// pipeline (2026-07-31): 20-24% usable on the corrected metric, mean ~37 —
/// it produced output 98% of the time and garbage 80% of the time. Qwen now
/// fires 50/50, so the only reason to keep EGCG (batch fill) is gone.
#[allow(clippy::too_many_arguments)]
pub fn generate(
    conn: &Connection,
    generator: &Generator,
    local_llm: Option<&mut LocalLlm>,
    keyword: &str,
    categories: &[String],
    style: &str,
    genre: &str,
    quantity: u32,
    tier: &str,
    finetune: &crate::prompt_spec::FineTune,
) -> Result<Vec<TitleResult>, String> {
    let mut results = Vec::new();

    // ── Pass 1: Local LLM (if loaded) ──
    // Build SEO scorer once from curated titles for batch efficiency
    let curated_for_seo = fetch_curated_sample(conn, categories);
    let seo_scorer = seo::SeoScorer::from_curated(&curated_for_seo);

    if let Some(llm) = local_llm {
        let target_per_cat = (quantity as usize / categories.len().max(1)).max(1);
        // Best-of-N multiplier — FORCED TO 1x (brief §4 Task 1, 2026-08-03).
        // Measured on a real 50-title batch: sorting the pool by calculate_score
        // correlates r = -0.04 with judge quality — it ranks by noise while
        // paying 4x generation time. A 1x multiplier is NOT a downgrade: same
        // quality, ~4x less wall clock (Core 25: ~5.7 min -> ~1.4 min).
        //
        // DO NOT DELETE this pool/dedupe/sort scaffolding. The moment a real
        // ranker exists (brief §4 Task 4 — holdout r >= 0.35 required), restore
        // the multiplier as a ONE-LINE change:
        //   let mult: usize = match tier { "studio" => 2, "pro" => 3, _ => 4 };
        let mult: usize = 1;
        // Small per-category requests need retry headroom: failed LLM attempts
        // and near-duplicate rejection otherwise make a 10-title request
        // return 8 even though the tier cap is not the constraint. Use the
        // per-category size to avoid a quantity=25/26 cliff across categories.
        let iteration_budget = if target_per_cat <= 12 {
            target_per_cat * 2
        } else {
            target_per_cat * mult
        };
        for cat in categories {
            // RAG: retrieve similar curated titles for few-shot prompting.
            // When keyword retrieval is empty (laptop, bitcoin, tennis, jazz,
            // cooking among ~13/50 benchmark keywords), fall back to the
            // highest-appeal curated titles for this category so the model
            // ALWAYS has strong exemplars. (brief §4 Task 3.)
            let mut examples = generator.retrieve_similar(keyword, cat, 4);
            if examples.is_empty() {
                examples = fetch_top_appeal_fewshot(conn, cat, 4);
            }
            // Candidate pool for THIS category — run the full budget, dedupe,
            // rank by score, keep the top target_per_cat.
            let mut pool: Vec<TitleResult> = Vec::new();
            // Rotate ONE structural constraint per call to break formula
            // repetition (7/25 "From X to Y"). Qwen 1.5B handles a single
            // constraint; the full 6-rule block measured worse. (brief §4 Task 4)
            // Set TF_NO_CONSTRAINTS=1 to A/B this off — Task 4 is the prime
            // suspect for the widened bottom tail measured 2026-08-02
            // (drift <50 went 3 -> 7, usable 94% -> 84%). A 1.5B juggling the
            // relevance guard + cliche filter + a structural constraint inside
            // a 3-attempt budget can exhaust its retries and return whatever
            // survived rather than the best candidate.
            let constraints: &[&str] = if std::env::var("TF_NO_CONSTRAINTS").is_ok() {
                &[""]
            } else {
                &[
                    "",
                    "Make this one a question.",
                    "Open this one with a specific number.",
                    "Frame this one as a personal story or first-person experience.",
                    "Build this one on a contrast or a reversal.",
                    "Make this one short — three words or fewer.",
                ]
            };
            let mut ci = 0usize;

            let spec = crate::prompt_spec::category_spec(cat);

            // A COLON-PROPORTION CAP WAS TRIED TWICE AND DOES NOT WORK. Do not
            // re-attempt it without reading this. Measured 2026-08-03:
            //   run 4, cap via instruction ("Do not use a colon in this one"):
            //     blog colons went UP 50% -> 75% and poem word-band conformance
            //     collapsed 67% -> 25%. A 1.5B does not follow negative
            //     instructions, and it displaced the rotated diversity
            //     constraint, which was doing real work.
            //   run 5, cap via soft rejection: book stayed at 75% colons. Qwen
            //     emits a colon on nearly every book attempt, so all 3 attempts
            //     are rejected and the soft fallback returns a colon title
            //     anyway. Headline metrics regressed (range 7.00 -> 5.88).
            //   run 2, cap via HARD rejection: works (75% -> 0%) but costs 18%
            //     of all output.
            // Conclusion: at 1.5B this is a model-capacity limit. Both forms are
            // legitimate for books anyway ("The Name of the Wind" and "Sapiens:
            // A Brief History"), so the proportion is left alone.

            for _ in 0..iteration_budget {
                let title = match llm.generate_one_clean(
                    keyword, cat, style, genre, &examples,
                    Some(constraints[ci % constraints.len()]), finetune,
                ) {
                    Some(t) => t,
                    None => { ci += 1; continue; }
                };
                ci += 1;
                // Dedup was exact-match only, which let near-duplicates through:
                // one measured book batch returned "Remote Revolution: How Work
                // Transformed", "Remote Revolution: How Work Changes When You
                // Do", "Remote Revolution: My Journey Unplugged" and "Remote
                // Revolution". Also reject a shared opening — the web prompt
                // already states "no two titles may share their opening three
                // words"; this enforces it instead of asking.
                let already_seen = pool.iter().any(|r: &TitleResult| {
                    r.title.eq_ignore_ascii_case(&title) || shares_opening(&r.title, &title, 2)
                });
                if already_seen { continue; }

                let (score, breakdown) = calculate_score(&title, keyword, cat);
                // SEO scoring is length/keyword-based and calibrated for
                // headlines on Google/YouTube/Amazon. Scoring a product NAME
                // against a 60-100 char Amazon sweet spot reports ~15 for a
                // correct answer, which reads to the user as "this is bad".
                // Names carry no SEO score rather than a misleading one; the
                // field is already Option and the UI omits it when absent.
                let (seo_score, seo_breakdown) = if spec.is_name {
                    (None, None)
                } else {
                    let platform = seo::platform_for_category(cat);
                    let (s, b) = seo_scorer.score_seo(&title, keyword, cat, platform);
                    (Some(s), Some(serde_json::to_value(&b).unwrap_or(serde_json::Value::Null)))
                };
                pool.push(TitleResult {
                    title,
                    score,
                    categories: vec![cat.clone()],
                    breakdown: Some(breakdown),
                    source: Some("local-llm".to_string()),
                    seo_score,
                    seo_breakdown,
                });
            }

            // Best-of-N: rank the pool by score, keep the top target_per_cat.
            pool.sort_by(|a, b| b.score.cmp(&a.score));
            pool.truncate(target_per_cat);
            results.extend(pool);
        }
    }

    // ── Pass 2: Instant curated-title retrieval fallback ──
    // The curated corpus is 2,623 TITLES. Using it to top up a product-name or
    // child-name request returns blog headlines for a name slot — the same
    // off-topic padding that `curated_is_relevant` was added to stop, one level
    // up. Name categories are excluded; if that leaves the batch short, we
    // return fewer results rather than wrong ones (established behaviour, see
    // the 2026-08-02 "off-topic curated titles" fix).
    let fallback_cats: Vec<String> = categories
        .iter()
        .filter(|c| !crate::prompt_spec::category_spec(c).is_name)
        .cloned()
        .collect();
    let remaining = (quantity as usize).saturating_sub(results.len());
    if remaining > 0 && !fallback_cats.is_empty() {
        let curated_results = retrieve_curated_fallback(
            conn, keyword, &fallback_cats, style, genre, remaining, &results,
        );
        results.extend(curated_results);
    }

    // ── SEO scoring sweep for curated results ──
    // LLM-pass titles were scored inline above; this fills remaining.
    for r in results.iter_mut() {
        if r.seo_score.is_some() { continue; }
        let cat = r.categories.first().map(|s| s.as_str()).unwrap_or("");
        let platform = seo::platform_for_category(cat);
        let (ss, sb) = seo_scorer.score_seo(&r.title, keyword, cat, platform);
        r.seo_score = Some(ss);
        r.seo_breakdown = Some(serde_json::to_value(&sb).unwrap_or(serde_json::Value::Null));
    }

    // Finalize: deduplicate, sort by score, truncate
    results.sort_by(|a, b| b.score.cmp(&a.score));
    results.dedup_by(|a, b| a.title.eq_ignore_ascii_case(&b.title));
    results.truncate(quantity as usize);

    Ok(results)
}

/// Few-shot exemplars for the local LLM when keyword retrieval is empty.
/// Returns the highest-`appeal_score` curated titles in the category — strong,
/// on-voice titles the model can imitate even without a keyword-specific match.
fn fetch_top_appeal_fewshot(conn: &Connection, category: &str, limit: i64) -> Vec<String> {
    match conn.prepare(
        "SELECT title FROM curated_titles WHERE category = ?1 ORDER BY appeal_score DESC LIMIT ?2"
    ) {
        Ok(mut stmt) => stmt
            .query_map(rusqlite::params![category, limit], |row| row.get::<_, String>(0))
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Pull a handful of matching curated titles to ground the local LLM in
/// TitleForge's actual voice for this category/genre/style. Same relax
/// ladder as EGCG's Mode C: exact genre+tone match first, then fall back to
/// any curated title in the category so there are always some examples to
/// work with even for sparsely-tagged combinations.
fn fetch_curated_examples(conn: &Connection, category: &str, genre: &str, style: &str, limit: i64) -> Vec<String> {
    let exact: Vec<String> = match conn.prepare(
        "SELECT title FROM curated_titles WHERE category = ?1 AND (genre = ?2 OR genre = 'any') AND (tone = ?3 OR tone = 'normal') ORDER BY RANDOM() LIMIT ?4"
    ) {
        Ok(mut stmt) => stmt
            .query_map(rusqlite::params![category, genre, style, limit], |row| row.get::<_, String>(0))
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    if !exact.is_empty() {
        return exact;
    }

    match conn.prepare("SELECT title FROM curated_titles WHERE category = ?1 ORDER BY RANDOM() LIMIT ?2") {
        Ok(mut stmt) => stmt
            .query_map(rusqlite::params![category, limit], |row| row.get::<_, String>(0))
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Instant curated-title fallback for remaining slots after the LLM pass.
/// Pulls matching curated titles via fetch_curated_examples (same category/
/// genre/tone relax ladder) and wraps them as scored TitleResults.
fn retrieve_curated_fallback(
    conn: &Connection,
    keyword: &str,
    categories: &[String],
    style: &str,
    genre: &str,
    limit: usize,
    existing: &[TitleResult],
) -> Vec<TitleResult> {
    if categories.is_empty() || limit == 0 {
        return Vec::new();
    }
    let per_cat = (limit / categories.len()).max(1) as i64;
    let mut out: Vec<TitleResult> = Vec::new();

    for cat in categories {
        if out.len() >= limit { break; }
        let titles = fetch_curated_examples(conn, cat, genre, style, per_cat);
        for title in titles {
            if existing.iter().any(|r| r.title.eq_ignore_ascii_case(&title)) { continue; }
            if out.iter().any(|r| r.title.eq_ignore_ascii_case(&title)) { continue; }
            // ONLY use curated titles the keyword is actually about. A random
            // category title that ignores the user's keyword is worse than no
            // fill — "clearly about the topic" is the Prime Directive, and a
            // tangentially-related or unrelated title reads as broken output.
            if !curated_is_relevant(&title, keyword) { continue; }
            let (score, breakdown) = calculate_score(&title, keyword, cat);
            out.push(TitleResult {
                title,
                score,
                categories: vec![cat.clone()],
                breakdown: Some(breakdown),
                source: Some("curated".to_string()),
                seo_score: None,
                seo_breakdown: None,
            });
            if out.len() >= limit { break; }
        }
    }

    out.sort_by(|a, b| b.score.cmp(&a.score));
    out.truncate(limit);
    out
}

/// True if a curated title is plausibly about the keyword — a soft token-stem
/// overlap check. We do NOT demand the literal keyword (that's inversely
/// correlated with quality), but we do require SOME lexical connection so
/// "coffee" never gets a gardening title.
///
/// Also used by local_llm.rs as the post-generation drift guard: accept any
/// >=4-char keyword word anywhere in the title. No literal full-phrase match,
/// so creative titles survive; genuine off-topic drift does not.
pub(crate) fn curated_is_relevant(title: &str, keyword: &str) -> bool {
    let t = title.to_lowercase();
    let kw = keyword.to_lowercase();
    if t.contains(&kw) { return true; }
    // Any significant word of the keyword (≥4 chars) appearing in the title.
    kw.split_whitespace()
        .filter(|w| w.len() >= 4)
        .any(|w| t.contains(w))
}

fn calculate_score(title: &str, keyword: &str, _category: &str) -> (u32, serde_json::Value) {
    let lower = title.to_lowercase();
    let kw = keyword.to_lowercase();
    let mut score = 45u32;
    let word_count = title.split_whitespace().count();

    let mut has_keyword = false;
    let mut has_number = false;
    let mut has_curiosity = false;
    let mut has_emotional = false;
    let mut _has_power = false;

    if lower.contains(&kw) { score += 15; has_keyword = true; }
    else if kw.split_whitespace().any(|w| lower.contains(w)) { score += 8; has_keyword = true; }

    if title.chars().any(|c| c.is_ascii_digit()) { score += 10; has_number = true; }
    if title.contains('?') || title.contains(':') || title.contains("...") { score += 10; has_curiosity = true; }

    let emotional = ["secret","hidden","truth","never","wrong","best","worst",
        "ultimate","essential","proven","easy","fast","simple","every","anyone",
        "nobody","everyone","always","forever","impossible","possible"];
    if emotional.iter().any(|w| lower.contains(w)) { score += 10; has_emotional = true; }

    let power = ["why","how","what","when","stop","start","transform","unlock",
        "master","hack","build","create","destroy","save","kill","love","hate"];
    if power.iter().any(|w| lower.contains(w)) { score += 5; _has_power = true; }

    if word_count >= 4 && word_count <= 14 { score += 10; }
    else if word_count >= 2 && word_count <= 18 { score += 5; }
    else { score = score.saturating_sub(8); }

    // Penalize repeated words (common in template filler)
    let words: Vec<&str> = lower.split_whitespace().collect();
    let unique_count = words.iter().collect::<std::collections::HashSet<&&str>>().len();
    if unique_count < words.len() && words.len() > 3 {
        score = score.saturating_sub(5);
    }

    score = score.min(100);

    let curiosity_gap = if has_curiosity { "High" } else if has_number { "Medium" } else { "Low" };
    let emotional_trigger = if has_emotional {
        if lower.contains("secret") || lower.contains("hidden") { "curiosity" }
        else if lower.contains("truth") || lower.contains("never") || lower.contains("wrong") { "surprise" }
        else if lower.contains("best") || lower.contains("ultimate") || lower.contains("essential") { "aspiration" }
        else if lower.contains("easy") || lower.contains("fast") || lower.contains("simple") { "aspiration" }
        else if lower.contains("every") || lower.contains("anyone") || lower.contains("nobody") { "curiosity" }
        else if lower.contains("forever") || lower.contains("impossible") { "surprise" }
        else { "curiosity" }
    } else if has_number { "curiosity" } else { "neutral" };
    let specificity = if has_keyword { "Concrete" } else if has_number { "Concrete" } else { "Abstract" };
    let length_analysis = if word_count <= 3 { format!("Short ({} words)", word_count) }
        else if word_count <= 8 { format!("Optimal ({} words)", word_count) }
        else { format!("Long ({} words)", word_count) };

    let mut power_words: Vec<&str> = Vec::new();
    for w in &power {
        if lower.contains(w) { power_words.push(w); }
    }
    for w in &emotional {
        if lower.contains(w) && !power_words.contains(w) { power_words.push(w); }
    }

    let breakdown = serde_json::json!({
        "curiosityGap": curiosity_gap,
        "emotionalTrigger": emotional_trigger,
        "powerWords": power_words,
        "lengthAnalysis": length_analysis,
        "specificity": specificity
    });

    (score, breakdown)
}

/// Pull a bounded random sample of curated titles for the given categories.
/// Used by the SEO scorer to build its n-gram reference corpus for uniqueness.
fn fetch_curated_sample(conn: &Connection, categories: &[String]) -> Vec<String> {
    if categories.is_empty() { return Vec::new(); }
    let placeholders = categories.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("SELECT title FROM curated_titles WHERE category IN ({}) ORDER BY RANDOM() LIMIT 800", placeholders);
    let mut stmt = match conn.prepare(&sql) { Ok(s) => s, Err(_) => return Vec::new() };
    let params: Vec<&dyn rusqlite::types::ToSql> = categories.iter().map(|c| c as &dyn rusqlite::types::ToSql).collect();
    stmt.query_map(params.as_slice(), |row| row.get::<_, String>(0))
        .ok().map(|rows| rows.filter_map(|r| r.ok()).collect()).unwrap_or_default()
}
