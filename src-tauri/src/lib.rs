#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

mod db;
mod engine;

pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TitleResult {
    pub title: String,
    pub score: u32,
    pub categories: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub breakdown: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    pub id: i64,
    pub keyword: String,
    pub categories: String,
    pub genre: String,
    pub style: String,
    pub titles: String, // JSON string of TitleResult[]
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FavoriteEntry {
    pub id: i64,
    pub title: String,
    pub keyword: String,
    pub score: i64,
    pub category: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProjectEntry {
    pub id: i64,
    pub name: String,
    pub titles: String, // JSON string
    pub created_at: String,
}

#[derive(Serialize, Deserialize)]
pub struct UsageStats {
    pub total_generations: i64,
    pub total_titles: i64,
    pub today_generations: i64,
}

// ── Title Generation ──

#[tauri::command]
fn generate_titles(
    keyword: String,
    categories: Vec<String>,
    style: String,
    genre: String,
    quantity: u32,
    state: tauri::State<AppState>,
) -> Result<Vec<TitleResult>, String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    engine::generate(&db, &keyword, &categories, &style, &genre, quantity)
}

#[tauri::command]
fn get_categories() -> Vec<&'static str> {
    vec![
        "book", "article", "blog", "movie", "song", "youtube",
        "podcast", "newsletter", "ebook", "speech", "album",
        "poem", "street", "character", "product", "childname",
    ]
}

// ── Usage & History ──

#[tauri::command]
fn get_usage_stats(state: tauri::State<AppState>) -> Result<serde_json::Value, String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());

    let total_gens: i64 = db
        .query_row("SELECT COUNT(*) FROM user_history", [], |row| row.get(0))
        .unwrap_or(0);

    let today_gens: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM user_history WHERE date(created_at) = date('now')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let total_titles: i64 = db
        .query_row("SELECT COUNT(*) FROM user_favorites", [], |row| row.get(0))
        .unwrap_or(0);

    Ok(serde_json::json!({
        "totalGenerations": total_gens,
        "todayGenerations": today_gens,
        "totalFavorites": total_titles,
        "isPro": true,
    }))
}

#[tauri::command]
fn record_generation(
    keyword: String,
    categories: Vec<String>,
    genre: String,
    style: String,
    titles: Vec<TitleResult>,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let cats_json = categories.join(",");
    let titles_json = serde_json::to_string(&titles).map_err(|e| e.to_string())?;

    db.execute(
        "INSERT INTO user_history (keyword, categories, genre, style, titles) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![keyword, cats_json, genre, style, titles_json],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn get_history(state: tauri::State<AppState>) -> Result<Vec<HistoryEntry>, String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let mut stmt = db
        .prepare("SELECT id, keyword, categories, genre, style, titles, created_at FROM user_history ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;

    let entries = stmt
        .query_map([], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                keyword: row.get(1)?,
                categories: row.get(2)?,
                genre: row.get::<_, String>(3).unwrap_or_default(),
                style: row.get::<_, String>(4).unwrap_or_default(),
                titles: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| { if let Err(ref e) = r { eprintln!("Row skipped: {}", e); } r.ok() })
        .collect();

    Ok(entries)
}

// ── Favorites ──

#[tauri::command]
fn get_favorites(state: tauri::State<AppState>) -> Result<Vec<FavoriteEntry>, String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let mut stmt = db
        .prepare("SELECT id, title, COALESCE(keyword,''), COALESCE(score,0), COALESCE(category,''), created_at FROM user_favorites ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;

    let entries = stmt
        .query_map([], |row| {
            Ok(FavoriteEntry {
                id: row.get(0)?,
                title: row.get(1)?,
                keyword: row.get(2)?,
                score: row.get(3)?,
                category: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| { if let Err(ref e) = r { eprintln!("Row skipped: {}", e); } r.ok() })
        .collect();

    Ok(entries)
}

#[tauri::command]
fn toggle_favorite(
    title: String,
    keyword: String,
    score: i64,
    category: String,
    state: tauri::State<AppState>,
) -> Result<bool, String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());

    // Check if already favorited
    let exists: bool = db
        .query_row(
            "SELECT COUNT(*) > 0 FROM user_favorites WHERE title = ?1",
            rusqlite::params![title],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if exists {
        db.execute(
            "DELETE FROM user_favorites WHERE title = ?1",
            rusqlite::params![title],
        )
        .map_err(|e| e.to_string())?;
        Ok(false) // now unfavorited
    } else {
        db.execute(
            "INSERT INTO user_favorites (title, keyword, score, category) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![title, keyword, score, category],
        )
        .map_err(|e| e.to_string())?;
        Ok(true) // now favorited
    }
}

// ── Projects ──

#[tauri::command]
fn get_projects(state: tauri::State<AppState>) -> Result<Vec<ProjectEntry>, String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let mut stmt = db
        .prepare(
            "SELECT p.id, p.name, COALESCE(p.created_at,''), 
                    COALESCE((SELECT json_group_array(json_object('title', pt.title, 'keyword', pt.keyword, 'score', pt.score, 'notes', pt.notes)) 
                     FROM project_titles pt WHERE pt.project_id = p.id), '[]') as titles
             FROM user_projects p ORDER BY p.created_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let entries = stmt
        .query_map([], |row| {
            Ok(ProjectEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                titles: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| { if let Err(ref e) = r { eprintln!("Row skipped: {}", e); } r.ok() })
        .collect();

    Ok(entries)
}

#[tauri::command]
fn create_project(name: String, state: tauri::State<AppState>) -> Result<ProjectEntry, String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());

    db.execute(
        "INSERT INTO user_projects (name) VALUES (?1)",
        rusqlite::params![name],
    )
    .map_err(|e| e.to_string())?;

    let id = db.last_insert_rowid();

    Ok(ProjectEntry {
        id,
        name,
        titles: "[]".to_string(),
        created_at: String::new(),
    })
}

#[tauri::command]
fn delete_project(project_id: i64, state: tauri::State<AppState>) -> Result<(), String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    db.execute(
        "DELETE FROM project_titles WHERE project_id = ?1",
        rusqlite::params![project_id],
    )
    .map_err(|e| e.to_string())?;
    db.execute(
        "DELETE FROM user_projects WHERE id = ?1",
        rusqlite::params![project_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn add_to_project(
    project_id: i64,
    title: String,
    keyword: String,
    score: i64,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    db.execute(
        "INSERT INTO project_titles (project_id, title, keyword, score) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![project_id, title, keyword, score],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn update_title_notes(
    project_id: i64,
    title: String,
    notes: String,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    db.execute(
        "UPDATE project_titles SET notes = ?1 WHERE project_id = ?2 AND title = ?3",
        rusqlite::params![notes, project_id, title],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Settings ──

/// Derive a simple XOR key from the machine hostname.
/// This is NOT strong encryption — it's a basic obfuscation to avoid
/// plaintext API keys sitting in SQLite. The hostname acts as a
/// device-local "key" so someone copying the DB file to another machine
/// won't get readable keys.
///
/// KNOWN LIMITATION: This is obfuscation, not encryption. A determined
/// attacker with filesystem access can extract the key. This should be
/// migrated to OS-level credential storage (keychain on macOS,
/// DPAPI/Windows Credential Manager on Windows, libsecret on Linux)
/// when Tauri has a stable keystore plugin.
fn xor_obfuscate(input: &str) -> String {
    let hostkey = hostname::get()
        .unwrap_or_else(|_| std::ffi::OsString::from("titleforge-fallback"))
        .to_string_lossy()
        .into_owned();
    let key_bytes = hostkey.as_bytes();
    let input_bytes = input.as_bytes();
    let mut output = Vec::with_capacity(input_bytes.len());
    for (i, b) in input_bytes.iter().enumerate() {
        output.push(b ^ key_bytes[i % key_bytes.len()]);
    }
    // Store as hex-encoded, prefixed with "obf:" marker
    format!("obf:{}", hex_encode(&output))
}

fn xor_deobfuscate(stored: &str) -> String {
    if !stored.starts_with("obf:") {
        return stored.to_string(); // not obfuscated — return as-is
    }
    let hex_part = &stored[4..]; // strip "obf:" prefix
    let decoded = match hex_decode(hex_part) {
        Some(v) => v,
        None => return stored.to_string(), // corrupt data, return raw
    };
    let hostkey = hostname::get()
        .unwrap_or_else(|_| std::ffi::OsString::from("titleforge-fallback"))
        .to_string_lossy()
        .into_owned();
    let key_bytes = hostkey.as_bytes();
    let mut output = Vec::with_capacity(decoded.len());
    for (i, b) in decoded.iter().enumerate() {
        output.push(b ^ key_bytes[i % key_bytes.len()]);
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

static SENSITIVE_KEY_PATTERNS: &[&str] = &["api_key", "apikey", "secret", "token", "password"];

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    SENSITIVE_KEY_PATTERNS.iter().any(|pat| lower.contains(pat))
}

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> Result<std::collections::HashMap<String, String>, String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let mut stmt = db
        .prepare("SELECT key, value FROM user_settings")
        .map_err(|e| e.to_string())?;

    let map: std::collections::HashMap<String, String> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| { if let Err(ref e) = r { eprintln!("Row skipped: {}", e); } r.ok() })
        .map(|(k, v)| {
            // Deobfuscate sensitive values on read
            let value = if is_sensitive_key(&k) {
                xor_deobfuscate(&v)
            } else {
                v
            };
            (k, value)
        })
        .collect();

    Ok(map)
}

#[tauri::command]
fn set_setting(key: String, value: String, state: tauri::State<AppState>) -> Result<(), String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());

    // Obfuscate sensitive values (API keys, tokens, etc.) before storing
    let stored_value = if is_sensitive_key(&key) {
        xor_obfuscate(&value)
    } else {
        value
    };

    db.execute(
        "INSERT OR REPLACE INTO user_settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, stored_value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ── License Validation ──

#[tauri::command]
fn validate_license(key: String, email: String, state: tauri::State<AppState>) -> Result<serde_json::Value, String> {
    let url = format!(
        "https://titleforge-tool.netlify.app/.netlify/functions/licenses?action=validate&key={}&email={}",
        urlencoding(&key),
        urlencoding(&email)
    );

    // Run HTTP call on a background thread to avoid blocking the UI
    let result = std::thread::spawn(move || -> Option<(bool, String)> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build().ok()?;
        let resp = client.get(&url).send().ok()?;
        let data: serde_json::Value = resp.json().ok()?;
        let valid = data.get("valid").and_then(|v| v.as_bool()).unwrap_or(false);
        let tier = data.get("tier").and_then(|v| v.as_str()).unwrap_or("basic").to_string();
        Some((valid, tier))
    }).join().map_err(|_| "Thread panicked".to_string())?;

    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());

    if let Some((is_valid, tier)) = result {
        if is_valid {
            let now = chrono::Utc::now().to_rfc3339();
            db.execute("INSERT OR REPLACE INTO user_settings (key, value) VALUES ('license_status', 'valid')", []).ok();
            db.execute("INSERT OR REPLACE INTO user_settings (key, value) VALUES ('license_tier', ?1)", rusqlite::params![&tier]).ok();
            db.execute("INSERT OR REPLACE INTO user_settings (key, value) VALUES ('license_validated_at', ?1)", rusqlite::params![&now]).ok();
            return Ok(serde_json::json!({ "valid": true, "tier": tier }));
        } else {
            db.execute("DELETE FROM user_settings WHERE key LIKE 'license_%'", []).ok();
            return Ok(serde_json::json!({ "valid": false }));
        }
    }

    // Server unreachable — use cache if < 24 hours old
    let cached_status: String = db
        .query_row("SELECT value FROM user_settings WHERE key = 'license_status'", [], |row| row.get(0))
        .unwrap_or_default();

    if cached_status == "valid" {
        let validated_at: String = db
            .query_row("SELECT value FROM user_settings WHERE key = 'license_validated_at'", [], |row| row.get(0))
            .unwrap_or_default();

        if !validated_at.is_empty() {
            if let Ok(parsed_time) = chrono::DateTime::parse_from_rfc3339(&validated_at) {
                if chrono::Utc::now().signed_duration_since(parsed_time).num_hours() < 24 {
                    let cached_tier: String = db
                        .query_row("SELECT value FROM user_settings WHERE key = 'license_tier'", [], |row| row.get(0))
                        .unwrap_or_default();
                    return Ok(serde_json::json!({ "valid": true, "tier": cached_tier, "cached": true }));
                }
            }
        }
        db.execute("DELETE FROM user_settings WHERE key LIKE 'license_%'", []).ok();
    }

    Ok(serde_json::json!({ "valid": false, "error": "Could not reach license server" }))
}

#[tauri::command]
fn deactivate_license(state: tauri::State<AppState>) -> Result<(), String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    db.execute("DELETE FROM user_settings WHERE key LIKE 'license_%'", []).map_err(|e| e.to_string())?;
    Ok(())
}

fn urlencoding(s: &str) -> String {
    s.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        _ => format!("%{:02X}", c as u8),
    }).collect()
}

// ── AI-Powered Generation (user brings their own key) ──

const AI_PROVIDERS: &[(&str, &str, &str, bool)] = &[
    ("openai", "https://api.openai.com/v1/chat/completions", "gpt-4o-mini", false),
    ("deepseek", "https://api.deepseek.com/v1/chat/completions", "deepseek-v4-flash", false),
    ("anthropic", "https://api.anthropic.com/v1/messages", "claude-sonnet-4-5", true),
    ("gemini", "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions", "gemini-2.0-flash", false),
];

#[tauri::command]
fn generate_with_ai(
    keyword: String,
    categories: Vec<String>,
    style: String,
    genre: String,
    quantity: u32,
    provider: String,
    api_key: String,
    cross_medium: bool,
    include_subtitles: bool,
    include_translation: bool,
    translate_lang: Option<String>,
    gender: Option<String>,
    finetune: Option<serde_json::Value>,
) -> Result<Vec<TitleResult>, String> {
    let provider_info = AI_PROVIDERS.iter().find(|p| p.0 == provider)
        .ok_or_else(|| format!("Unsupported provider: {}", provider))?;

    let url = provider_info.1;
    let model = provider_info.2;
    let is_anthropic = provider_info.3;

    let cat_list = categories.join(", ");
    let genre_text = if genre == "any" { String::new() } else { format!(" in the {} genre", genre) };
    let style_desc = match style.as_str() {
        "shout" => "bold, attention-grabbing, high-impact",
        "whisper" => "subtle, understated, quietly intriguing",
        "blessing" => "wholesome, uplifting, positive",
        "provocative" => "controversial, bold stance, sparks debate",
        "minimalist" => "ultra-clean, 2-4 words max",
        "storytelling" => "narrative framing, anecdotal story hook",
        "question" => "framed as a question",
        "playful" => "clever, witty, sharp but light",
        _ => "clear, direct, professional",
    };

    let mut extra = String::new();
    if cross_medium { extra.push_str("\n- Adapt each title to its specific medium — a YouTube title should not read like a book title"); }
    if include_subtitles { extra.push_str("\n- Include a subtitle for each title"); }
    if include_translation {
        let lang = translate_lang.as_deref().unwrap_or("Spanish");
        extra.push_str(&format!("\n- Include a translation into {}", lang));
    }
    if let Some(g) = gender {
        if g != "any" { extra.push_str(&format!("\n- Use {} names or perspectives", g)); }
    }
    if let Some(ref ft) = finetune {
        if let Some(aud) = ft.get("audience").and_then(|v| v.as_str()) {
            extra.push_str(&format!("\n- Target audience: {}", aud));
        }
        if let Some(em) = ft.get("emotion").and_then(|v| v.as_str()) {
            extra.push_str(&format!("\n- Primary emotion: {}", em));
        }
        if let Some(len) = ft.get("length").and_then(|v| v.as_str()) {
            extra.push_str(&format!("\n- Title length: {}", len));
        }
        if let Some(angle) = ft.get("angle").and_then(|v| v.as_str()) {
            extra.push_str(&format!("\n- Angle: {}", angle));
        }
        if let Some(must) = ft.get("mustInclude").and_then(|v| v.as_str()) {
            extra.push_str(&format!("\n- MUST include these words: {}", must));
        }
        if let Some(avoid) = ft.get("avoid").and_then(|v| v.as_str()) {
            extra.push_str(&format!("\n- AVOID these words: {}", avoid));
        }
    }

    let prompt = format!(
        "Generate {} powerful, click-worthy titles about \"{}\" for: {}{}.\n\nCommunication style: {}\n\n\
        QUALITY RULES:\n- Emotional pull: make the reader feel something\n\
        - Specificity: use concrete details, numbers, vivid specifics\n\
        - Curiosity gap: the reader should need to click to satisfy an open question\n\
        - No filler: every title must be genuinely strong\n\
        - Variety: mix structures\n\
        - No cliches: avoid AI cliches{}\n\n\
        Return a JSON object with a \"titles\" key containing an array of objects with title, score (0-100), and breakdown with curiosityGap, emotionalTrigger, powerWords, lengthAnalysis, specificity fields.\n\n\
        EVERY title must have a complete breakdown with all 5 fields.\n\n\
        Remember: every title must be about \"{}\".",
        quantity, keyword, cat_list, genre_text, style_desc, extra, keyword
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let response_text: String;

    if is_anthropic {
        let body = serde_json::json!({
            "model": model,
            "max_tokens": 4096,
            "temperature": 0.85,
            "system": "You are TitleForge, an elite title generator. Generate titles that people actually click. Before you write each title, ask: 'Would I click this?' If the answer is no, replace it. Return ONLY valid JSON.",
            "messages": [
                {"role": "user", "content": prompt}
            ]
        });

        let resp = client.post(url)
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .map_err(|e| format!("API request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(format!("API error ({}): Provider returned an error", status));
        }

        let data: serde_json::Value = resp.json().map_err(|e| format!("Failed to parse response: {}", e))?;
        response_text = data["content"][0]["text"].as_str().unwrap_or("").to_string();
    } else {
        let body = serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": "You are TitleForge, an elite title generator. Generate titles that people actually click. Before you write each title, ask: 'Would I click this?' If the answer is no, replace it. Return ONLY valid JSON."},
                {"role": "user", "content": prompt}
            ],
            "temperature": 0.85,
            "max_tokens": 4096,
            "response_format": {"type": "json_object"}
        });

        let resp = client.post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .map_err(|e| format!("API request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(format!("API error ({}): Provider returned an error", status));
        }

        let data: serde_json::Value = resp.json().map_err(|e| format!("Failed to parse response: {}", e))?;
        response_text = data["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
    }

    // Clean and parse JSON
    let cleaned = response_text
        .replace("```json", "")
        .replace("```", "")
        .trim()
        .to_string();

    let parsed: serde_json::Value = serde_json::from_str(&cleaned)
        .map_err(|_| "AI returned malformed JSON. Try again.".to_string())?;

    let titles_array = parsed["titles"].as_array()
        .or_else(|| parsed.as_array())
        .ok_or("AI response missing titles array".to_string())?;

    let results: Vec<TitleResult> = titles_array.iter()
        .filter_map(|item| {
            let title = item["title"].as_str()?.trim().to_string();
            if title.is_empty() { return None; }
            let score = item["score"].as_u64().unwrap_or(50).min(100) as u32;
            Some(TitleResult { title, score, categories: categories.clone(), breakdown: item.get("breakdown").cloned() })
        })
        .collect();

    if results.is_empty() {
        return Err("AI generated no valid titles. Try a different keyword.".to_string());
    }

    Ok(results.into_iter().take(quantity as usize).collect())
}

// ── Seed Check ──

#[tauri::command]
fn get_app_info(state: tauri::State<AppState>) -> Result<serde_json::Value, String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM patterns", [], |row| row.get(0))
        .unwrap_or(0);
    Ok(serde_json::json!({
        "app": "titleforge-desktop",
        "version": env!("CARGO_PKG_VERSION"),
        "seeded": count > 0,
        "templateCount": count,
    }))
}

pub fn run() {
    let app_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("titleforge-desktop");
    std::fs::create_dir_all(&app_dir).ok();

    let db_path = app_dir.join("titles.db");
    let conn = db::init_db(&db_path).expect("Failed to initialize database");

    // Seed on first launch if tables are empty
    {
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM patterns", [], |row| row.get(0))
            .unwrap_or(0);
        if count == 0 {
            // Try file paths first (resource dir, app dir, CWD)
            let seed_paths = [
                std::path::PathBuf::from("seed-data.json"),
                app_dir.join("seed-data.json"),
            ];
            let mut imported = false;
            for sp in &seed_paths {
                if sp.exists() {
                    if let Err(e) = db::import_seed(&conn, sp) {
                        eprintln!("Warning: seed import from file failed: {}", e);
                    } else {
                        println!("Seed data imported from {:?}", sp);
                        imported = true;
                    }
                    break;
                }
            }
            // Guaranteed fallback: embed seed-data.json in the binary
            if !imported {
                println!("Seed file not found on disk, using embedded seed data...");
                if let Err(e) = db::import_seed_from_str(&conn, include_str!("../../seed-data.json")) {
                    eprintln!("Warning: embedded seed import failed: {}", e);
                } else {
                    println!("Seed data imported from embedded binary data");
                }
            }
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            db: Mutex::new(conn),
        })
        .invoke_handler(tauri::generate_handler![
            generate_titles,
            generate_with_ai,
            get_categories,
            get_usage_stats,
            record_generation,
            get_history,
            get_favorites,
            toggle_favorite,
            get_projects,
            create_project,
            delete_project,
            add_to_project,
            update_title_notes,
            get_settings,
            set_setting,
            get_app_info,
            validate_license,
            deactivate_license,
        ])
        .run(tauri::generate_context!())
        .expect("Error running TitleForge Desktop");
}
