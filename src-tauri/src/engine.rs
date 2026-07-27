use rusqlite::Connection;
use serde_json;

use crate::local_llm::LocalLlm;
use crate::seo;
use crate::title_gen::Generator;
use crate::TitleResult;

/// Orchestrate title generation: local LLM first, then EGCG fallback,
/// then curated-title retrieval as last resort.
pub fn generate(
    conn: &Connection,
    generator: &Generator,
    local_llm: Option<&mut LocalLlm>,
    keyword: &str,
    categories: &[String],
    style: &str,
    genre: &str,
    quantity: u32,
) -> Result<Vec<TitleResult>, String> {
    let mut results = Vec::new();

    // ── Pass 1: Local LLM (if loaded) ──
    // Build SEO scorer once from curated titles for batch efficiency
    let curated_for_seo = fetch_curated_sample(conn, categories);
    let seo_scorer = seo::SeoScorer::from_curated(&curated_for_seo);

    if let Some(llm) = local_llm {
        let target_per_cat = (quantity as usize / categories.len().max(1)).max(1);
        for cat in categories {
            let examples = fetch_curated_examples(conn, cat, genre, style, 4);
            let mut attempts = 0usize;
            let mut got = 0usize;
            let max_attempts = target_per_cat * 3;
            let kw_lower = keyword.to_lowercase();

            while got < target_per_cat && attempts < max_attempts {
                attempts += 1;
                let prompt = build_llm_prompt(cat, keyword, style, &examples);
                let title = match llm.generate_one(&prompt) {
                    Some(t) => t,
                    None => continue,
                };

                // Hard QC gate — reject rather than accept a bad result to
                // hit the count. A title that doesn't relate to the keyword,
                // or that's just the model echoing back one of its own
                // few-shot examples verbatim, is worse than showing fewer
                // titles than requested.
                let lower = title.to_lowercase();
                let has_keyword = lower.contains(&kw_lower)
                    || kw_lower.split_whitespace().any(|w| lower.contains(w));
                let is_echo = examples.iter().any(|e| e.eq_ignore_ascii_case(&title));
                let long_enough = title.split_whitespace().count() >= 3;
                let already_seen = results.iter().any(|r: &TitleResult| r.title.eq_ignore_ascii_case(&title));

                if has_keyword && !is_echo && long_enough && !already_seen {
                    // Use the same scorer that produces the breakdown so the
                    // displayed score and its breakdown never disagree (using
                    // calculate_heuristic_score for the number and leaving
                    // breakdown: None here previously meant the LLM path was
                    // the only one in the UI without a score explanation).
                    let (score, breakdown) = calculate_score(&title, keyword, cat);
                    let platform = seo::platform_for_category(cat);
                    let (seo_score, seo_breakdown) = seo_scorer.score_seo(&title, keyword, cat, platform);
                    results.push(TitleResult {
                        title,
                        score,
                        categories: vec![cat.clone()],
                        breakdown: Some(breakdown),
                        source: Some("local-llm".to_string()),
                        seo_score: Some(seo_score),
                        seo_breakdown: Some(serde_json::to_value(&seo_breakdown).unwrap_or(serde_json::Value::Null)),
                    });
                    got += 1;
                }
            }
        }
    }

    // ── Pass 2: EGCG generation for remaining slots ──
    let remaining = (quantity as usize).saturating_sub(results.len());
    if remaining > 0 && keyword.len() > 2 {
        let egcg_results = generator.generate(keyword, categories, style, genre, remaining as u32);
        results.extend(egcg_results);
    }

    // ── Pass 3: Instant curated-title retrieval fallback ──
    let remaining = (quantity as usize).saturating_sub(results.len());
    if remaining > 0 {
        let curated_results = retrieve_curated_fallback(
            conn, keyword, categories, style, genre, remaining, &results,
        );
        results.extend(curated_results);
    }

    // ── SEO scoring sweep for EGCG + curated results ──
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

/// Build the few-shot prompt for one local-LLM generation call. Kept to
/// plain text, one title per call — a 360M model won't reliably follow
/// complex formatting instructions or batch requests.
fn build_llm_prompt(category: &str, keyword: &str, style: &str, examples: &[String]) -> String {
    let style_label = if style.is_empty() || style == "any" { "normal" } else { style };
    let mut prompt = String::new();
    if !examples.is_empty() {
        prompt.push_str(&format!("Examples of {} {} titles:\n", style_label, category));
        for ex in examples {
            prompt.push_str(&format!("- \"{}\"\n", ex));
        }
        prompt.push('\n');
    }
    prompt.push_str(&format!(
        "Write ONE new {} {} title about \"{}\". Reply with only the title, nothing else.",
        style_label, category, keyword
    ));
    prompt
}

/// Map 80+ specialized pool names to the 8 available SQLite word pools.
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