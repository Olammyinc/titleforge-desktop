#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

mod db;
mod engine;

pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
}

#[derive(Serialize, Deserialize)]
pub struct TitleResult {
    pub title: String,
    pub score: u32,
    pub categories: Vec<String>,
}

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

#[tauri::command]
fn get_usage(db_path: String) -> Result<serde_json::Value, String> {
    // Return basic app info for the dashboard
    Ok(serde_json::json!({ "app": "titleforge-desktop", "version": "1.0.0" }))
}

fn main() {
    let app_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("titleforge-desktop");
    std::fs::create_dir_all(&app_dir).ok();

    let db_path = app_dir.join("titles.db");
    let conn = db::init_db(&db_path).expect("Failed to initialize database");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            db: Mutex::new(conn),
        })
        .invoke_handler(tauri::generate_handler![
            generate_titles,
            get_categories,
            get_usage,
        ])
        .run(tauri::generate_context!())
        .expect("Error running TitleForge Desktop");
}
