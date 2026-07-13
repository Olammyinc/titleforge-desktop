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
    let db = state.db.lock().map_err(|e| e.to_string())?;
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
    let db = state.db.lock().map_err(|e| e.to_string())?;

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
    let db = state.db.lock().map_err(|e| e.to_string())?;
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
    let db = state.db.lock().map_err(|e| e.to_string())?;
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
        .filter_map(|r| r.ok())
        .collect();

    Ok(entries)
}

// ── Favorites ──

#[tauri::command]
fn get_favorites(state: tauri::State<AppState>) -> Result<Vec<FavoriteEntry>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
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
        .filter_map(|r| r.ok())
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
    let db = state.db.lock().map_err(|e| e.to_string())?;

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
    let db = state.db.lock().map_err(|e| e.to_string())?;
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
        .filter_map(|r| r.ok())
        .collect();

    Ok(entries)
}

#[tauri::command]
fn create_project(name: String, state: tauri::State<AppState>) -> Result<ProjectEntry, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;

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
    let db = state.db.lock().map_err(|e| e.to_string())?;
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
    let db = state.db.lock().map_err(|e| e.to_string())?;
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
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.execute(
        "UPDATE project_titles SET notes = ?1 WHERE project_id = ?2 AND title = ?3",
        rusqlite::params![notes, project_id, title],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ── Settings ──

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> Result<std::collections::HashMap<String, String>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = db
        .prepare("SELECT key, value FROM user_settings")
        .map_err(|e| e.to_string())?;

    let map: std::collections::HashMap<String, String> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(map)
}

#[tauri::command]
fn set_setting(key: String, value: String, state: tauri::State<AppState>) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.execute(
        "INSERT OR REPLACE INTO user_settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ── License Validation ──

#[tauri::command]
fn validate_license(key: String, email: String, state: tauri::State<AppState>) -> Result<serde_json::Value, String> {
    // Check if already cached as valid
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let cached: String = db
        .query_row(
            "SELECT value FROM user_settings WHERE key = 'license_status'",
            [],
            |row| row.get(0),
        )
        .unwrap_or_default();

    if cached == "valid" {
        return Ok(serde_json::json!({ "valid": true, "tier": "basic", "cached": true }));
    }

    // Call the web app's validation endpoint
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!(
        "https://titleforge-tool.netlify.app/.netlify/functions/licenses?action=validate&key={}&email={}",
        urlencoding(&key),
        urlencoding(&email)
    );

    let resp = client.get(&url).send().map_err(|e| {
        // If offline, check if we have a cached valid license
        let cached_tier: String = db
            .query_row("SELECT value FROM user_settings WHERE key = 'license_tier'", [], |row| row.get(0))
            .unwrap_or_default();
        if !cached_tier.is_empty() {
            return String::new(); // silent, handled below
        }
        format!("Could not validate license online: {}", e)
    });

    if let Ok(response) = resp {
        if let Ok(data) = response.json::<serde_json::Value>() {
            if data.get("valid").and_then(|v| v.as_bool()).unwrap_or(false) {
                let tier = data.get("tier").and_then(|v| v.as_str()).unwrap_or("basic");
                db.execute("INSERT OR REPLACE INTO user_settings (key, value) VALUES ('license_status', 'valid')", []).ok();
                db.execute("INSERT OR REPLACE INTO user_settings (key, value) VALUES ('license_tier', ?1)", rusqlite::params![tier]).ok();
                return Ok(serde_json::json!({ "valid": true, "tier": tier }));
            }
        }
    }

    // Check cached license as fallback
    let cached_tier: String = db
        .query_row("SELECT value FROM user_settings WHERE key = 'license_tier'", [], |row| row.get(0))
        .unwrap_or_default();
    if !cached_tier.is_empty() {
        return Ok(serde_json::json!({ "valid": true, "tier": cached_tier, "cached": true }));
    }

    Ok(serde_json::json!({ "valid": false }))
}

#[tauri::command]
fn deactivate_license(state: tauri::State<AppState>) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.execute("DELETE FROM user_settings WHERE key LIKE 'license_%'", []).map_err(|e| e.to_string())?;
    Ok(())
}

fn urlencoding(s: &str) -> String {
    s.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        _ => format!("%{:02X}", c as u8),
    }).collect()
}

// ── Seed Check ──

#[tauri::command]
fn get_app_info(state: tauri::State<AppState>) -> Result<serde_json::Value, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM patterns", [], |row| row.get(0))
        .unwrap_or(0);
    Ok(serde_json::json!({
        "app": "titleforge-desktop",
        "version": "1.0.0",
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
            // Look for seed-data.json next to the binary, or in the resource dir
            let seed_paths = [
                std::path::PathBuf::from("seed-data.json"),
                app_dir.join("seed-data.json"),
            ];
            for sp in &seed_paths {
                if sp.exists() {
                    if let Err(e) = db::import_seed(&conn, sp) {
                        eprintln!("Warning: seed import failed: {}", e);
                    } else {
                        println!("Seed data imported from {:?}", sp);
                    }
                    break;
                }
            }
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            db: Mutex::new(conn),
        })
        .invoke_handler(tauri::generate_handler![
            generate_titles,
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
