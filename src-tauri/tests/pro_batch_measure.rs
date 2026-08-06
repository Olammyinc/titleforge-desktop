//! Pro-tier batch measurement (replaces the interpolated ~7 min figure).
//! Mirrors batch_measure.rs but at the Pro tier promise (50 titles) so the
//! documented "~7 min INTERPOLATED" becomes a measured value (or a revised one).
//!
//! Usage:
//!   cargo test --release --test pro_batch_measure -- --nocapture

use std::time::Instant;
use rusqlite::Connection;

fn init_db() -> Connection {
    let conn = Connection::open_in_memory().expect("mem");
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS curated_titles (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, category TEXT NOT NULL, genre TEXT, tone TEXT, appeal_score INTEGER, notes TEXT);
        CREATE TABLE IF NOT EXISTS patterns (id INTEGER PRIMARY KEY AUTOINCREMENT, category TEXT NOT NULL, template TEXT NOT NULL, slots TEXT NOT NULL, genre TEXT, tone TEXT, quality_score REAL DEFAULT 0.5, usage_count INTEGER DEFAULT 0);
        CREATE TABLE IF NOT EXISTS word_pools (id INTEGER PRIMARY KEY AUTOINCREMENT, pool_name TEXT NOT NULL, word TEXT NOT NULL, category TEXT, weight REAL DEFAULT 1.0);
    ").expect("schema");
    let seed = include_str!("../../seed-data.json");
    titleforge_lib::db::import_seed_from_str(&conn, seed).expect("seed");
    conn
}

fn load_llm() -> Option<titleforge_lib::local_llm::LocalLlm> {
    let models_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("models");
    for name in &["qwen2.5-1.5b-instruct-q4_k_m.gguf", "SmolLM2-360M-Instruct-Q4_K_M.gguf", "SmolLM2-135M-Instruct-Q4_K_M.gguf"] {
        let p = models_dir.join(name);
        if p.exists() {
            if let Some(llm) = titleforge_lib::local_llm::LocalLlm::load(&p) {
                return Some(llm);
            }
        }
    }
    None
}

#[test]
fn pro_50_measure() {
    let conn = init_db();
    let generator = titleforge_lib::title_gen::Generator::build(&conn);
    let mut llm = match load_llm() {
        Some(l) => l,
        None => { eprintln!("[pro] No model found — skipping"); return; }
    };

    let keyword = "coffee";
    let categories = vec!["youtube".to_string()];
    let quantity = 50;
    let tier = "pro";

    eprintln!("[pro] Generating {} titles for '{}' ({} category, tier {})...", quantity, keyword, categories.len(), tier);
    let start = Instant::now();
    let results = titleforge_lib::engine::generate(
        &conn, &generator, Some(&mut llm), keyword, &categories, "normal", "any", quantity, tier,
        &Default::default(),
    ).expect("engine::generate");
    let elapsed = start.elapsed();

    let unique: std::collections::HashSet<&str> = results.iter().map(|r| r.title.as_str()).collect();
    let wall_secs = elapsed.as_secs_f64();

    eprintln!("\n══════════════════════════════════════════════════════════════");
    eprintln!("  PRO MEASUREMENT — '{}' × 50 (Pro tier promise, tier={})", keyword, tier);
    eprintln!("  Requested:      {}", quantity);
    eprintln!("  Returned:       {}", results.len());
    eprintln!("  UNIQUE titles:  {} / {}", unique.len(), results.len());
    eprintln!("  Wall clock:     {:.1}s ({:.2}s/title)", wall_secs, wall_secs / results.len().max(1) as f64);
    eprintln!("══════════════════════════════════════════════════════════════");

    // Reporting: don't hard-assert; the point is to capture the measured time.
    assert!(results.len() >= 50, "Pro batch returned {} — engine broken?", results.len());
    eprintln!("[pro] PASS — {} unique of {} in {:.1}s", unique.len(), results.len(), wall_secs);
}
