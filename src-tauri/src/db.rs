use rusqlite::{Connection, Result};
use serde_json;

pub fn init_db(path: &std::path::Path) -> Result<Connection> {
    let conn = Connection::open(path)?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS patterns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            category TEXT NOT NULL,
            template TEXT NOT NULL,
            slots TEXT NOT NULL,
            genre TEXT,
            tone TEXT,
            quality_score REAL DEFAULT 0.5,
            usage_count INTEGER DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS word_pools (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pool_name TEXT NOT NULL,
            word TEXT NOT NULL,
            category TEXT,
            weight REAL DEFAULT 1.0
        );

        CREATE TABLE IF NOT EXISTS curated_titles (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            category TEXT NOT NULL,
            genre TEXT,
            tone TEXT,
            appeal_score INTEGER,
            notes TEXT
        );

        CREATE TABLE IF NOT EXISTS user_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            keyword TEXT NOT NULL,
            categories TEXT NOT NULL,
            genre TEXT,
            style TEXT,
            titles TEXT NOT NULL,
            created_at TEXT DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS user_favorites (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            keyword TEXT,
            score INTEGER,
            category TEXT,
            created_at TEXT DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS user_settings (
            key TEXT PRIMARY KEY,
            value TEXT
        );

        CREATE TABLE IF NOT EXISTS user_projects (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            created_at TEXT DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS project_titles (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            title TEXT NOT NULL,
            keyword TEXT,
            score INTEGER,
            notes TEXT,
            FOREIGN KEY (project_id) REFERENCES user_projects(id) ON DELETE CASCADE
        );"
    )?;

    Ok(conn)
}

pub fn import_seed(conn: &Connection, seed_path: &std::path::Path) -> Result<()> {
    let content = std::fs::read_to_string(seed_path)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    let data: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

    // Import templates
    if let Some(templates) = data["templates"].as_object() {
        for (cat, templates) in templates {
            if let Some(arr) = templates.as_array() {
                for t in arr {
                    conn.execute(
                        "INSERT OR IGNORE INTO patterns (category, template, slots, genre, tone, quality_score) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        rusqlite::params![
                            cat,
                            t["template"].as_str().unwrap_or(""),
                            serde_json::to_string(&t["slots"]).unwrap_or_default(),
                            t["genre"].as_str().unwrap_or("any"),
                            t["tone"].as_str().unwrap_or("normal"),
                            t["quality_score"].as_f64().unwrap_or(0.5),
                        ],
                    ).ok();
                }
            }
        }
    }

    // Import word pools
    if let Some(pools) = data["word_pools"].as_object() {
        for (pool_name, words) in pools {
            if let Some(arr) = words.as_array() {
                for w in arr {
                    conn.execute(
                        "INSERT OR IGNORE INTO word_pools (pool_name, word) VALUES (?1, ?2)",
                        rusqlite::params![pool_name, w.as_str().unwrap_or("")],
                    ).ok();
                }
            }
        }
    }

    // Import curated titles
    if let Some(curated) = data["curated_titles"].as_object() {
        for (cat, titles) in curated {
            if let Some(arr) = titles.as_array() {
                for t in arr {
                    conn.execute(
                        "INSERT OR IGNORE INTO curated_titles (title, category, genre, tone, appeal_score, notes) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        rusqlite::params![
                            t["title"].as_str().unwrap_or(""),
                            cat,
                            t["genre"].as_str().unwrap_or("any"),
                            t["tone"].as_str().unwrap_or("normal"),
                            t["appeal_score"].as_i64().unwrap_or(50),
                            t["notes"].as_str().unwrap_or(""),
                        ],
                    ).ok();
                }
            }
        }
    }

    Ok(())
}
