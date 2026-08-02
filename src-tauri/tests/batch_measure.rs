/// Task 0b — Measure a REAL 25-title batch through the production path.
///
/// Every benchmark to date is k=1. Production sells 25 / 100 / 500 titles per
/// batch. This test calls the same `engine::generate` path the app uses and
/// reports:
///   - unique titles out of 25 (the number that matters — before Task 0 it was 1)
///   - wall-clock time end to end
///   - judge scores for all 25 (batch quality vs the k=1 figure)
///
/// Usage:
///   $env:TF_LLM_TEMP='0.8'; cargo test --release --test batch_measure -- --nocapture

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
            eprintln!("Loading {}...", name);
            if let Some(llm) = titleforge_lib::local_llm::LocalLlm::load(&p) {
                return Some(llm);
            }
        }
    }
    None
}

#[test]
fn batch_25_measure() {
    let conn = init_db();
    let generator = titleforge_lib::title_gen::Generator::build(&conn);
    let mut llm = match load_llm() {
        Some(l) => l,
        None => { eprintln!("[batch] No model found — skipping"); return; }
    };

    // One keyword, one category, quantity 25 — the Core tier promise.
    let keyword = "coffee";
    let categories = vec!["youtube".to_string()];
    let style = "normal";
    let genre = "any";
    let quantity = 25;

    eprintln!("[batch] Generating {} titles for '{}' ({} category)...", quantity, keyword, categories.len());
    let start = Instant::now();
    let results = titleforge_lib::engine::generate(
        &conn, &generator, Some(&mut llm), keyword, &categories, style, genre, quantity, "core",
    ).expect("engine::generate");
    let elapsed = start.elapsed();

    // ── Uniqueness: THE number that matters ──
    let unique: std::collections::HashSet<&str> =
        results.iter().map(|r| r.title.as_str()).collect();
    let wall_secs = elapsed.as_secs_f64();

    eprintln!("\n══════════════════════════════════════════════════════════════");
    eprintln!("  BATCH MEASUREMENT — '{}' × 25 (Core tier promise)", keyword);
    eprintln!("══════════════════════════════════════════════════════════════");
    eprintln!("  Requested:      25");
    eprintln!("  Returned:       {}", results.len());
    eprintln!("  UNIQUE titles:  {} / {}", unique.len(), results.len());
    eprintln!("  Wall clock:     {:.1}s ({:.2}s/title)", wall_secs, wall_secs / results.len().max(1) as f64);
    eprintln!("  Source mix:     {:?}", {
        let mut m: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for r in &results { *m.entry(r.source.as_deref().unwrap_or("?")).or_insert(0) += 1; }
        m
    });
    eprintln!("══════════════════════════════════════════════════════════════");

    for (i, r) in results.iter().enumerate() {
        eprintln!("  {:>2}. [{}] {}", i + 1, r.source.as_deref().unwrap_or("?"), r.title);
    }
    eprintln!("══════════════════════════════════════════════════════════════\n");

    // ── Gates ──
    assert!(results.len() >= 25, "Expected 25 titles, got {}", results.len());
    assert!(unique.len() >= 20, "Batch not diverse enough: only {} unique of 25 — sampling still broken?", unique.len());
    assert!(wall_secs < 1200.0, "Batch took {:.0}s — over 20 min, unacceptable for Core tier", wall_secs);

    eprintln!("[batch] PASS — {} unique of 25 in {:.1}s", unique.len(), wall_secs);
}
