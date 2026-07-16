use rand::seq::SliceRandom;
use rand::Rng;
use rusqlite::Connection;
use serde_json;

use crate::title_gen::Generator;
use crate::TitleResult;

/// Orchestrate title generation: try EGCG first, fall back to template engine.
pub fn generate(
    conn: &Connection,
    generator: &Generator,
    keyword: &str,
    categories: &[String],
    style: &str,
    genre: &str,
    quantity: u32,
) -> Result<Vec<TitleResult>, String> {
    let mut results = Vec::new();

    // Pass 1: Try EGCG generation
    if keyword.len() > 2 {
        let egcg_results = generator.generate(keyword, categories, style, genre, quantity);
        results.extend(egcg_results);
    }

    // Pass 2: Template engine fills remaining slots
    let remaining = (quantity as usize).saturating_sub(results.len());
    if remaining > 0 {
        let template_results = generate_from_templates(
            conn, keyword, categories, style, genre, remaining as u32,
        )?;
        results.extend(template_results);
    }

    // Finalize: deduplicate, sort by score, truncate
    results.sort_by(|a, b| b.score.cmp(&a.score));
    results.dedup_by(|a, b| a.title.eq_ignore_ascii_case(&b.title));
    results.truncate(quantity as usize);

    Ok(results)
}

/// Map 80+ specialized pool names to the 8 available SQLite word pools.
fn slot_name_to_pool_name(slot_name: &str) -> &'static str {
    match slot_name {
        // Verb variants
        "verb" | "verbs" | "action_verb" | "action_verbs" | "action_verbs_ing" | "action_verbs_past"
        | "actions_positive" | "positive_action" | "positive_action_verb" | "positive_action_verbs"
        | "comparison_verb" | "comparison_verbs" | "imperative_verb" | "imperative_verbs"
        | "negative_action_verb" | "negative_action_verbs" | "transformational_verb"
        | "transformational_verbs" |         "gerund_verb" | "gerund_verbs" | "gerund_verb_2" | "gerund" | "gerunds"
        | "verb_ing" | "verbing" | "verb_ing2" | "gerund2" | "right_verb" | "wrong_verb"
        | "verb_alt" | "verb_2" | "verb_past" => "action_verbs",

        // Adjective variants
        "adjective" | "adjectives" | "power_adjective" | "power_adjectives" | "positive_adjective"
        | "positive_adjectives" | "negative_adjective" | "negative_adjectives" | "overused_adjective"
        | "overused_adjectives" | "contrarian_adjective" | "contrarian_adjectives"
        | "comparative_adjective" | "comparative_adjectives" | "descriptive_adjective"
        | "descriptive_adjectives" | "opinion_adjective" | "opinion_adjectives"
        | "adjectives_describing_movies" | "character_adjective" | "character_adjectives"
        | "superlative_adjective" | "superlative_adjectives" | "superlative"
        | "adjective1" | "adjective2" | "adjective_2" | "adjective_alt" | "adjective_opinion" => "power_adjectives",

        // Noun/topic variants
        "noun" | "nouns" | "common_noun" | "common_nouns" | "abstract_noun" | "abstract_nouns"
        | "nouns_contrast" | "nouns_identity" | "nouns_opposite" | "nouns_persona" | "nouns_plural"
        | "concept" | "concepts" | "theme" | "themes" | "scenario" | "scenarios"
        | "movie_topic" | "movie_topics" | "street_topic" | "street_topics" | "trend" | "trends"
        | "life_domain" | "life_domains" | "life_lesson" | "life_lessons" | "movie_element"
        | "movie_elements" | "profession_or_role" | "professions_or_roles" | "profession" | "professions"
        | "experience" | "experiences" | "audience" | "audiences" | "audience_type" | "audience_types"
        | "audience2" | "name" | "names" | "pronoun" | "pronouns" | "actor" | "actors" | "director"
        | "directors" | "director1" | "director2" | "genre" | "genres" | "different_genre"
        | "film_achievement" | "film_achievements" | "production_event" | "production_events"
        | "movie_title" | "movie_titles" | "adverb" | "adverbs" | "character_element"
        | "character_elements" | "common_pitfall" | "common_pitfalls" | "pitfall" | "pitfalls"
        | "topic" | "topics" | "topic1" | "topic2" | "topic_2" | "subject" | "another_movie"
        | "movie_aspect" | "movie_genre" | "life_aspect" | "event" | "lesson"
        | "opposite_noun" | "noun_a" | "noun_b" | "noun_alt" | "noun1" | "noun2" | "noun3"
        | "negative_trait" | "negative_traits" | "positive_trait" | "positive_traits"
        | "positive_emotion" | "positive_emotions" | "negative_emotion" | "negative_emotions" => "nouns",

        // Results/outcomes
        "result" | "results" | "outcome" | "outcomes" | "desired_outcome" | "desired_outcomes"
        | "desired_result" | "desired_results" | "benefit" | "benefits" | "achievement" => "results",

        // Timeframes
        "timeframe" | "timeframes" | "time" | "times" | "time_day" | "time_of_day" | "times_day"
        | "decade" | "decades" | "decade2" => "timeframes",

        // Emotions
        "emotion" | "emotions" | "emotional_state" | "emotional_states" | "emotions_adj"
        | "emotion2" | "emotions_negative" => "emotions",

        // Numbers
        "number" | "numbers" | "list_number" | "list_numbers" => "numbers",

        // Hooks
        "hook" | "hooks" | "question_word" | "question_words" | "alternative" | "alternatives" => "hooks",

        _ => "nouns",
    }
}

/// Template-based title generation (fills remaining slots after Markov).
fn generate_from_templates(
    conn: &Connection,
    keyword: &str,
    categories: &[String],
    style: &str,
    genre: &str,
    quantity: u32,
) -> Result<Vec<TitleResult>, String> {
    if categories.is_empty() {
        return Ok(vec![]);
    }

    // Load all word pools into memory (eliminates N+1 per-slot queries)
    let mut pools: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT pool_name, word FROM word_pools ORDER BY RANDOM()") {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            for row in rows.flatten() {
                pools.entry(row.0).or_default().push(row.1);
            }
        }
    }

    let mut rng = rand::thread_rng();
    let mut results = Vec::new();
    let per_cat = (quantity.max(categories.len() as u32) / categories.len() as u32).max(3) + 3;

    for cat in categories {
        let mut stmt = conn
            .prepare(
                "SELECT template, slots FROM patterns WHERE category = ?1 AND (genre = ?2 OR genre = 'any') AND (tone = ?3 OR tone = 'normal') ORDER BY RANDOM()"
            )
            .map_err(|e| e.to_string())?;

        let templates: Vec<(String, String)> = stmt
            .query_map(rusqlite::params![cat, genre, style], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| { if let Err(ref e) = r { eprintln!("Row skipped: {}", e); } r.ok() })
            .collect();

        if templates.is_empty() {
            continue;
        }

        for _ in 0..per_cat {
            if let Some((template, slots_json)) = templates.choose(&mut rng) {
                let slots: Vec<Slot> =
                    serde_json::from_str(slots_json).unwrap_or_default();
                let generated = fill_template(&pools, template, &slots, keyword, &mut rng);
                // Skip if title doesn't contain the keyword (produces unrelated results)
                let kw_lower = keyword.to_lowercase();
                let gen_lower = generated.to_lowercase();
                let has_keyword = gen_lower.contains(&kw_lower) || kw_lower.split_whitespace().any(|w| gen_lower.contains(w));
                if has_keyword && generated.len() > 5 && !results.iter().any(|r: &TitleResult| r.title == generated) {
                    let (score, breakdown) = calculate_score(&generated, keyword, cat);
                    results.push(TitleResult {
                        title: generated,
                        score,
                        categories: vec![cat.clone()],
                        breakdown: Some(breakdown),
                    });
                }
            }
        }
    }

    // Sort by score, deduplicate, and truncate to requested quantity
    results.sort_by(|a, b| b.score.cmp(&a.score));
    results.dedup_by(|a, b| a.title == b.title);
    results.truncate(quantity as usize);

    // Fallback: if still not enough, pull more random templates from all categories,
    // relaxing the genre/tone filter incrementally
    if results.len() < quantity as usize {
        let all_cats: Vec<&String> = categories.iter().collect();
        // Pass 1: try same genre and style
        for _ in 0..(quantity as usize) {
            if results.len() >= quantity as usize { break; }
            let cat = match all_cats.choose(&mut rng) { Some(c) => *c, None => break };
            let fb_rows: Vec<(String, String)> = match conn.prepare(
                "SELECT template, slots FROM patterns WHERE category = ?1 AND (genre = ?2 OR genre = 'any') AND (tone = ?3 OR tone = 'normal') ORDER BY RANDOM() LIMIT 1"
            ) {
                Ok(mut stmt) => {
                    stmt.query_map(rusqlite::params![cat, genre, style], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })
                    .ok()
                    .map(|rows| rows.filter_map(|r| { if let Err(ref e) = r { eprintln!("Row skipped: {}", e); } r.ok() }).collect())
                    .unwrap_or_default()
                }
                Err(_) => continue,
            };
            for (template, slots_json) in &fb_rows {
                let slots: Vec<Slot> = serde_json::from_str(slots_json).unwrap_or_default();
                let generated = fill_template(&pools, template, &slots, keyword, &mut rng);
                let kw_lower = keyword.to_lowercase();
                let gen_lower = generated.to_lowercase();
                let has_keyword = gen_lower.contains(&kw_lower) || kw_lower.split_whitespace().any(|w| gen_lower.contains(w));
                if has_keyword && generated.len() > 5 && !results.iter().any(|r: &TitleResult| r.title == generated) {
                    let (score, breakdown) = calculate_score(&generated, keyword, cat);
                    results.push(TitleResult { title: generated, score, categories: vec![cat.to_string()], breakdown: Some(breakdown) });
                }
            }
        }
    }
    // Pass 2: if still not enough, relax all filters — any category, any genre, any tone
    if results.len() < quantity as usize {
        let all_cats: Vec<&String> = categories.iter().collect();
        for _ in 0..(quantity as usize * 2) {
            if results.len() >= quantity as usize { break; }
            let cat = match all_cats.choose(&mut rng) { Some(c) => *c, None => break };
            let fb_rows: Vec<(String, String)> = match conn.prepare(
                "SELECT template, slots FROM patterns WHERE category = ?1 ORDER BY RANDOM() LIMIT 1"
            ) {
                Ok(mut stmt) => {
                    stmt.query_map(rusqlite::params![cat], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })
                    .ok()
                    .map(|rows| rows.filter_map(|r| { if let Err(ref e) = r { eprintln!("Row skipped: {}", e); } r.ok() }).collect())
                    .unwrap_or_default()
                }
                Err(_) => continue,
            };
            for (template, slots_json) in &fb_rows {
                let slots: Vec<Slot> = serde_json::from_str(slots_json).unwrap_or_default();
                let generated = fill_template(&pools, template, &slots, keyword, &mut rng);
                let kw_lower = keyword.to_lowercase();
                let gen_lower = generated.to_lowercase();
                let has_keyword = gen_lower.contains(&kw_lower) || kw_lower.split_whitespace().any(|w| gen_lower.contains(w));
                if has_keyword && generated.len() > 5 && !results.iter().any(|r: &TitleResult| r.title == generated) {
                    let (score, breakdown) = calculate_score(&generated, keyword, cat);
                    results.push(TitleResult { title: generated, score, categories: vec![cat.to_string()], breakdown: Some(breakdown) });
                }
            }
        }
    }

    results.truncate(quantity as usize);
    Ok(results)
}

#[derive(serde::Deserialize)]
struct Slot {
    name: String,
    pool: Option<String>,
    #[allow(dead_code)]
    pos: Option<String>,
}

/// Slot name aliases that expect an "-ing" gerund form rather than the bare
/// verb stored in the action_verbs pool (e.g. template text like "The Art of
/// {gerund_verb} {topic}" needs "Navigating", not "Navigate").
fn wants_gerund(slot_name: &str) -> bool {
    matches!(
        slot_name,
        "gerund_verb" | "gerund_verbs" | "gerund_verb_2" | "gerund" | "gerunds"
            | "verb_ing" | "verbing" | "verb_ing2" | "gerund2" | "action_verbs_ing"
    )
}

/// Roughly convert a base-form verb to its "-ing" (gerund) form. Heuristic,
/// not linguistically perfect (doesn't handle consonant-doubling like
/// run -> running), but far better than leaving a bare verb where a gerund
/// is grammatically required.
fn to_gerund(word: &str) -> String {
    if word.is_empty() {
        return word.to_string();
    }
    let lower = word.to_lowercase();
    if lower.ends_with("ing") {
        return word.to_string();
    }
    if lower.ends_with('e') && !lower.ends_with("ee") && !lower.ends_with("oe") {
        return format!("{}ing", &word[..word.len() - 1]);
    }
    format!("{}ing", word)
}

fn fill_template(
    pools: &std::collections::HashMap<String, Vec<String>>,
    template: &str,
    slots: &[Slot],
    keyword: &str,
    rng: &mut impl Rng,
) -> String {
    /// Small words that should remain lowercase in title-case when not leading.
    const SMALL_WORDS: &[&str] = &[
        "a", "an", "the", "in", "of", "to", "for", "and", "or", "but",
        "by", "with", "at", "from", "on", "as", "is", "it",
    ];

    fn title_case_word(word: &str, is_first: bool) -> String {
        if word.is_empty() {
            return word.to_string();
        }
        let lower = word.to_lowercase();
        if !is_first && SMALL_WORDS.contains(&lower.as_str()) {
            return lower;
        }
        let mut chars = word.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => word.to_string(),
        }
    }

    let mut result = template.to_string();
    let mut filled_words: Vec<String> = Vec::new();

    for slot in slots {
        let placeholder = format!("{{{}}}", slot.name);
        let mut raw = match slot.name.as_str() {
            "keyword" | "topic" => keyword.to_string(),
            "number" => format!("{}", rng.gen_range(3..=12)),
            _ => {
                let pool_name = slot.pool.as_deref()
                    .map(|p| slot_name_to_pool_name(p))
                    .unwrap_or_else(|| slot_name_to_pool_name(&slot.name));

                let base_pool: Vec<String> = pools.get(pool_name)
                    .filter(|w| !w.is_empty())
                    .or_else(|| pools.get("nouns"))
                    .cloned()
                    .unwrap_or_default();

                // Prefer a word not already used elsewhere in this title
                // (avoids "The hidden and hidden Sides of X" repeats).
                let lower_filled: Vec<String> = filled_words.iter().map(|w| w.to_lowercase()).collect();
                let unused: Vec<String> = base_pool.iter()
                    .filter(|w| !lower_filled.contains(&w.to_lowercase()))
                    .cloned()
                    .collect();

                if !unused.is_empty() {
                    unused[rng.gen_range(0..unused.len())].clone()
                } else if !base_pool.is_empty() {
                    base_pool[rng.gen_range(0..base_pool.len())].clone()
                } else {
                    keyword.to_string()
                }
            }
        };

        if wants_gerund(&slot.name) {
            raw = to_gerund(&raw);
        }

        filled_words.push(raw.clone());
        let is_first_word = result.starts_with(&placeholder);
        let replacement = title_case_word(&raw, is_first_word);
        result = result.replace(&placeholder, &replacement);
    }

    result
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