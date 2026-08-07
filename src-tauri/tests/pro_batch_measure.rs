//! Pro-tier batch measurement (replaces the interpolated ~7 min figure).
//! Mirrors batch_measure.rs but at the Pro tier promise (50 titles) so the
//! documented "~7 min INTERPOLATED" becomes a measured value (or a revised one).
//!
//! Evidence (rule #1 + #7): writes a run-suffixed CSV via evidence.rs
//! (PRO_RUN env -> pro-batch-runN.csv) and is meant to be run TWICE. The
//! 2026-08-06 "3.8 min Pro-50" was n=1 and an early-exit good draw; run it
//! here to get the range (measured elsewhere at 3.8-8.9 min depending on
//! whether the 2x budget is consumed -> early exit).
//!
//! Usage (run twice, rule #7):
//!   cargo test --release --test pro_batch_measure -- --nocapture          (run 1)
//!   PRO_RUN=2 cargo test --release --test pro_batch_measure -- --nocapture

use std::time::Instant;
use rusqlite::Connection;

#[path = "evidence.rs"]
mod evidence;

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

#[test]
fn pro_50_measure() {
    let run = evidence::run_tag("PRO_RUN");
    let conn = init_db();
    let generator = titleforge_lib::title_gen::Generator::build(&conn);

    let model = match titleforge_lib::local_llm::LocalLlm::find_model("qwen2.5-1.5b-instruct-q4_k_m.gguf") {
        Some(p) => p,
        None => { eprintln!("[pro] Qwen model not found — skipping"); return; }
    };
    let mut llm = match titleforge_lib::local_llm::LocalLlm::load(&model) {
        Some(m) => m,
        None => { eprintln!("[pro] Model failed to load — skipping"); return; }
    };

    let keyword = "coffee";
    let categories = vec!["youtube".to_string()];
    let quantity = 50usize;
    let tier = "pro";

    eprintln!("[pro] run {}: generating {} titles for '{}' ({} category, tier {})...", run, quantity, keyword, categories.len(), tier);
    let start = Instant::now();
    let results = titleforge_lib::engine::generate(
        &conn, &generator, Some(&mut llm), keyword, &categories, "normal", "any", quantity as u32, tier,
        &Default::default(),
    ).expect("engine::generate");
    let wall_secs = start.elapsed().as_secs_f64();

    let unique: std::collections::HashSet<&str> = results.iter().map(|r| r.title.as_str()).collect();
    let per_title = wall_secs / results.len().max(1) as f64;
    // Whether the 2x fill budget was fully consumed cannot be read directly
    // from engine output; report attempts only via wall-clock-vs-6.8s/title.
    // The distinguishing number is the wall clock and the per-title rate.

    eprintln!("\n══════════════════════════════════════════════════════════════");
    eprintln!("  PRO MEASUREMENT — '{}' × {} (Pro tier promise, tier={}), run {}", keyword, quantity, tier, run);
    eprintln!("  Requested:      {}", quantity);
    eprintln!("  Returned:       {}", results.len());
    eprintln!("  UNIQUE titles:  {} / {}", unique.len(), results.len());
    eprintln!("  Wall clock:     {:.1}s ({:.2}s/title)", wall_secs, per_title);
    eprintln!("  READING (range): one run is n=1; quote the range. At ~4.3s/attempt,");
    eprintln!("    ~226s ≈ early exit (52 attempts), ~535s ≈ full 100-attempt budget.");
    eprintln!("══════════════════════════════════════════════════════════════");

    // Evidence artifact — run-suffixed so run 1 and run 2 never collide.
    let header = "run,keyword,category,requested,returned,unique,wall_secs,per_title_secs";
    let rows = format!("{},\"{}\",\"{}\",{},{},{},{:.1},{:.2}\n",
        run, keyword, "youtube", quantity, results.len(), unique.len(), wall_secs, per_title);
    let _ = evidence::write_evidence_csv("pro-batch", "PRO_RUN", header, &rows);
    let path = evidence::evidence_path("pro-batch", &run);
    println!("\n  CSV: {}", path.display());

    assert!(results.len() >= quantity, "Pro batch returned {} — engine broken?", results.len());
    eprintln!("[pro] PASS — {} unique of {} in {:.1}s, run {}", unique.len(), results.len(), wall_secs, run);
}
