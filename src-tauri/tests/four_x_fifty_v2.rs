//! 4x50 OVERLAP re-take (U3) — how many DISTINCT titles do four independent
//! Pro-50 batches deliver for the same keyword/category?
//!
//! THIS SUPERSEDES the original `four_x_fifty_overlap` measurement (2026-08-06),
//! which unioned on `to_ascii_lowercase()` — EXACT MATCH ONLY. The engine
//! dedups on exact match OR a shared two-word opening (`engine.rs::shares_opening`,
//! n=2, with the function-word filter). The original counted titles as distinct
//! that the engine itself would reject as near-duplicates, so its "198/200 (99%)"
//! understated cross-batch overlap. This harness applies the ENGINE'S OWN rule.
//!
//! Standing rule: "Measure with the engine's own rule, by calling it."
//! `engine::shares_opening` is `pub` for exactly this reason. We call it directly.
//!
//! Method (per §7 U3):
//!   - Call `engine::generate` for 4 independent Pro-50 batches on the same
//!     keyword/category (the production path, one hash per call, nothing reads
//!     history across calls).
//!   - Union the 200 titles under BOTH rules and report both side by side:
//!       exact union  : case-insensitive exact match (reproduces the old result)
//!       engine union : exact match OR shares_opening(_, 2), order-preserving
//!                      -- the same rule the engine uses within a call.
//!   - Write a per-attempt CSV with an `outcome` column (evidence artifact,
//!     rule #1 -- a measurement that only prints to stdout has not been taken).
//!   - Run TWICE (rule #7): pass TF_4X50_RUN=1 / =2 to name the output CSV.
//!
//! Usage:
//!   cargo test --release --test four_x_fifty_v2 -- --nocapture   (defaults run 1)
//!   TF_4X50_RUN=2 cargo test --release --test four_x_fifty_v2 -- --nocapture

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

#[test]
fn four_x_fifty_v2() {
    let run_tag = std::env::var("TF_4X50_RUN").unwrap_or_else(|_| "1".to_string());

    let conn = init_db();
    let generator = titleforge_lib::title_gen::Generator::build(&conn);

    let model = match titleforge_lib::local_llm::LocalLlm::find_model("qwen2.5-1.5b-instruct-q4_k_m.gguf") {
        Some(p) => p,
        None => { eprintln!("[4x50v2] Qwen model not found -- skipping."); return; }
    };
    let mut llm = match titleforge_lib::local_llm::LocalLlm::load(&model) {
        Some(m) => m,
        None => { eprintln!("[4x50v2] Model failed to load -- skipping."); return; }
    };

    let keyword = "coffee";
    let categories = vec!["youtube".to_string()];
    let quantity = 50;
    let tier = "pro";

    // Two unions side by side.
    // exact_union  : case-insensitive EXACT match only (reproduces the old result)
    // engine_seen  : order-preserving, applies the engine's dedup rule ACROSS batches:
    //                a title is admitted iff it shares no exact text AND no 2-word
    //                opening with any already-admitted title (mirrors engine.rs 158-161).
    let mut exact_union: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut engine_seen: Vec<String> = Vec::new();

    let mut exact_per_batch: Vec<usize> = Vec::new();
    let mut engine_per_batch: Vec<usize> = Vec::new();

    // Number of batches: default 4 (the full 4x50 re-take). Set TF_4X50_BATCHES
    // to something small (e.g. 1) to smoke-test the harness cheaply before a full
    // unattended run -- rule #1: get the artifact before you trust the runtime.
    let batches: usize = std::env::var("TF_4X50_BATCHES").ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);

    // Evidence is flushed to disk after EVERY batch, so an interrupted run still
    // leaves a per-batch artifact (rule #1 -- stdout-only is not a measurement).
    let out_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(format!("four-x-fifty-run{}.csv", run_tag));

    let mut csv = String::from("run,batch,title,exact_new,engine_new,exact_union,engine_union\n");
    let _ = std::fs::write(&out_path, csv.clone());
    let t0 = Instant::now();

    for b in 1..=batches {
        eprintln!("[4x50v2] run {} batch {} ...", run_tag, b);
        let results = titleforge_lib::engine::generate(
            &conn, &generator, Some(&mut llm), keyword, &categories, "normal", "any", quantity, tier,
            &Default::default(),
        ).expect("engine::generate");

        let mut exact_added = 0usize;
        let mut engine_added = 0usize;

        for r in &results {
            // Rule A: exact, case-insensitive.
            let exact_new = exact_union.insert(r.title.to_ascii_lowercase());
            if exact_new { exact_added += 1; }

            // Rule B: the ENGINE's rule, called directly. A title is new iff it is
            // not exact-equal AND does not share a 2-word opening with any title
            // already admitted into the engine union.
            let is_engine_dup = engine_seen.iter().any(|s: &String| {
                s.eq_ignore_ascii_case(&r.title)
                    || titleforge_lib::engine::shares_opening(s, &r.title, 2)
            });
            let engine_new = !is_engine_dup;
            if engine_new { engine_seen.push(r.title.clone()); engine_added += 1; }

            csv.push_str(&format!("{},{},\"{}\",{},{},{},{}\n",
                run_tag, b, r.title.replace('"', "'"),
                if exact_new { 1 } else { 0 },
                if engine_new { 1 } else { 0 },
                exact_union.len(), engine_seen.len()));
        }

        exact_per_batch.push(exact_added);
        engine_per_batch.push(engine_added);
        eprintln!("[4x50v2] batch {b}: {} titles | +{} exact / +{} engine (exact union {} | engine union {})",
            results.len(), exact_added, engine_added, exact_union.len(), engine_seen.len());

        // Flush evidence after each batch so an interruption never loses the run.
        let _ = std::fs::write(&out_path, csv.clone());
    }

    let wall = t0.elapsed().as_secs_f64();
    let total = batches * quantity as usize;
    println!("\n╔═════════════════════════════════════════════════════════════════════╗");
    println!("║  4x50 OVERLAP RE-TAKE -- '{}' × {} Pro-{} ({} youtube), run {} ║", keyword, batches, quantity, tier, run_tag);
    println!("╚═════════════════════════════════════════════════════════════════════╝");
    println!("  sum of {} batches:            {}", batches, total);
    println!("  exact union (case-insensit.): {}  ({:.0}% of {})", exact_union.len(), 100.0 * exact_union.len() as f64 / total as f64, total);
    println!("  ENGINE union (exact|opening): {}  ({:.0}% of {})", engine_seen.len(), 100.0 * engine_seen.len() as f64 / total as f64, total);
    println!("  exact per-batch new:         {:?}", exact_per_batch);
    println!("  engine per-batch new:        {:?}", engine_per_batch);
    println!("  wall clock:                  {:.1}s", wall);
    println!("  READING: overlap = {} - union.", total);
    println!("    exact union reproduces the old ~99% -- that rule is the bug.");
    println!("    ENGINE union is the honest number: if it is well below 200,");
    println!("    four 50-batches do NOT reach Studio capacity; the tier cap is");
    println!("    what bounds distinct titles delivered across calls.");

    let _ = std::fs::write(&out_path, csv);
    println!("\n  CSV: {}", out_path.display());
    eprintln!("[4x50v2] done -- exact {} / engine {} of {} in {:.1}s", exact_union.len(), engine_seen.len(), total, wall);
}
