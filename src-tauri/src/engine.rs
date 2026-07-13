use rand::seq::SliceRandom;
use rand::Rng;
use rusqlite::Connection;
use serde_json;

use crate::TitleResult;

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

    for cat in categories {
        let mut stmt = conn
            .prepare(
                "SELECT template, slots, quality_score FROM patterns WHERE category = ?1 AND (genre = ?2 OR genre = 'any') AND (tone = ?3 OR tone = 'normal') ORDER BY quality_score DESC",
            )
            .map_err(|e| e.to_string())?;

        let templates: Vec<(String, String, f64)> = stmt
            .query_map(rusqlite::params![cat, genre, style], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        if templates.is_empty() {
            continue;
        }

        for _ in 0..(quantity / categories.len() as u32).max(1) {
            if let Some((template, slots_json, _score)) = templates.choose(&mut rng) {
                let slots: Vec<Slot> =
                    serde_json::from_str(slots_json).unwrap_or_default();
                let title = fill_template(template, &slots, keyword, &mut rng);
                if title.len() > 5 {
                    let score = calculate_score(&title, keyword, cat);
                    results.push(TitleResult {
                        title,
                        score,
                        categories: vec![cat.clone()],
                        breakdown: None,
                    });
                }
            }
        }
    }

    // Deduplicate
    results.sort_by(|a, b| b.score.cmp(&a.score));
    results.dedup_by(|a, b| a.title == b.title);
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
            "timeframe" => pick_random(&[
                "Today", "This Week", "30 Days", "7 Days", "Right Now",
                "2026", "This Year", "Tonight", "Overnight",
            ], rng),
            "result" => pick_random(&[
                "That Will Change Your Life", "That Actually Work",
                "That Matter", "Worth Your Time", "That Deliver Results",
            ], rng),
            "audience" => pick_random(&[
                "for Beginners", "for Experts", "for Everyone",
                "for Busy People", "for Creators",
            ], rng),
            "emotion" => pick_random(&[
                "Hidden", "Secret", "Surprising", "Unexpected",
                "Essential", "Ultimate", "Forgotten",
            ], rng),
            "hook" => pick_random(&[
                "The Truth About", "What Nobody Tells You About",
                "The Surprising Science of", "Why You Should",
            ], rng),
            _ => {
                if let Some(pool) = &slot.pool {
                    match pool.as_str() {
                        "action_verbs" => pick_random(&[
                            "Master", "Build", "Create", "Transform",
                            "Unlock", "Hack", "Accelerate", "Simplify",
                        ], rng),
                        "power_adjectives" => pick_random(&[
                            "Essential", "Ultimate", "Radical", "Bold",
                            "Powerful", "Proven", "Smart", "Fast",
                        ], rng),
                        "nouns" => pick_random(&[
                            "Guide", "Blueprint", "Framework", "Toolkit",
                            "System", "Strategy", "Formula", "Method",
                        ], rng),
                        _ => keyword.to_string(),
                    }
                } else {
                    keyword.to_string()
                }
            }
        };
        result = result.replace(&placeholder, &replacement);
    }

    // Capitalize first letter
    if let Some(c) = result.chars().next() {
        result.replace_range(..1, &c.to_uppercase().to_string());
    }

    result
}

fn pick_random<'a>(items: &[&'a str], rng: &mut impl Rng) -> String {
    items.choose(rng).unwrap_or(&"").to_string()
}

fn calculate_score(title: &str, keyword: &str, _category: &str) -> u32 {
    let mut score = 50u32;

    // Bonus for including keyword
    if title.to_lowercase().contains(&keyword.to_lowercase()) {
        score += 15;
    }

    // Bonus for numbers
    if title.chars().any(|c| c.is_ascii_digit()) {
        score += 10;
    }

    // Bonus for curiosity (question marks, colons)
    if title.contains('?') || title.contains(':') {
        score += 10;
    }

    // Bonus for emotional words
    let emotional = [
        "secret", "hidden", "truth", "never", "wrong", "best", "worst",
        "ultimate", "essential", "proven", "easy", "fast", "simple",
    ];
    for word in &emotional {
        if title.to_lowercase().contains(word) {
            score += 5;
            break;
        }
    }

    // Penalty for very short or very long titles
    let len = title.len();
    if len < 15 {
        score = score.saturating_sub(10);
    } else if len > 100 {
        score = score.saturating_sub(10);
    }

    score.min(100)
}
