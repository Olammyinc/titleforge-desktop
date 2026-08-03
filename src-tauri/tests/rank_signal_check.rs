/// Offline check: does ANY local scoring signal predict judge quality?
///
/// Best-of-N (Task 2) sorts the candidate pool by `calculate_score`. Measured
/// 2026-08-02 on a real 50-title batch, that function correlates r = -0.04 with
/// judge score and has stdev 4.6 in an 80-100 band — it cannot discriminate, so
/// the 4x generation cost buys a random reordering.
///
/// This scores already-judged titles with the OTHER available local signal
/// (`seo::score_seo`) so the correlation can be computed without regenerating
/// anything. If SEO correlates, Task 2 is salvageable by swapping the sort key.
/// If it doesn't, no local signal ranks and Task 2 should drop to 1x.
///
/// Reads  ../bench-batch-constraints.csv
/// Writes ../rank-signal-check.csv
///
/// Usage: cargo test --release rank_signal_check -- --nocapture

use std::path::Path;
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

/// Minimal CSV field splitter — handles the quoted fields this file writes.
fn split_csv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut inq = false;
    for c in line.chars() {
        match c {
            '"' => inq = !inq,
            ',' if !inq => { out.push(cur.clone()); cur.clear(); }
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

#[test]
fn rank_signal_check() {
    let in_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("bench-batch-constraints.csv");
    let data = match std::fs::read_to_string(&in_path) {
        Ok(d) => d,
        Err(_) => { eprintln!("No batch CSV at {:?} — run bench_batch_quality first.", in_path); return; }
    };

    let conn = init_db();
    // Build the SEO scorer over the same categories the batch used.
    let cats: Vec<String> = vec!["product".into(), "blog".into()];
    let mut curated: Vec<String> = Vec::new();
    for c in &cats {
        let mut st = conn.prepare("SELECT title FROM curated_titles WHERE category = ?1 LIMIT 400").unwrap();
        let rows = st.query_map([c], |r| r.get::<_, String>(0)).unwrap();
        curated.extend(rows.filter_map(|r| r.ok()));
    }
    let scorer = titleforge_lib::seo::SeoScorer::from_curated(&curated);

    let mut out = String::from("keyword,category,title,judge_score,seo_score\n");
    let mut n = 0usize;

    for (i, line) in data.lines().enumerate() {
        if i == 0 || line.trim().is_empty() { continue; }
        let f = split_csv(line);
        if f.len() < 7 { continue; }
        let (kw, cat, title, judge) = (&f[0], &f[1], &f[3], &f[5]);
        let judge: u32 = judge.trim().parse().unwrap_or(0);
        if judge == 0 || title.trim().is_empty() { continue; }

        let platform = titleforge_lib::seo::platform_for_category(cat);
        let (seo_score, _) = scorer.score_seo(title, kw, cat, platform);

        out.push_str(&format!("\"{}\",\"{}\",\"{}\",{},{}\n",
            kw, cat, title.replace('"', "'"), judge, seo_score));
        n += 1;
    }

    let out_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("rank-signal-check.csv");
    std::fs::write(&out_path, &out).expect("write");
    println!("Scored {} titles with seo::score_seo -> {:?}", n, out_path);
}
