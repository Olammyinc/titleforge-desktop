//! Studio-tier batch measurement (post-D1 re-take).
//!
//! Measures the offline Qwen engine's DISTINCT-USABLE YIELD at the Studio tier
//! promise (200 titles for one keyword/category), under the post-D1 FILL model
//! (flat 2x attempt budget => 400 attempts for a 200-title request).
//!
//! WHY THIS RE-TAKE (audit, HANDOFF-DESKTOP.md §7 / CONTEXT.md §5 2026-08-06):
//!   - the original `5940dd2` measured "124/200 in 26.2 min" but wrote no CSV
//!     and logged no rejection outcomes, so the number was unreproducible and
//!     the stated cause (distinct-mass ceiling) was never measured.
//!   - "124/124 distinct exact" cannot fail: `engine.rs:158-161` rejects exact
//!     matches and `shares_opening(n=2)` before anything enters the pool, so
//!     exact-uniqueness is guaranteed by the engine. The metric that CAN show the
//!     product defect is YIELD (delivered ÷ requested).
//!
//! DESIGN:
//!   - Drives `generate_one_clean` per attempt in ACCEPTANCE ORDER (never
//!     score-sorted), exactly like `yield_curve.rs`.
//!   - Classifies each attempt: `accepted` / `duplicate` / `qc_fail`, using the
//!     SAME rule the engine dedups on: exact match OR `engine::shares_opening(n=2)`
//!     (faithful to `engine.rs:158-161`, not a reimplementation).
//!   - Writes a per-attempt CSV (`studio-batch-runN.csv`) with an outcome column
//!     so the duplicate:QC split is reproducible.
//!   - Headline = YIELD (delivered ÷ requested), not exact-uniqueness.
//!
//! NOTE (local_llm.rs:288-291): `generate_one_clean` returns the first
//! soft-rejected candidate if the 3-attempt budget runs out, so `qc_fail`
//! UNDERCOUNTS by design — a soft QC failure rarely surfaces as None.
//!
//! Usage:
//!   cargo test --release --test studio_batch_measure -- --nocapture

use rusqlite::Connection;

#[path = "evidence.rs"]
mod evidence;

/// One Studio cell: 200 requested titles for coffee/youtube.
const KEYWORD: &str = "coffee";
const CATEGORY: &str = "youtube";
/// Target delivered titles (the Studio tier promise).
const TARGET: usize = 200;
/// Post-D1 flat 2x fill budget: 400 attempts for a 200-title request.
const ATTEMPTS: usize = 400;
/// Report cumulative distinct yield at these attempt counts.
const CHECKPOINTS: &[usize] = &[50, 100, 150, 200, 250, 300, 350, 400];

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
fn studio_batch_measure() {
    let conn = init_db();
    let generator = titleforge_lib::title_gen::Generator::build(&conn);

    let model = match titleforge_lib::local_llm::LocalLlm::find_model("qwen2.5-1.5b-instruct-q4_k_m.gguf") {
        Some(p) => p,
        None => { eprintln!("Qwen model not found — skipping."); return; }
    };
    let mut llm = match titleforge_lib::local_llm::LocalLlm::load(&model) {
        Some(m) => m,
        None => { eprintln!("Model failed to load — skipping."); return; }
    };

    let spec = titleforge_lib::prompt_spec::category_spec(CATEGORY);
    // Same few-shot the production path uses.
    let mut examples = generator.retrieve_similar(KEYWORD, CATEGORY, 4);
    if examples.is_empty() {
        examples = titleforge_lib::engine::fetch_top_appeal_fewshot(&conn, CATEGORY, 4);
    }
    // Same rotated constraints engine.rs uses.
    let constraints: &[&str] = &[
        "",
        "Make this one a question.",
        "Open this one with a specific number.",
        "Frame this one as a personal story or first-person experience.",
        "Build this one on a contrast or a reversal.",
        "Make this one short — three words or fewer.",
    ];

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  STUDIO YIELD — {} titles requested for '{}' / {} (post-D1, {} attempts) ║", TARGET, KEYWORD, CATEGORY, ATTEMPTS);
    println!("╚══════════════════════════════════════════════════════════════════════╝");

    let mut csv = String::from("keyword,category,attempt,accepted_index,outcome,title\n");
    let mut accepted: Vec<String> = Vec::new();
    let mut dup = 0usize;
    let mut qc = 0usize;
    let mut curve: Vec<(usize, usize)> = Vec::new();
    let t0 = std::time::Instant::now();

    for attempt in 1..=ATTEMPTS {
        let c = if spec.is_name { "" } else { constraints[(attempt - 1) % constraints.len()] };
        eprintln!("[studio] attempt {}/{} (accepted {}) ...", attempt, ATTEMPTS, accepted.len());
        let out = llm.generate_one_clean(KEYWORD, CATEGORY, "normal", "any", &examples, Some(c), &Default::default());

        let (outcome, title) = match out {
            None => { qc += 1; ("qc_fail".to_string(), String::new()) }
            Some(t) => {
                // Same dedup rule engine.rs:158-161 applies: exact match OR a
                // shared two-word opening. Use `engine::is_duplicate` — it IS
                // the production rule; a reimplementation is a measurement bug.
                let is_dup = accepted.iter().any(|a: &String| titleforge_lib::engine::is_duplicate(a, &t));
                if is_dup { dup += 1; ("duplicate".to_string(), t.clone()) }
                else { accepted.push(t.clone()); ("accepted".to_string(), t.clone()) }
            }
        };
        csv.push_str(&format!("\"{}\",\"{}\",{},{},{},\"{}\"\n",
            KEYWORD, CATEGORY, attempt, accepted.len(), outcome, title.replace('"', "'")));
        if CHECKPOINTS.contains(&attempt) { curve.push((attempt, accepted.len())); }
        // Early exit mirror of D1: stop once we have enough to return TARGET.
        if accepted.len() >= TARGET { break; }
    }

    let wall = t0.elapsed().as_secs_f64();
    let attempts_used = curve.last().map(|(a, _)| *a).unwrap_or(attempts_run(&csv));
    // Recompute actual attempts used (early exit may be < ATTEMPTS).
    let attempts_used = ATTEMPTS.min(attempts_used.max(accepted.len() + dup + qc));

    // Headline: YIELD = delivered / requested.
    let delivered = accepted.len().min(TARGET);
    let yield_pct = 100.0 * delivered as f64 / TARGET as f64;

    println!();
    println!("── {} / {}", KEYWORD, CATEGORY);
    print!("   cumulative distinct: ");
    for (at, n) in &curve { print!("@{}={} ", at, n); }
    println!();
    println!("   requested    {}", TARGET);
    println!("   attempts     {}", attempts_used);
    println!("   delivered    {}  (YIELD {:.0}%)", delivered, yield_pct);
    println!("   drop to fill {}", TARGET.saturating_sub(delivered));
    println!("   rejections   {} duplicate | {} QC-fail", dup, qc);
    println!("   wall clock   {:.0}s ({:.2}s/attempt)", wall, wall / attempts_used.max(1) as f64);

    println!();
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  RESULT — decides the Studio cap                                ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!("  YIELD ({:.0}%) is the headline metric.", yield_pct);
    println!("  duplicates dominate -> ceiling is DISTINCT-MASS; a second distribution helps.");
    println!("  QC-fail dominates   -> ceiling is QUALITY; the model is the limit.");
    println!("  NOTE: qc_fail undercounts by design (local_llm.rs:288-291 soft-returns).");

    // Evidence via evidence.rs — run-suffixed (STUDIO_RUN=1/2), so two re-takes
    // never clobber each other (the audit's `studio-batch-run1/2.csv`). `csv`
    // already carries the header + all rows; hand it over whole.
    let _ = evidence::write_evidence_csv("studio-batch", "STUDIO_RUN", &csv, "");
    let out = evidence::evidence_path("studio-batch", &evidence::run_tag("STUDIO_RUN"));
    println!("\n  CSV: {}", out.display());
    println!("  wall clock: {:.0}s", wall);

    // Reporting harness — never fails on measured outcome; only guards against
    // a completely broken engine (0 delivered).
    assert!(delivered > 0, "Studio batch returned 0 — engine broken?");
}

/// Best-effort count of rows written to the CSV (used to infer attempts used).
fn attempts_run(csv: &str) -> usize {
    csv.lines().skip(1).count()
}
