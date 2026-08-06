#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::Emitter;

pub mod db;
pub mod engine;
pub mod local_llm;
pub mod prompt_spec;
pub mod seo;
pub mod title_gen;

pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    pub generator: std::sync::Mutex<title_gen::Generator>,
    pub local_llm: std::sync::Mutex<Option<local_llm::LocalLlm>>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TitleResult {
    pub title: String,
    pub score: u32,
    pub categories: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub breakdown: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 0–100 SEO score from seo::score_seo. None for pre-feature or cloud-AI results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seo_score: Option<u8>,
    /// Per-signal SEO breakdown, serialized from seo::SeoBreakdown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seo_breakdown: Option<serde_json::Value>,
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
async fn generate_titles(
    app: tauri::AppHandle,
    keyword: String,
    categories: Vec<String>,
    style: String,
    genre: String,
    quantity: u32,
    // Offline generation ignored fine-tune entirely until 2026-08-03 — the UI
    // exposed the controls and the engine silently dropped them. Same camelCase
    // JSON shape the frontend already sends to `generate_with_ai`.
    finetune: Option<serde_json::Value>,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<TitleResult>, String> {
    // Async command: Tauri v2 runs async commands on its worker runtime (off
    // the main / UI thread), so the WebView stays responsive during the
    // CPU-heavy LLM inference. (A `spawn_blocking` closure would need the
    // state to be Clone — it isn't — so the inline await is the practical
    // non-UI-blocking path here.)
    // Read tier and curated data before releasing DB lock.
    // The DB mutex must never be held during LLM inference (3.5s+/title).
    //
    // Offline caps (user decision 2026-07-31): lowered from 100/500 because
    // measured ~7-12s/title makes 100 ≈ 22 min and 500 ≈ 110 min — unshippable.
    // 50 ≈ 8 min, 200 ≈ 33 min worst-case. BYOK path (generate_with_ai) has
    // NO cap — users bring their own key precisely to generate large batches.
    let (tier, quantity) = {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        let tier = get_tier(&db);
        let cap: u32 = match tier.as_str() {
            "pro" => 50,
            "studio" => 200,
            _ => 25,
        };
        let quantity = quantity.min(cap);
        (tier, quantity)
    };

    let generator = state.generator.lock().unwrap_or_else(|e| e.into_inner());
    let mut llm_guard = state.local_llm.lock().unwrap_or_else(|e| e.into_inner());
    if llm_guard.is_none() {
        *llm_guard = lazy_load_llm();
    }
    // Re-acquire DB for the engine passes (fetch_curated_sample, fallback queries).
    // These are millisecond operations — safe to lock.
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let ft = prompt_spec::FineTune::from_json(finetune.as_ref());
    // Use the streaming variant so the UI can render ACROSS titles as they
    // land (U1): emit a Tauri event per accepted title. The label keeps the
    // tauri:// convention used elsewhere. Emit is non-blocking/best-effort —
    // a failure to emit must never fail generation.
    let mut accepted = 0usize;
    let mut emit_fn = |r: &TitleResult| {
        accepted += 1;
        let _ = app.emit(
            "titleforge://title-generated",
            serde_json::json!({ "accepted": accepted, "title": r.title }),
        );
    };
    engine::generate_streaming(&db, &generator, llm_guard.as_mut(), &keyword, &categories, &style, &genre, quantity, &tier, &ft, &mut emit_fn)
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

    let tier = get_tier(&db);

    Ok(serde_json::json!({
        "totalGenerations": total_gens,
        "todayGenerations": today_gens,
        "totalFavorites": total_titles,
        "isPro": tier != "core",
        "tier": tier,
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
    batch_titles: Option<Vec<String>>,
    batch_id: Option<String>,
    display_randomized: Option<bool>,
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

        // ── Revealed-preference capture (brief §4 Task 2b + handoff 5a) ──
        // When the user favorites ONE title out of a batch, the rest are the
        // ones they passed over — that is (batch - 1) labelled comparisons
        // from a single click, from the actual target user. Purely local.
        // No telemetry, no upload: this stays in the user's SQLite.
        //
        // handoff 5a: ALSO record the DISPLAYED RANK of the chosen title and
        // the batch size. Position bias dominates click data — people pick
        // from the top. Without rank, every label is confounded and cannot
        // be corrected afterwards. display_randomized marks batches whose
        // order was shuffled (a slice of batches), so those favourites are
        // near-experimental rather than correlational.
        if let Some(batch) = batch_titles {
            if batch.len() >= 2 && batch.iter().any(|t| *t == title) {
                let passed: Vec<&String> = batch.iter().filter(|t| **t != title).collect();
                let passed_json = serde_json::to_string(&passed).unwrap_or_else(|_| "[]".to_string());
                let bid = batch_id.unwrap_or_else(|| format!("{}-{}", keyword, category));
                // 1-based display rank of the chosen title within the batch.
                let chosen_rank = batch.iter().position(|t| *t == title)
                    .map(|i| (i + 1) as i64)
                    .unwrap_or(0);
                let batch_size = batch.len() as i64;
                let rand = if display_randomized.unwrap_or(false) { 1 } else { 0 };
                db.execute(
                    "INSERT INTO revealed_preference (batch_id, keyword, category, chosen_title, passed_over_titles, chosen_rank, batch_size, display_randomized)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![bid, keyword, category, title, passed_json, chosen_rank, batch_size, rand],
                )
                .map_err(|e| e.to_string())?;
            }
        }

        Ok(true) // now favorited
    }
}

// ── Projects ──

#[tauri::command]
fn get_projects(state: tauri::State<AppState>) -> Result<Vec<ProjectEntry>, String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    if get_tier(&db) == "core" {
        return Err(PRO_REQUIRED_MSG.to_string());
    }
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
    if get_tier(&db) == "core" {
        return Err(PRO_REQUIRED_MSG.to_string());
    }

    db.execute(
        "INSERT INTO user_projects (name) VALUES (?1)",
        rusqlite::params![name],
    )
    .map_err(|e| e.to_string())?;

    let id = db.last_insert_rowid();

    // Query the actual created_at value
    let created_at: String = db
        .query_row("SELECT COALESCE(created_at, '') FROM user_projects WHERE id = ?1",
            rusqlite::params![id], |row| row.get(0))
        .unwrap_or_default();

    Ok(ProjectEntry {
        id,
        name,
        titles: "[]".to_string(),
        created_at,
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
    if get_tier(&db) == "core" {
        return Err(PRO_REQUIRED_MSG.to_string());
    }
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
    if get_tier(&db) == "core" {
        return Err(PRO_REQUIRED_MSG.to_string());
    }
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
/// Store a sensitive value in the OS keyring (macOS Keychain, Windows Credential
/// Manager, Linux libsecret). Falls back to XOR-obfuscated SQLite storage if the
/// keyring is unavailable (headless Linux, restricted environments).
fn store_secret(key: &str, value: &str) {
    let entry = keyring::Entry::new("titleforge-desktop", key);
    match entry {
        Ok(e) => {
            if let Err(e) = e.set_password(value) {
                eprintln!("[keyring] store failed for '{}': {} — falling back to XOR", key, e);
                // Don't store in SQLite as fallback here — caller handles that
            }
        }
        Err(e) => {
            eprintln!("[keyring] entry creation failed for '{}': {} — falling back to XOR", key, e);
        }
    }
}

/// Retrieve a sensitive value from the OS keyring. Returns None if the keyring
/// is unavailable or the entry doesn't exist. The caller should fall back to
/// XOR-obfuscated SQLite.
fn retrieve_secret(key: &str) -> Option<String> {
    let entry = keyring::Entry::new("titleforge-desktop", key).ok()?;
    entry.get_password().ok()
}

/// Delete a sensitive value from the OS keyring.
fn delete_secret(key: &str) {
    if let Ok(entry) = keyring::Entry::new("titleforge-desktop", key) {
        let _ = entry.delete_credential();
    }
}

/// Legacy XOR obfuscation — kept as a fallback for environments where the OS
/// keyring is unavailable (headless Linux without libsecret, restricted
/// sandboxed environments, etc.). Also provides backward compatibility for
/// any existing API keys stored before the keyring migration.
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
            // Prefer OS keyring for sensitive values, fall back to XOR-deobfuscated SQLite
            let value = if is_sensitive_key(&k) {
                retrieve_secret(&k).unwrap_or_else(|| xor_deobfuscate(&v))
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

    // Store sensitive values in OS keyring (preferred) with XOR-obfuscated
    // SQLite as fallback for environments where the keyring is unavailable.
    if is_sensitive_key(&key) {
        if value.is_empty() {
            // Clearing: remove from keyring and SQLite
            delete_secret(&key);
            db.execute("DELETE FROM user_settings WHERE key = ?1", rusqlite::params![&key])
                .map_err(|e| e.to_string())?;
        } else {
            store_secret(&key, &value);
            // Also store XOR-obfuscated in SQLite as fallback
            let xor_val = xor_obfuscate(&value);
            db.execute(
                "INSERT OR REPLACE INTO user_settings (key, value) VALUES (?1, ?2)",
                rusqlite::params![&key, xor_val],
            )
            .map_err(|e| e.to_string())?;
        }
    } else {
        db.execute(
            "INSERT OR REPLACE INTO user_settings (key, value) VALUES (?1, ?2)",
            rusqlite::params![&key, &value],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ── License Validation ──

/// Shared upgrade prompt returned by every tier-gated command.
const PRO_REQUIRED_MSG: &str = "This feature requires a Pro or Studio license. Upgrade at titleforge-tool.netlify.app/desktop";

/// Read the locally-cached license tier from `user_settings`.
/// Returns "core" when no tier row exists — fail-closed.
fn get_tier(db: &rusqlite::Connection) -> String {
    db.query_row(
        "SELECT value FROM user_settings WHERE key = 'license_tier'",
        [],
        |row| row.get::<_, String>(0),
    )
    .unwrap_or_else(|_| "core".to_string())
}

#[tauri::command]
fn validate_license(key: String, email: String, state: tauri::State<AppState>) -> Result<serde_json::Value, String> {
    let machine = hostname::get()
        .unwrap_or_else(|_| std::ffi::OsString::from("unknown"))
        .to_string_lossy()
        .into_owned();
    let url = format!(
        "https://titleforge-tool.netlify.app/.netlify/functions/licenses?action=validate&key={}&email={}&machine={}",
        urlencoding(&key),
        urlencoding(&email),
        urlencoding(&machine)
    );

    // Run HTTP call on a background thread to avoid blocking the UI
    let result = std::thread::spawn(move || -> Option<(bool, String)> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build().ok()?;
        let resp = client.get(&url).send().ok()?;
        let data: serde_json::Value = resp.json().ok()?;
        let valid = data.get("valid").and_then(|v| v.as_bool()).unwrap_or(false);
        let tier = data.get("tier").and_then(|v| v.as_str()).unwrap_or("core").to_string();
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

/// Silently re-validate the license in the background.
/// On server reachable + valid: overwrites local cache.
/// On ANY failure (network error, server unreachable, invalid response,
/// server says invalid): does NOTHING — leaves existing cache untouched.
/// The foreground `validate_license` is the authority for revocation.
/// This is a refresh-only command to keep cache fresh, not to revoke.
#[tauri::command]
fn background_verify(key: String, email: String, state: tauri::State<AppState>) -> Result<(), String> {
    let machine = hostname::get()
        .unwrap_or_else(|_| std::ffi::OsString::from("unknown"))
        .to_string_lossy()
        .into_owned();
    let url = format!(
        "https://titleforge-tool.netlify.app/.netlify/functions/licenses?action=validate&key={}&email={}&machine={}",
        urlencoding(&key),
        urlencoding(&email),
        urlencoding(&machine)
    );

    let result = match std::thread::spawn(move || -> Option<(bool, String)> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build().ok()?;
        let resp = client.get(&url).send().ok()?;
        let data: serde_json::Value = resp.json().ok()?;
        let valid = data.get("valid").and_then(|v| v.as_bool()).unwrap_or(false);
        let tier = data.get("tier").and_then(|v| v.as_str()).unwrap_or("core").to_string();
        Some((valid, tier))
    }).join() {
        Ok(opt) => opt,
        Err(_) => return Ok(()),
    };

    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());

    if let Some((true, tier)) = result {
        let now = chrono::Utc::now().to_rfc3339();
        db.execute("INSERT OR REPLACE INTO user_settings (key, value) VALUES ('license_status', 'valid')", []).ok();
        db.execute("INSERT OR REPLACE INTO user_settings (key, value) VALUES ('license_tier', ?1)", rusqlite::params![&tier]).ok();
        db.execute("INSERT OR REPLACE INTO user_settings (key, value) VALUES ('license_validated_at', ?1)", rusqlite::params![&now]).ok();
    }
    // background_verify NEVER revokes — it only refreshes on success.
    // Revocation is handled by the foreground validate_license command.
    Ok(())
}

fn urlencoding(s: &str) -> String {
    s.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => char::from(b).to_string(),
        _ => format!("%{:02X}", b),
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
async fn generate_with_ai(
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
    state: tauri::State<'_, AppState>,
) -> Result<Vec<TitleResult>, String> {
    // Tier gate: cloud AI is Pro/Studio only. DB lock scoped before HTTP call.
    {
        let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
        if get_tier(&db) == "core" {
            return Err(PRO_REQUIRED_MSG.to_string());
        }
    }

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
            Some(TitleResult { title, score, categories: categories.clone(), breakdown: item.get("breakdown").cloned(), source: Some("ai".to_string()), seo_score: None, seo_breakdown: None })
        })
        .collect();

    if results.is_empty() {
        return Err("AI generated no valid titles. Try a different keyword.".to_string());
    }

    Ok(results.into_iter().take(quantity as usize).collect())
}

// ── Model Download (first-launch delivery — Task 5, user decision 2026-07-31) ──
//
// The installer ships WITHOUT the 940 MB Qwen model (keeps it ~22 MB). On
// first launch the app detects Qwen is missing and offers to download it to
// $DATA_DIR/titleforge-desktop/models/qwen2.5-1.5b-instruct-q4_k_m.gguf.
// Download runs in a background thread; the frontend polls progress.
// Source: bartowski/Qwen2.5-1.5B-Instruct-GGUF (Apache 2.0 — verified).
// SHA256 pinned; file verified before the model is considered present.

const QWEN_URL: &str = "https://huggingface.co/bartowski/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf";
const QWEN_FILENAME: &str = "qwen2.5-1.5b-instruct-q4_k_m.gguf";
const QWEN_EXPECTED_SHA256: &str = "1adf0b11065d8ad2e8123ea110d1ec956dab4ab038eab665614adba04b6c3370";
const QWEN_EXPECTED_SIZE: u64 = 986_048_768;

/// Where the Qwen model lives after download.
fn qwen_model_path() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("titleforge-desktop")
        .join("models")
        .join(QWEN_FILENAME)
}

/// True when Qwen is present AND matches the pinned SHA256.
fn qwen_present() -> bool {
    let path = qwen_model_path();
    if !path.exists() { return false; }
    // Size check is a fast pre-filter; full hash verify only when close.
    let size_ok = std::fs::metadata(&path).map(|m| m.len() == QWEN_EXPECTED_SIZE).unwrap_or(false);
    if !size_ok { return false; }
    // Full SHA256 verify (one-time ~2s for 940 MB).
    let digest = sha256_file(&path);
    digest.map(|d| d == QWEN_EXPECTED_SHA256).unwrap_or(false)
}

fn sha256_file(path: &std::path::Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut ctx = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 { break; }
        ctx.update(&buf[..n]);
    }
    let digest = ctx.finalize();
    Some(digest.iter().map(|b| format!("{:02x}", b)).collect())
}

/// Global download progress: (bytes_done, bytes_total, finished_ok).
static MODEL_DOWNLOAD: std::sync::OnceLock<Mutex<(u64, u64, Option<bool>)>> = std::sync::OnceLock::new();

fn model_download_state() -> &'static Mutex<(u64, u64, Option<bool>)> {
    MODEL_DOWNLOAD.get_or_init(|| Mutex::new((0, 0, None)))
}

#[tauri::command]
fn get_model_status() -> Result<serde_json::Value, String> {
    let present = qwen_present();
    let (done, total, finished) = *model_download_state().lock().unwrap_or_else(|e| e.into_inner());
    Ok(serde_json::json!({
        "qwenPresent": present,
        "qwenSize": QWEN_EXPECTED_SIZE,
        "downloadDone": done,
        "downloadTotal": total,
        "downloadFinished": finished,
    }))
}

#[tauri::command]
fn start_model_download(app: tauri::AppHandle) -> Result<(), String> {
    // If already present or already downloading, do nothing.
    if qwen_present() { return Ok(()); }
    {
        // In-flight detection: a download in progress has total = QWEN_EXPECTED_SIZE
        // and finished = Some(false). A completed (success or fail) download
        // resets total to 0. This lets a failed download be retried.
        let (_, total, _) = *model_download_state().lock().unwrap_or_else(|e| e.into_inner());
        if total == QWEN_EXPECTED_SIZE { return Ok(()); } // a download is in flight
    }

    let target = qwen_model_path();
    std::fs::create_dir_all(target.parent().ok_or("no parent dir")?)
        .map_err(|e| format!("cannot create models dir: {}", e))?;

    // Mark download as in-flight. `finished = Some(false)` means "downloading"
    // so the UI poller can show progress; Some(true) once done,
    // Some(false) again after a failure. (JS checks downloadFinished === false.)
    *model_download_state().lock().unwrap_or_else(|e| e.into_inner()) = (0, QWEN_EXPECTED_SIZE, Some(false));

    // Spawn a background thread so the command returns immediately and the
    // UI can poll get_model_status for progress.
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .map_err(|e| format!("http client: {}", e))?;
            let mut resp = client
                .get(QWEN_URL)
                .header("User-Agent", "TitleForge-Desktop/1.0")
                .send()
                .map_err(|e| format!("download start: {}", e))?;
            if !resp.status().is_success() {
                return Err(format!("download failed: HTTP {}", resp.status()));
            }

            // Stream to a temp file then rename (atomic — a partial file never
            // looks like a complete model).
            let tmp = target.with_extension("gguf.part");
            let mut file = std::fs::File::create(&tmp).map_err(|e| format!("create temp: {}", e))?;
            let mut done: u64 = 0;
            let mut buf = [0u8; 128 * 1024];
            loop {
                use std::io::Read;
                let n = resp.read(&mut buf).map_err(|e| format!("read: {}", e))?;
                if n == 0 { break; }
                use std::io::Write;
                file.write_all(&buf[..n]).map_err(|e| format!("write: {}", e))?;
                done += n as u64;
                *model_download_state().lock().unwrap_or_else(|e| e.into_inner()) = (done, QWEN_EXPECTED_SIZE, Some(false));
            }
            drop(file);

            if done != QWEN_EXPECTED_SIZE {
                let _ = std::fs::remove_file(&tmp);
                return Err(format!("size mismatch: got {} expected {}", done, QWEN_EXPECTED_SIZE));
            }
            // SHA256 verify.
            let digest = sha256_file(&tmp).ok_or("hash failed")?;
            if digest != QWEN_EXPECTED_SHA256 {
                let _ = std::fs::remove_file(&tmp);
                return Err(format!("checksum mismatch: {}", digest));
            }
            std::fs::rename(&tmp, &target).map_err(|e| format!("finalize: {}", e))?;
            Ok(())
        })();

        *model_download_state().lock().unwrap_or_else(|e| e.into_inner()) =
            (0, 0, Some(result.is_ok()));
        let _ = app; // keep handle for future event emit if needed
        if let Err(e) = result {
            eprintln!("[model-download] FAILED: {}", e);
        }
    });

    Ok(())
}

// ── Seed Check ──

#[tauri::command]
fn get_app_info(state: tauri::State<AppState>) -> Result<serde_json::Value, String> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM patterns", [], |row| row.get(0))
        .unwrap_or(0);
    let llm_guard = state.local_llm.lock().unwrap_or_else(|e| e.into_inner());
    Ok(serde_json::json!({
        "app": "titleforge-desktop",
        "version": env!("CARGO_PKG_VERSION"),
        "seeded": count > 0,
        "templateCount": count,
        "localLlmLoaded": llm_guard.is_some(),
        "enginePresent": qwen_present(),
    }))
}

/// Lazy-load the local LLM model on first generation call.
/// Checks multiple paths so it works in dev, production, and CI.
/// Prefers Qwen2.5-1.5B (the shipped TitleForge Engine, fetched on first run).
///
/// SmolLM2 entries are kept only so a developer — or a user who manually drops a
/// GGUF into the data directory — can run an alternative model. **Neither SmolLM2
/// file is bundled in the installer any more** (removed 2026-08-01): they were
/// installed to `$INSTDIR\_up_\models\`, which is not on this search path, so
/// 272 MB shipped that could never be loaded. The installer is now minimal and
/// the engine arrives via first-run download.
fn lazy_load_llm() -> Option<local_llm::LocalLlm> {
    let model_names = vec![
        "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        "SmolLM2-360M-Instruct-Q4_K_M.gguf",
        "SmolLM2-135M-Instruct-Q4_K_M.gguf",
    ];
    let app_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("titleforge-desktop");

    for model_name in &model_names {
        let mut model_paths = vec![
            std::path::PathBuf::from("../models").join(model_name),
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("models").join(model_name)))
                .unwrap_or_default(),
            dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("com.titleforge.desktop")
                .join("models")
                .join(model_name),
            dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("titleforge-desktop")
                .join("models")
                .join(model_name),
            app_dir.join("models").join(model_name),
        ];
        model_paths.retain(|p| !p.as_os_str().is_empty());

        for p in &model_paths {
            if p.exists() {
                let llm = local_llm::LocalLlm::load(p);
                if llm.is_some() {
                    println!("[local_llm] Loaded {} from {:?}", model_name, p);
                    return llm;
                }
            }
        }
    }

    eprintln!("[local_llm] Model not found at any path. LLM inference disabled.");
    None
}

pub fn run() {
    let app_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("titleforge-desktop");
    std::fs::create_dir_all(&app_dir).ok();

    let db_path = app_dir.join("titles.db");
    let conn = db::init_db(&db_path).expect("Failed to initialize database");

    // Seed on first launch if tables are empty — check both patterns AND curated_titles
    {
        let patterns_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM patterns", [], |row| row.get(0))
            .unwrap_or(0);
        let curated_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM curated_titles", [], |row| row.get(0))
            .unwrap_or(0);
        if patterns_count == 0 || curated_count == 0 {
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

    // Build EGCG generator from curated titles (available for benchmarking as fallback)
    let generator = title_gen::Generator::build(&conn);
    println!("EGCG generator built ({} words in vocabulary)", generator.word_count());

    // Local LLM is loaded lazily on first generation call (so resource_dir is available)
    println!("Local LLM will be loaded on first use (lazy init)");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            db: Mutex::new(conn),
            generator: std::sync::Mutex::new(generator),
            local_llm: std::sync::Mutex::new(None),
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
            background_verify,
            get_model_status,
            start_model_download,
        ])
        .run(tauri::generate_context!())
        .expect("Error running TitleForge");
}
