//! How many DISTINCT, USABLE titles can one model produce for one keyword?
//!
//! This answers the product question "what batch size can we honestly promise?"
//! and replaces the retracted "depth exhaustion" curve.
//!
//! WHY THE PREVIOUS ATTEMPT WAS INVALID (CONTEXT.md §5, 2026-08-05):
//!   - it read quality by RANK, but `engine.rs` sorts the pool by
//!     `calculate_score` before returning, and that score is r = -0.04 against
//!     quality. Rank was a position in a randomly-ordered list.
//!   - it was one keyword; the second keyword in the same run showed no trend.
//!   - "depth exhaustion" implies a model running out of ideas mid-batch, but
//!     every title is an INDEPENDENT call. There is no shared context to
//!     exhaust.
//!
//! WHAT IS ACTUALLY HAPPENING: repeated sampling from ONE distribution for ONE
//! keyword collides more as it goes. So the ceiling is DISTINCT MASS, not depth.
//!
//! THE MEASUREMENT: call `generate_one_clean` in a loop, in ACCEPTANCE ORDER,
//! never sorted. Two rejection buckets fall out for free and they decide the
//! product answer:
//!
//!   None returned        -> QC/drift rejected it        -> QUALITY ceiling
//!   Some, but a duplicate -> deduped                    -> DISTINCT-MASS ceiling
//!
//! If late rejections are mostly duplicates, a second provider/model raises the
//! ceiling (exactly what the web dual-provider result showed: 70 -> 100).
//! If they are mostly QC, more providers will not help.
//!
//! NO JUDGE CALL. Distinctness and fire rate are objective. Quality scoring is
//! deliberately out of scope here — the judge failed calibration for ordering,
//! and this harness must stay runnable offline and free.
//!
//! Usage:
//!   cargo test --release --test yield_curve -- --nocapture

use rusqlite::Connection;

/// (keyword, category). >=3 keywords and >=2 categories — the previous attempt
/// used 2 keywords and they disagreed with each other.
const CELLS: &[(&str, &str)] = &[
    ("coffee", "blog"),
    ("remote work", "blog"),
    ("sourdough bread", "article"),
    ("coffee", "product"),
];

/// Attempts per cell. Past any plausible ceiling without unbounded runtime.
const ATTEMPTS: usize = 40;
/// Report cumulative distinct yield at these attempt counts.
const CHECKPOINTS: &[usize] = &[5, 10, 15, 20, 25, 30, 35, 40];

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
fn yield_curve() {
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

    println!("\n╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║  DISTINCT-USABLE YIELD — how many titles can one model give per keyword? ║");
    println!("╚══════════════════════════════════════════════════════════════════════════╝");
    println!("  {} attempts per cell, acceptance order preserved, NO score sorting.\n", ATTEMPTS);

    let mut csv = String::from("keyword,category,attempt,accepted_index,outcome,title\n");
    let t0 = std::time::Instant::now();
    let mut grand_dup = 0usize;
    let mut grand_qc = 0usize;
    let mut grand_acc = 0usize;

    for (kw, cat) in CELLS {
        let spec = titleforge_lib::prompt_spec::category_spec(cat);
        // Same few-shot the production path uses — an empty slice is NOT
        // production behaviour and biases the result (see phi_smoke notes).
        let mut examples = generator.retrieve_similar(kw, cat, 4);
        if examples.is_empty() {
            examples = titleforge_lib::engine::fetch_top_appeal_fewshot(&conn, cat, 4);
        }

        // Same rotated constraints engine.rs uses, so this mirrors production.
        let constraints: &[&str] = &[
            "",
            "Make this one a question.",
            "Open this one with a specific number.",
            "Frame this one as a personal story or first-person experience.",
            "Build this one on a contrast or a reversal.",
            "Make this one short — three words or fewer.",
        ];

        let mut accepted: Vec<String> = Vec::new();
        let mut dup = 0usize;
        let mut qc = 0usize;
        let mut curve: Vec<(usize, usize)> = Vec::new();

        eprintln!("[yield] {} / {} ...", kw, cat);
        for attempt in 1..=ATTEMPTS {
            let c = if spec.is_name { "" } else { constraints[(attempt - 1) % constraints.len()] };
            let out = llm.generate_one_clean(kw, cat, "normal", "any", &examples, Some(c), &Default::default());

            let (outcome, title) = match out {
                None => { qc += 1; ("qc_reject".to_string(), String::new()) }
                Some(t) => {
                    // Same dedup rule engine.rs applies: exact match OR a shared
                    // two-word opening (shares_opening).
                    let is_dup = accepted.iter().any(|a: &String| {
                        a.eq_ignore_ascii_case(&t)
                            || titleforge_lib::engine::shares_opening(a, &t, 2)
                    });
                    if is_dup { dup += 1; ("duplicate".to_string(), t.clone()) }
                    else { accepted.push(t.clone()); ("accepted".to_string(), t.clone()) }
                }
            };
            csv.push_str(&format!("\"{}\",\"{}\",{},{},{},\"{}\"\n",
                kw, cat, attempt, accepted.len(), outcome, title.replace('"', "'")));
            if CHECKPOINTS.contains(&attempt) { curve.push((attempt, accepted.len())); }
        }

        grand_dup += dup; grand_qc += qc; grand_acc += accepted.len();

        println!("── {} / {}", kw, cat);
        print!("   cumulative distinct: ");
        for (at, n) in &curve { print!("@{}={} ", at, n); }
        println!();
        println!("   {} attempts -> {} distinct  |  {} duplicate  |  {} QC-rejected",
            ATTEMPTS, accepted.len(), dup, qc);
        // Marginal yield over the last half tells you whether it is still producing.
        let half = ATTEMPTS / 2;
        let first_half = curve.iter().find(|(a, _)| *a == half).map(|(_, n)| *n).unwrap_or(0);
        println!("   marginal: first {} attempts -> {} distinct | last {} attempts -> {} distinct",
            half, first_half, ATTEMPTS - half, accepted.len() - first_half);
        for (i, t) in accepted.iter().enumerate().take(3) { println!("      #{} {}", i + 1, t); }
        if accepted.len() > 3 {
            println!("      ... #{} {}", accepted.len(), accepted[accepted.len() - 1]);
        }
        println!();
    }

    let total = ATTEMPTS * CELLS.len();
    println!("╔══════════════════════════════════════════════════════════════════════════╗");
    println!("║  RESULT                                                                  ║");
    println!("╚══════════════════════════════════════════════════════════════════════════╝");
    println!("  attempts {}  ->  distinct {}  ({:.0}%)", total, grand_acc, 100.0 * grand_acc as f64 / total as f64);
    println!("  rejections: {} duplicate ({:.0}%) | {} QC ({:.0}%)",
        grand_dup, 100.0 * grand_dup as f64 / total as f64,
        grand_qc, 100.0 * grand_qc as f64 / total as f64);
    println!();
    println!("  READING THIS — it decides the product answer:");
    println!("    duplicates dominate -> ceiling is DISTINCT MASS.");
    println!("      A second provider/model raises it. This is what the web");
    println!("      dual-provider result showed (70 -> 100 distinct per 100).");
    println!("    QC rejects dominate -> ceiling is QUALITY.");
    println!("      More providers will NOT help; the model is the limit.");

    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("yield-curve.csv");
    let _ = std::fs::write(&out, csv);
    println!("\n  CSV: {}", out.display());
    println!("  wall clock: {:.1}s", t0.elapsed().as_secs_f64());
}
