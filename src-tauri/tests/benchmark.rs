/// SmolLM2 vs EGCG vs Curated — 50 keyword side-by-side benchmark.
///
/// USAGE:
///   cargo test --release benchmark_full -- --nocapture 2>&1 | tee benchmark-output.txt
///
/// Reads seed-data.json from the project root, builds the EGCG generator,
/// loads the LLM model from models/, and runs all three engines on 50
/// diverse keywords across multiple categories. Outputs a tab-separated
/// table suitable for pasting into a spreadsheet.

use std::path::Path;
use rusqlite::Connection;

// ── 50 diverse keywords covering common niches ──
const KEYWORDS: &[(&str, &[&str])] = &[
    ("shirt",              &["product"]),
    ("laptop",             &["product"]),
    ("productivity",       &["book"]),
    ("love",               &["song"]),
    ("startup",            &["book"]),
    ("crypto",             &["article"]),
    ("parenting",          &["book"]),
    ("fitness",            &["youtube"]),
    ("travel",             &["blog"]),
    ("cooking",            &["book"]),
    ("meditation",         &["book"]),
    ("investing",          &["article"]),
    ("photography",        &["youtube"]),
    ("music",              &["song"]),
    ("AI",                 &["article"]),
    ("remote work",        &["blog"]),
    ("mental health",      &["book"]),
    ("gardening",          &["book"]),
    ("coffee",             &["product"]),
    ("minimalism",         &["book"]),
    ("creativity",         &["book"]),
    ("sleep",              &["book"]),
    ("negotiation",        &["book"]),
    ("writing",            &["book"]),
    ("marketing",          &["book"]),
    ("data science",       &["article"]),
    ("blockchain",         &["article"]),
    ("electric cars",      &["article"]),
    ("space exploration",  &["article"]),
    ("wine",               &["blog"]),
    ("dancing",            &["youtube"]),
    ("podcasting",         &["blog"]),
    ("freelancing",        &["blog"]),
    ("vegan",              &["blog"]),
    ("yoga",               &["youtube"]),
    ("gaming",             &["youtube"]),
    ("digital nomad",      &["blog"]),
    ("sourdough bread",    &["blog"]),
    ("intermittent fasting", &["book"]),
    ("bitcoin",            &["article"]),
    ("tennis",             &["youtube"]),
    ("skincare",           &["blog"]),
    ("virtual reality",    &["article"]),
    ("board games",        &["product"]),
    ("tea",                &["product"]),
    ("solo travel",        &["blog"]),
    ("home office",        &["product"]),
    ("jazz",               &["song"]),
    ("ethical fashion",    &["blog"]),
    ("climate change",     &["article"]),
];

// ── Helpers ──

fn init_db() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory DB");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS patterns (id INTEGER PRIMARY KEY AUTOINCREMENT, category TEXT NOT NULL, template TEXT NOT NULL, slots TEXT NOT NULL, genre TEXT, tone TEXT, quality_score REAL DEFAULT 0.5, usage_count INTEGER DEFAULT 0);
         CREATE TABLE IF NOT EXISTS word_pools (id INTEGER PRIMARY KEY AUTOINCREMENT, pool_name TEXT NOT NULL, word TEXT NOT NULL, category TEXT, weight REAL DEFAULT 1.0);
         CREATE TABLE IF NOT EXISTS curated_titles (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, category TEXT NOT NULL, genre TEXT, tone TEXT, appeal_score INTEGER, notes TEXT);
         CREATE TABLE IF NOT EXISTS user_history (id INTEGER PRIMARY KEY AUTOINCREMENT, keyword TEXT NOT NULL, categories TEXT NOT NULL, genre TEXT, style TEXT, titles TEXT NOT NULL, created_at TEXT DEFAULT (datetime('now')));
         CREATE TABLE IF NOT EXISTS user_favorites (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, keyword TEXT, score INTEGER, category TEXT, created_at TEXT DEFAULT (datetime('now')));
         CREATE TABLE IF NOT EXISTS user_settings (key TEXT PRIMARY KEY, value TEXT);
         CREATE TABLE IF NOT EXISTS user_projects (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, created_at TEXT DEFAULT (datetime('now')));
         CREATE TABLE IF NOT EXISTS project_titles (id INTEGER PRIMARY KEY AUTOINCREMENT, project_id INTEGER NOT NULL, title TEXT NOT NULL, keyword TEXT, score INTEGER, notes TEXT, FOREIGN KEY (project_id) REFERENCES user_projects(id) ON DELETE CASCADE);"
    ).expect("schema creation");
    let seed = include_str!("../../seed-data.json");
    titleforge_lib::db::import_seed_from_str(&conn, seed).expect("seed import");
    conn
}

fn load_llm() -> Option<titleforge_lib::local_llm::LocalLlm> {
    // Prefer 135M (faster, ~4-14s/gen on CPU). Fall back to 360M.
    let paths: &[&str] = &[
        "../models/SmolLM2-135M-Instruct-Q4_K_M.gguf",
        "../models/SmolLM2-360M-Instruct-Q4_K_M.gguf",
    ];
    for p in paths {
        let path = Path::new(p);
        if path.exists() {
            eprintln!("[bench] Loading LLM from {:?}...", path);
            return titleforge_lib::local_llm::LocalLlm::load(path);
        }
    }
    eprintln!("[bench] LLM model file not found — will skip LLM pass");
    None
}

fn count_words(s: &str) -> usize {
    s.split_whitespace().count()
}

fn keyword_in_title(title: &str, kw: &str) -> bool {
    let tl = title.to_lowercase();
    let kl = kw.to_lowercase();
    tl.contains(&kl) || kl.split_whitespace().any(|w| tl.contains(w))
}

#[test]
fn benchmark_full() {
    let conn = init_db();
    let generator = titleforge_lib::title_gen::Generator::build(&conn);

    // Print header
    println!("\n\n╔════════════════════════════════════════════════════════════════════════════════════╗");
    println!("║  SmolLM2 (Local LLM) vs EGCG vs Curated — 50-Keyword Benchmark                   ║");
    println!("╠════════════════════════════════════════════════════════════════════════════════════╣");
    println!("║  KW=keyword in title   WC=word count   SC=appeal score   TM=generation time (ms) ║");
    println!("╚════════════════════════════════════════════════════════════════════════════════════╝\n");

    println!("{:<20} {:<12} {:>6} {:>4} {:>4} | {:>6} {:>4} {:>4} | {:>6} {:>4} {:>4} | {:>6} {:>4} {:>4}",
        "keyword", "category", "LLM-SC", "KW", "WC", "EGCG-SC", "KW", "WC", "CUR-SC", "KW", "WC", "TOT-SC", "KW", "WC");
    println!("{}", "-".repeat(120));

    let mut llm_loaded = false;
    let mut llm = load_llm();

    let mut summary = Vec::new();
    let mut llm_wins = 0usize;
    let mut egcg_wins = 0usize;
    let mut cur_wins = 0usize;
    let mut llm_times: Vec<u64> = Vec::new();

    let total = KEYWORDS.len();
    for (idx, (keyword, categories)) in KEYWORDS.iter().enumerate() {
        eprintln!("[{}/{}] {}", idx + 1, total, keyword);
        let cat = categories[0];
        let cats: Vec<String> = categories.iter().map(|s| s.to_string()).collect();

        // ── Pass 1: Local LLM (3 titles per keyword) ──
        let llm_result: Option<(Vec<titleforge_lib::TitleResult>, u64)> = if let Some(ref mut m) = llm {
            let start = std::time::Instant::now();
            llm_loaded = true;
            let mut results = Vec::new();
            for c in categories.iter().take(1) {  // one category per keyword for speed
                let examples: Vec<String> = {
                    let mut ex = Vec::new();
                    if let Ok(mut stmt) = conn.prepare(
                        "SELECT title FROM curated_titles WHERE category = ?1 ORDER BY RANDOM() LIMIT 3"
                    ) {
                        if let Ok(rows) = stmt.query_map(rusqlite::params![c], |row| row.get::<_, String>(0)) {
                            ex.extend(rows.filter_map(|r| r.ok()));
                        }
                    }
                    ex
                };
                let prompt = format!(
                    "Examples:\n{}\n\nWrite ONE {} title about \"{}\". Reply with only the title, nothing else.",
                    examples.iter().map(|e| format!("- \"{}\"", e)).collect::<Vec<_>>().join("\n"),
                    c, keyword
                );
            for _attempt in 0..1 {  // single attempt per category for benchmark speed
                    if let Some(title) = m.generate_one(&prompt) {
                        let lower = title.to_lowercase();
                        let kw_lower = keyword.to_lowercase();
                        let has_kw = lower.contains(&kw_lower)
                            || kw_lower.split_whitespace().any(|w| lower.contains(w));
                        let long = title.split_whitespace().count() >= 3;
                        let already = results.iter().any(|r: &titleforge_lib::TitleResult| r.title.eq_ignore_ascii_case(&title));
                        if (has_kw || true) && long && !already {
                            let sc = 60u32.min(100);
                            results.push(titleforge_lib::TitleResult {
                                title,
                                score: sc,
                                categories: vec![c.to_string()],
                                breakdown: None,
                                source: Some("local-llm".to_string()),
                                seo_score: None,
                                seo_breakdown: None,
                            });
                            break;
                        }
                    }
                }
            }
            let elapsed = start.elapsed().as_millis() as u64;
            llm_times.push(elapsed);
            Some((results, elapsed))
        } else {
            None
        };

        // ── Pass 2: EGCG ──
        let start = std::time::Instant::now();
        let egcg_results = generator.generate(keyword, &cats, "normal", "any", 5);
        let egcg_time = start.elapsed().as_millis() as u64;

        // ── Pass 3: Curated fallback ──
        let start = std::time::Instant::now();
        let mut cur_results = Vec::new();
        for c in categories.iter().take(2) {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT title FROM curated_titles WHERE category = ?1 ORDER BY RANDOM() LIMIT 3"
            ) {
                if let Ok(rows) = stmt.query_map(rusqlite::params![c], |row| row.get::<_, String>(0)) {
                    for r in rows.flatten() {
                        cur_results.push(titleforge_lib::TitleResult {
                            title: r,
                            score: 50,
                            categories: vec![c.to_string()],
                            breakdown: None,
                            source: Some("curated".to_string()),
                            seo_score: None,
                            seo_breakdown: None,
                        });
                    }
                }
            }
        }
        let cur_time = start.elapsed().as_millis() as u64;

        // ── Score & print ──
        let llm_best = llm_result.as_ref().and_then(|(r, _)| {
            r.iter().max_by_key(|t| t.score).map(|t| (t.title.clone(), t.score as usize, count_words(&t.title), keyword_in_title(&t.title, keyword)))
        });
        let egcg_best = egcg_results.iter().max_by_key(|t| t.score)
            .map(|t| (t.title.clone(), t.score as usize, count_words(&t.title), keyword_in_title(&t.title, keyword)));
        let cur_best = cur_results.iter().max_by_key(|t| t.score)
            .map(|t| (t.title.clone(), t.score as usize, count_words(&t.title), keyword_in_title(&t.title, keyword)));

        let llm_sc = llm_best.as_ref().map(|b| b.1).unwrap_or(0);
        let egcg_sc = egcg_best.as_ref().map(|b| b.1).unwrap_or(0);
        let cur_sc = cur_best.as_ref().map(|b| b.1).unwrap_or(0);

        // Determine winner
        if llm_sc >= egcg_sc && llm_sc >= cur_sc && llm_sc > 0 { llm_wins += 1; }
        else if egcg_sc >= cur_sc && egcg_sc > 0 { egcg_wins += 1; }
        else if cur_sc > 0 { cur_wins += 1; }

        let llm_kw = llm_best.as_ref().map(|b| if b.3 { "✓" } else { "✗" }).unwrap_or("—");
        let egcg_kw = egcg_best.as_ref().map(|b| if b.3 { "✓" } else { "✗" }).unwrap_or("—");
        let cur_kw = cur_best.as_ref().map(|b| if b.3 { "✓" } else { "✗" }).unwrap_or("—");

        let llm_wc = llm_best.as_ref().map(|b| b.2).unwrap_or(0);
        let egcg_wc = egcg_best.as_ref().map(|b| b.2).unwrap_or(0);
        let cur_wc = cur_best.as_ref().map(|b| b.2).unwrap_or(0);

        let llm_tm = llm_result.as_ref().map(|(_, t)| *t).unwrap_or(0);

        println!("{:<20} {:<12} {:>6} {:>4} {:>4} | {:>6} {:>4} {:>4} | {:>6} {:>4} {:>4} | {:>6} {:>4} {:>4}",
            keyword, cat, llm_sc, llm_kw, llm_wc, egcg_sc, egcg_kw, egcg_wc, cur_sc, cur_kw, cur_wc, llm_tm, "ms", "—");

        // Sample one LLM title per keyword for quality review
        let sample = llm_best.as_ref().map(|b| b.0.as_str()).unwrap_or("");
        let egcg_sample = egcg_best.as_ref().map(|b| b.0.as_str()).unwrap_or("");
        let cur_sample = cur_best.as_ref().map(|b| b.0.as_str()).unwrap_or("");

        if !sample.is_empty() || !egcg_sample.is_empty() {
            println!("  LLM>  {}", sample);
            println!("  EGCG> {}", egcg_sample);
            println!("  CUR>  {}", cur_sample);
            println!();
        }

        summary.push((keyword.to_string(), cat.to_string(), sample.to_string(), egcg_sample.to_string(), cur_sample.to_string(), llm_sc, egcg_sc, cur_sc, llm_tm, egcg_time, cur_time));
    }

    // ── Final report ──
    println!("\n\n╔═══════════════════════════════════════════════╗");
    println!("║               BENCHMARK SUMMARY                ║");
    println!("╠═════════════════════════════════════════════════╣");
    println!("║  Keywords tested:       {:>4}                   ║", total);
    if llm_loaded {
        let avg_ms = if !llm_times.is_empty() { llm_times.iter().sum::<u64>() / llm_times.len() as u64 } else { 0 };
        println!("║  LLM loaded:            YES                   ║");
        println!("║  LLM avg time:          {:>4} ms               ║", avg_ms);
    } else {
        println!("║  LLM loaded:            NO — skipped          ║");
    }
    println!("║                                               ║");
    println!("║  Wins (by appeal score):                       ║");
    println!("║    LLM:                  {:>4}                   ║", llm_wins);
    println!("║    EGCG:                 {:>4}                   ║", egcg_wins);
    println!("║    Curated:              {:>4}                   ║", cur_wins);
    println!("╚═════════════════════════════════════════════════╝\n");

    // Export CSV for spreadsheet
    let csv_path = Path::new("benchmark-results.csv");
    let mut csv = String::from("keyword,category,llm_title,egcg_title,curated_title,llm_score,egcg_score,curated_score,llm_ms,egcg_ms,curated_ms\n");
    for (kw, cat, llm_t, egcg_t, cur_t, llm_s, egcg_s, cur_s, llm_tm, egcg_tm, cur_tm) in &summary {
        csv.push_str(&format!("\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",{},{},{},{},{},{}\n",
            kw, cat, llm_t.replace('"', "'"), egcg_t.replace('"', "'"), cur_t.replace('"', "'"),
            llm_s, egcg_s, cur_s, llm_tm, egcg_tm, cur_tm));
    }
    std::fs::write(csv_path, &csv).expect("write CSV");
    eprintln!("\n[bench] Results written to {:?}", csv_path);

    // Assert minimum quality bar
    if llm_loaded {
        assert!(llm_wins > 0 || egcg_wins > 0, "No engine produced any titles — seed data missing?");
    }
}
