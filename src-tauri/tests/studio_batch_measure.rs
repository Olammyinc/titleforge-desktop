/// Studio-tier batch measurement (mirrors batch_measure.rs Core baseline at Studio scale).
/// keyword "coffee" / category "youtube" / quantity 200 / tier "studio".
/// REPORTING harness: never fails on quality/timing. Only asserts engine returned >0.
/// No judge - distinctness is objective (judge failed calibration, S5 2026-08-05).
/// Usage: cargo test --release --test studio_batch_measure -- --nocapture
/// Expected ~23 min at ~6.8s/title. Let it run to completion.

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
            eprintln!("[studio] Loading {}...", name);
            if let Some(llm) = titleforge_lib::local_llm::LocalLlm::load(&p) {
                return Some(llm);
            }
        }
    }
    None
}

/// Opening-4-word signature: lowercased first 4 whitespace tokens joined by spaces.
/// Mirrors web generate.js near-duplicate dedup (drops titles sharing first 4 words).
fn opening_signature(title: &str) -> String {
    let norm: String = title.to_lowercase();
    let toks: Vec<&str> = norm.split_whitespace().collect();
    toks.iter().take(4).copied().collect::<Vec<&str>>().join(" ")
}

#[test]
fn studio_batch_measure() {
    let conn = init_db();
    let generator = titleforge_lib::title_gen::Generator::build(&conn);
    let mut llm = match load_llm() {
        Some(l) => l,
        None => { eprintln!("[studio] No model found - skipping"); return; }
    };

    let keyword = "coffee";
    let categories = vec!["youtube".to_string()];
    let style = "normal";
    let genre = "any";
    let quantity: u32 = 200;
    let tier = "studio";

    eprintln!("\n[studio] Generating {} titles for '{}' (category: {}) at tier '{}'...",
        quantity, keyword, categories[0], tier);
    eprintln!("[studio] (At ~6.8s/title offline this is ~23 min. Let it run.)");

    let start = Instant::now();
    let results = titleforge_lib::engine::generate(
        &conn, &generator, Some(&mut llm), keyword, &categories, style, genre, quantity, tier,
        &Default::default(),
    ).expect("engine::generate did not Err");
    let elapsed = start.elapsed();

    let unique_exact: std::collections::HashSet<&str> =
        results.iter().map(|r| r.title.as_str()).collect();
    let unique_sigs: std::collections::HashSet<String> =
        results.iter().map(|r| opening_signature(&r.title)).collect();
    let wall_secs = elapsed.as_secs_f64();
    let per_title = wall_secs / results.len().max(1) as f64;

    let mut source_mix: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for r in &results {
        *source_mix.entry(r.source.as_deref().unwrap_or("?")).or_insert(0) += 1;
    }

    eprintln!("\n==========================================================================");
    eprintln!("  STUDIO BATCH MEASUREMENT - '{}' x {} (tier: {})", keyword, quantity, tier);
    eprintln!("==========================================================================");
    eprintln!("  Requested:                   {}", quantity);
    eprintln!("  Returned:                    {}", results.len());
    eprintln!("  UNIQUE exact:                {} / {}", unique_exact.len(), results.len());
    eprintln!("  UNIQUE opening-4-word sigs:  {} / {}", unique_sigs.len(), results.len());
    eprintln!("  Wall clock:                  {:.1}s", wall_secs);
    eprintln!("  Seconds per title:           {:.2}s", per_title);
    eprintln!("  Source mix:                  {:?}", source_mix);
    eprintln!("==========================================================================");
    eprintln!("  READING (CONTEXT.md S6.5): if unique_sigs < returned by a wide margin,");
    eprintln!("  the ceiling is DISTINCT MASS per distribution - same mechanism the web");
    eprintln!("  dual-provider result showed (one provider ~70/100, two -> 100/100).");
    eprintln!("==========================================================================\n");

    assert!(!results.is_empty(), "engine returned 0 titles - engine panicked or produced nothing");
    eprintln!("[studio] PASS (reporting-only) - {} returned, {} unique exact, {} unique 4-word sigs, {:.1}s, {:.2}s/title",
        results.len(), unique_exact.len(), unique_sigs.len(), wall_secs, per_title);
}
