use rand::seq::SliceRandom;
use rand::Rng;
use rusqlite::Connection;
use serde_json;

use crate::TitleResult;

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

pub fn generate(
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
                if generated.len() > 5 && !results.iter().any(|r: &TitleResult| r.title == generated) {
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

    // Fallback: if still not enough, pull more random templates from all categories
    if results.len() < quantity as usize {
        let all_cats: Vec<&String> = categories.iter().collect();
        for _ in 0..(quantity as usize * 2) {
            if results.len() >= quantity as usize {
                break;
            }
            let cat = match all_cats.choose(&mut rng) {
                Some(c) => *c,
                None => break,
            };
            // Collect eagerly so the PreparedStatement can be dropped
            let fallback_rows: Vec<(String, String)> = match conn.prepare(
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
                Err(_) => break,
            };
            for (template, slots_json) in &fallback_rows {
                let slots: Vec<Slot> = serde_json::from_str(slots_json).unwrap_or_default();
                let generated = fill_template(&pools, template, &slots, keyword, &mut rng);
                if generated.len() > 5 && !results.iter().any(|r: &TitleResult| r.title == generated) {
                    let (score, breakdown) = calculate_score(&generated, keyword, cat);
                    results.push(TitleResult {
                        title: generated,
                        score,
                        categories: vec![cat.to_string()],
                        breakdown: Some(breakdown),
                    });
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

fn fill_template(
    pools: &std::collections::HashMap<String, Vec<String>>,
    template: &str,
    slots: &[Slot],
    keyword: &str,
    rng: &mut impl Rng,
) -> String {
    let mut result = template.to_string();

    for slot in slots {
        let placeholder = format!("{{{}}}", slot.name);
        let replacement = match slot.name.as_str() {
            "keyword" | "topic" => keyword.to_string(),
            "number" => format!("{}", rng.gen_range(3..=12)),
            _ => {
                let pool_name = slot.pool.as_deref()
                    .map(|p| slot_name_to_pool_name(p))
                    .unwrap_or_else(|| slot_name_to_pool_name(&slot.name));

                let word = pools.get(pool_name).and_then(|w| {
                    if w.is_empty() { None } else { Some(w[rng.gen_range(0..w.len())].clone()) }
                });

                word.unwrap_or_else(|| {
                    pools.get("nouns").and_then(|w| {
                        if w.is_empty() { None } else { Some(w[rng.gen_range(0..w.len())].clone()) }
                    }).unwrap_or_else(|| keyword.to_string())
                })
            }
        };
        result = result.replace(&placeholder, &replacement);
    }

    if let Some(c) = result.chars().next() {
        result.replace_range(..1, &c.to_uppercase().to_string());
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