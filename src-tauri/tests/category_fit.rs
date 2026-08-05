//! Does the offline engine actually respect the CATEGORY?
//!
//! The user reported (2026-08-03) that product titles read like blog titles and
//! that every category shared one tone. Measured in the cloud benchmark data:
//! mean word count varied by under ONE word across five categories, 0% of
//! titles were questions in any category, and 100% of "product" results were
//! blog headlines rather than product names.
//!
//! This harness measures the same thing on the LOCAL engine, structurally.
//! Deliberately NO judge API call: category fit is objective (a product name is
//! or is not one word without digits), unlike "quality", and the DeepSeek judge
//! failed calibration against the user on 2026-08-03 — agreement 51.6% in the
//! usable band. A metric that does not depend on it is worth more here.
//!
//! THE HEADLINE NUMBER is cross-category spread in mean word count. Before the
//! fix that was under 1.0 word (collapse). If it stays under 1.0, the change
//! did not work, whatever the individual titles look like.
//!
//! Usage:
//!   cargo test --release --test category_fit -- --nocapture
//! Model: found via LocalLlm::find_model (repo ../models, OS data dir, or
//! TF_MODEL_PATH).

use rusqlite::Connection;
use std::collections::BTreeMap;

/// (keyword, category). Spread across title categories AND name categories so
/// the two rubrics are both exercised. Kept small — each title is ~7-12s.
const CASES: &[(&str, &str)] = &[
    ("coffee", "product"),
    ("coffee", "song"),
    ("coffee", "blog"),
    ("remote work", "youtube"),
    ("remote work", "book"),
    ("sourdough bread", "product"),
    ("sourdough bread", "poem"),
];
// Evaluation protocol: the Phi migration brief requires >=8 per case so a
// single sampled title cannot decide category fit.
const PER_CASE: u32 = 8;

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

#[derive(Default)]
struct CatStats {
    titles: Vec<String>,
    llm_titles: usize,
}

#[test]
fn category_fit() {
    let conn = init_db();
    let generator = titleforge_lib::title_gen::Generator::build(&conn);

    let model = match titleforge_lib::local_llm::LocalLlm::find_model("qwen2.5-1.5b-instruct-q4_k_m.gguf") {
        Some(p) => p,
        None => { eprintln!("Qwen model not found — skipping."); return; }
    };
    eprintln!("[category_fit] model: {}", model.display());
    let mut llm = match titleforge_lib::local_llm::LocalLlm::load(&model) {
        Some(m) => m,
        None => { eprintln!("Model failed to load — skipping."); return; }
    };

    let mut by_cat: BTreeMap<String, CatStats> = BTreeMap::new();
    let t0 = std::time::Instant::now();

    for (kw, cat) in CASES {
        eprintln!("[category_fit] {} x {} for '{}' ...", PER_CASE, cat, kw);
        let cats = vec![cat.to_string()];
        let results = titleforge_lib::engine::generate(
            &conn, &generator, Some(&mut llm), kw, &cats, "normal", "any", PER_CASE, "core",
            &Default::default(),
        ).unwrap_or_default();

        let e = by_cat.entry(cat.to_string()).or_default();
        for r in &results {
            // Only LLM output measures the prompt change; curated fallback is
            // retrieved verbatim from the corpus and would mask the signal.
            if r.source.as_deref() == Some("local-llm") {
                e.llm_titles += 1;
                e.titles.push(r.title.clone());
            }
        }
    }

    let mean = |v: &[f64]| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
    let pct = |n: usize, d: usize| if d == 0 { 0.0 } else { 100.0 * n as f64 / d as f64 };

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  CATEGORY FIT — offline engine (Qwen2.5-1.5B)                        ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");

    let mut csv = String::from("category,title,words,has_digit,has_colon,in_word_band,shape_ok\n");
    let mut cat_mean_words: Vec<f64> = Vec::new();
    let mut total_in_band = 0usize;
    let mut total_titles = 0usize;

    for (cat, st) in &by_cat {
        let spec = titleforge_lib::prompt_spec::category_spec(cat);
        let words: Vec<f64> = st.titles.iter().map(|t| t.split_whitespace().count() as f64).collect();
        let digits = st.titles.iter().filter(|t| t.chars().any(|c| c.is_ascii_digit())).count();
        let colons = st.titles.iter().filter(|t| t.contains(':')).count();
        let in_band = st.titles.iter()
            .filter(|t| {
                let w = t.split_whitespace().count();
                w >= spec.words.0 && w <= spec.words.1
            }).count();
        let shape_ok = st.titles.iter()
            .filter(|t| titleforge_lib::prompt_spec::passes_name_shape(t, &spec)).count();

        total_in_band += in_band;
        total_titles += st.titles.len();
        if !words.is_empty() { cat_mean_words.push(mean(&words)); }

        println!("\n── {} ({}{}) — target {}-{} words, n={}",
            cat, spec.label, if spec.is_name { ", NAME" } else { "" },
            spec.words.0, spec.words.1, st.titles.len());
        println!("   mean words {:.2} | digits {:.0}% | colons {:.0}% | IN WORD BAND {:.0}% | shape ok {:.0}%",
            mean(&words), pct(digits, st.titles.len()), pct(colons, st.titles.len()),
            pct(in_band, st.titles.len()), pct(shape_ok, st.titles.len()));
        for t in &st.titles {
            println!("      {:>2}w | {}", t.split_whitespace().count(), t);
            csv.push_str(&format!("{},\"{}\",{},{},{},{},{}\n",
                cat, t.replace('"', "'"), t.split_whitespace().count(),
                t.chars().any(|c| c.is_ascii_digit()) as u8,
                t.contains(':') as u8,
                { let w = t.split_whitespace().count(); (w >= spec.words.0 && w <= spec.words.1) as u8 },
                titleforge_lib::prompt_spec::passes_name_shape(t, &spec) as u8));
        }
    }

    // ── THE HEADLINE ──
    // Cross-category spread in mean word count. Cloud measured 0.96 words
    // across five categories BEFORE the fix — that is what collapse looks like.
    let overall_mean = mean(&cat_mean_words);
    let spread = if cat_mean_words.len() < 2 { 0.0 } else {
        let var = cat_mean_words.iter().map(|x| (x - overall_mean).powi(2)).sum::<f64>()
            / cat_mean_words.len() as f64;
        var.sqrt()
    };
    let range = if cat_mean_words.is_empty() { 0.0 } else {
        cat_mean_words.iter().cloned().fold(f64::MIN, f64::max)
            - cat_mean_words.iter().cloned().fold(f64::MAX, f64::min)
    };

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  RESULT                                                              ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    // FIRE RATE. Adding rejection reasons to a fixed 3-attempt budget is how
    // the cliche filter dropped output 50 -> 34 on 2026-08-02. Colon/digit
    // bans and the exemplar-echo guard are more rejection reasons, so this
    // number has to be watched, not assumed.
    let requested = CASES.len() * PER_CASE as usize;
    println!("  categories measured        : {}", cat_mean_words.len());
    println!("  LLM titles measured        : {} / {} requested ({:.0}% FIRE RATE)",
        total_titles, requested, pct(total_titles, requested));
    println!("  cross-category stdev (words): {:.2}", spread);
    println!("  cross-category range (words): {:.2}   <- cloud was 2.65 (8.10..10.75) pre-fix",
        range);
    println!("  titles inside their word band: {:.0}%", pct(total_in_band, total_titles));
    println!("\n  Collapse baseline: mean-length RANGE across categories was 2.65 words");
    println!("  and stdev 0.96 in the pre-fix cloud data. A range that stays near");
    println!("  zero means category is still not binding.");

    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("category-fit.csv");
    let _ = std::fs::write(&out, csv);
    println!("\n  CSV: {}", out.display());
    println!("  wall clock: {:.1}s", t0.elapsed().as_secs_f64());
}
