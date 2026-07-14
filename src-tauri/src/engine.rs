use rand::seq::SliceRandom;
use rand::Rng;
use rusqlite::Connection;
use serde_json;

use crate::TitleResult;

/// Map 80+ specialized pool names to the 8 available SQLite word pools.
fn slot_name_to_pool_name(slot_name: &str) -> &'static str {
    match slot_name {
        "verb" | "verbs" | "action_verb" | "action_verbs" | "action_verbs_ing" | "action_verbs_past"
        | "actions_positive" | "positive_action_verbs" | "comparison_verbs" | "imperative_verbs"
        | "negative_action_verbs" | "transformational_verbs" | "gerund_verbs" | "gerunds" => "action_verbs",

        "adjective" | "adjectives" | "power_adjective" | "power_adjectives" | "positive_adjective"
        | "positive_adjectives" | "negative_adjectives" | "overused_adjectives" | "contrarian_adjectives"
        | "comparative_adjectives" | "descriptive_adjectives" | "opinion_adjectives"
        | "adjectives_describing_movies" | "character_adjectives" | "superlative_adjectives"
        | "superlatives" => "power_adjectives",

        "noun" | "nouns" | "common_nouns" | "abstract_nouns" | "nouns_contrast"
        | "nouns_identity" | "nouns_opposite" | "nouns_persona" | "nouns_plural"
        | "concepts" | "themes" | "scenarios" | "movie_topics" | "street_topics" | "trends"
        | "life_domains" | "life_lessons" | "movie_elements" | "professions_or_roles"
        | "experiences" | "audience" | "audiences" | "audience_types" | "audience_type"
        | "names" | "pronouns" | "professions" | "actors" | "directors" | "genres"
        | "film_achievements" | "production_events" | "movie_titles" | "adverbs"
        | "character_elements" | "common_pitfalls" | "topics" => "nouns",

        "result" | "results" | "outcomes" | "desired_outcomes" | "desired_results" | "benefits" => "results",
        "timeframe" | "timeframes" | "times" | "times_day" | "decades" => "timeframes",
        "emotion" | "emotions" | "emotional_states" | "emotions_adj" | "emotions_negative"
        | "positive_emotions" | "negative_traits" | "positive_traits" | "character_attributes" => "emotions",
        "number" | "numbers" | "list_numbers" => "numbers",
        "hook" | "hooks" | "question_words" | "alternatives" => "hooks",
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
            .filter_map(|r| r.ok())
            .collect();

        if templates.is_empty() {
            continue;
        }

        for _ in 0..per_cat {
            if let Some((template, slots_json)) = templates.choose(&mut rng) {
                let slots: Vec<Slot> =
                    serde_json::from_str(slots_json).unwrap_or_default();
                let title = fill_template(conn, template, &slots, keyword, &mut rng);
                if title.len() > 5 && !results.iter().any(|r| r.title == title) {
                    let (score, breakdown) = calculate_score(&title, keyword, cat);
                    results.push(TitleResult {
                        title,
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
            let mut stmt = match conn.prepare(
                "SELECT template, slots FROM patterns WHERE category = ?1 ORDER BY RANDOM() LIMIT 1"
            ) {
                Ok(s) => s,
                Err(_) => break,
            };
            if let Ok(rows) = stmt.query_map(rusqlite::params![cat], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) {
                for row in rows {
                    if let Ok((template, slots_json)) = row {
                        let slots: Vec<Slot> = serde_json::from_str(&slots_json).unwrap_or_default();
                        let title = fill_template(conn, &template, &slots, keyword, &mut rng);
                        if title.len() > 5 && !results.iter().any(|r| r.title == title) {
                            let (score, breakdown) = calculate_score(&title, keyword, cat);
                            results.push(TitleResult {
                                title,
                                score,
                                categories: vec![cat.to_string()],
                                breakdown: Some(breakdown),
                            });
                        }
                    }
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
    conn: &Connection,
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

                let word: Option<String> = conn
                    .query_row(
                        "SELECT word FROM word_pools WHERE pool_name = ?1 ORDER BY RANDOM() LIMIT 1",
                        rusqlite::params![pool_name],
                        |row| row.get(0),
                    )
                    .ok();

                word.unwrap_or_else(|| {
                    conn.query_row(
                        "SELECT word FROM word_pools WHERE pool_name = 'nouns' ORDER BY RANDOM() LIMIT 1",
                        [],
                        |row| row.get::<_, String>(0),
                    ).unwrap_or_else(|_| keyword.to_string())
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
    let mut has_power = false;

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
    if power.iter().any(|w| lower.contains(w)) { score += 5; has_power = true; }

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