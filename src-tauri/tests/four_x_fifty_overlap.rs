//! 4x50 overlap measurement — how many DISTINCT titles do four independent
//! Pro-50 batches produce for the same keyword/category?
//!
//! This settles the Studio tier question: if "run 50 four times" is offered as
//! the path to Studio capacity, the overlap across the four batches is the
//! number that decides whether 4x50 reaches ~200 distinct or collides heavily.
//!
//! Dedup is per-call (the HashSet in generate() is local); nothing reads
//! history across calls. So each batch resamples the same distribution from
//! scratch and the union is what matters.
//!
//! Usage:
//!   cargo test --release --test four_x_fifty_overlap -- --nocapture

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
fn four_x_fifty_overlap() {
    let conn = init_db();
    let generator = titleforge_lib::title_gen::Generator::build(&conn);
    let mut llm = match load_llm() {
        Some(l) => l,
        None => { eprintln!("[4x50] No model found — skipping"); return; }
    };

    let keyword = "coffee";
    let categories = vec!["youtube".to_string()];
    let quantity = 50;
    let tier = "pro";

    let mut union: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut per_batch: Vec<usize> = Vec::new();
    let t0 = Instant::now();

    for b in 1..=4 {
        eprintln!("[4x50] batch {} ...", b);
        let results = titleforge_lib::engine::generate(
            &conn, &generator, Some(&mut llm), keyword, &categories, "normal", "any", quantity, tier,
            &Default::default(),
        ).expect("engine::generate");
        // Union: case-insensitive, exact-match across batches (same rule the
        // engine dedups within a call).
        let before = union.len();
        for r in &results { union.insert(r.title.to_ascii_lowercase()); }
        let added = union.len() - before;
        per_batch.push(added);
        eprintln!("  batch {}: {} titles, {} NEW into union (union now {})", b, results.len(), added, union.len());
    }

    let wall = t0.elapsed().as_secs_f64();
    println!("\n══════════════════════════════════════════════════════════════");
    println!("  4x50 OVERLAP — '{}' × 4 batches of 50 (tier {})", keyword, tier);
    println!("  per-batch NEW-into-union: {:?}", per_batch);
    println!("  sum of 4 batches:        200");
    println!("  UNION distinct:          {}", union.len());
    println!("  distinct yield:          {:.0}% of 200", 100.0 * union.len() as f64 / 200.0);
    println!("  wall clock:              {:.1}s", wall);
    println!("══════════════════════════════════════════════════════════════");
    println!("  READING: overlap = 200 - union. If overlap is large, four");
    println!("  50-batches do NOT reach Studio capacity (~200 distinct); the");
    println!("  tier cap is genuinely what bounds distinct titles delivered.");
    eprintln!("[4x50] done — union {} of 200 in {:.1}s", union.len(), wall);
}
