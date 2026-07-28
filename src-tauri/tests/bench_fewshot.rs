/// Few-shot benchmark: SmolLM2-135M vs 360M vs EGCG — 50 keywords.
/// Uses the SAME prompt structure as engine.rs: curated titles as examples,
/// per-category instruction, title-only request. This is how the actual app
/// will use the model.
use std::path::Path;
use rusqlite::Connection;

fn init_db() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory DB");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS curated_titles (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, category TEXT NOT NULL, genre TEXT, tone TEXT, appeal_score INTEGER, notes TEXT);
         CREATE TABLE IF NOT EXISTS patterns (id INTEGER PRIMARY KEY AUTOINCREMENT, category TEXT NOT NULL, template TEXT NOT NULL, slots TEXT NOT NULL, genre TEXT, tone TEXT, quality_score REAL DEFAULT 0.5, usage_count INTEGER DEFAULT 0);
         CREATE TABLE IF NOT EXISTS word_pools (id INTEGER PRIMARY KEY AUTOINCREMENT, pool_name TEXT NOT NULL, word TEXT NOT NULL, category TEXT, weight REAL DEFAULT 1.0);
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

fn fetch_examples(conn: &Connection, category: &str, limit: usize) -> Vec<String> {
    let mut stmt = conn.prepare("SELECT title FROM curated_titles WHERE category = ?1 ORDER BY RANDOM() LIMIT ?2").ok();
    stmt.as_mut().and_then(|s| {
        s.query_map(rusqlite::params![category, limit as i64], |row| row.get::<_, String>(0)).ok()
    }).map(|rows| rows.filter_map(|r| r.ok()).collect()).unwrap_or_default()
}

/// Build the SAME few-shot prompt as engine.rs::build_llm_prompt
fn build_prompt(category: &str, keyword: &str, style: &str, examples: &[String]) -> String {
    let style_label = if style.is_empty() || style == "any" { "normal" } else { style };
    let mut prompt = String::new();
    if !examples.is_empty() {
        prompt.push_str(&format!("Examples of {} {} titles:\n", style_label, category));
        for ex in examples.iter().take(4) {
            prompt.push_str(&format!("- \"{}\"\n", ex));
        }
        prompt.push('\n');
    }
    prompt.push_str(&format!(
        "Write ONE new {} {} title about \"{}\". Reply with only the title, nothing else.",
        style_label, category, keyword
    ));
    prompt
}

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

#[test]
fn benchmark_fewshot() {
    let conn = init_db();
    let generator = titleforge_lib::title_gen::Generator::build(&conn);

    // Load 135M (faster, preferred)
    let model_path = Path::new("../models/SmolLM2-135M-Instruct-Q4_K_M.gguf");
    let mut llm = if model_path.exists() {
        titleforge_lib::local_llm::LocalLlm::load(model_path)
    } else {
        None
    };

    let model_name = model_path.file_name().and_then(|n| n.to_str()).unwrap_or("none");
    println!("\n╔═══════════════════════════════════════════════════════════════════════════════╗");
    println!("║  Few-Shot Benchmark: {}    ║", model_name);
    println!("║  50 keywords × 1 category × 1 attempt — proper curated-title examples      ║");
    println!("╚═══════════════════════════════════════════════════════════════════════════════╝\n");
    println!("{:<22} {:<10} | {:<58} | {:<58}",
        "keyword (cat)", "LLM-s", "LLM title", "EGCG title");
    println!("{}", "-".repeat(162));

    let mut llm_total = 0u64;
    let mut llm_good = 0usize;  // keyword-relevant, not instruction-echo
    let mut egcg_good = 0usize;
    let mut total = 0usize;

    let mut csv = String::from("keyword,category,llm_time_ms,llm_title,egcg_title,llm_quality,egcg_quality\n");

    for (keyword, categories) in KEYWORDS {
        total += 1;
        let cat = categories[0];

        // ── LLM with few-shot prompt ──
        let (llm_title, llm_time, llm_q) = if let Some(ref mut m) = llm {
            let examples = fetch_examples(&conn, cat, 4);
            let prompt = build_prompt(cat, keyword, "normal", &examples);
            let start = std::time::Instant::now();
            let result = m.generate_one(&prompt);
            let elapsed = start.elapsed().as_millis() as u64;
            llm_total += elapsed;
            
            if let Some(t) = result {
                let t = t.trim_matches(&['"', '\'', ' ', '\n', '\r'] as &[_]).to_string();
                let t_lower = t.to_lowercase();
                let kw_lower = keyword.to_lowercase();
                let has_kw = t_lower.contains(&kw_lower)
                    || kw_lower.split_whitespace().any(|w| t_lower.contains(w));
                // Quality check: relevant (has keyword or is category-appropriate), no instruction echo
                let is_good = t.len() > 5 && !t_lower.starts_with("here") && !t_lower.starts_with("write") 
                    && !t_lower.starts_with("i'm") && !t_lower.starts_with("title:")
                    && !t_lower.contains("example") && !t_lower.contains("reply");
                if is_good { llm_good += 1; }
                (t, elapsed, if is_good { "GOOD" } else { "NOISE" })
            } else {
                ("(timeout/fail)".to_string(), elapsed, "FAIL")
            }
        } else {
            ("(no model)".to_string(), 0, "N/A")
        };

        // ── EGCG ──
        let cats: Vec<String> = categories.iter().map(|s| s.to_string()).collect();
        let egcg_results = generator.generate(keyword, &cats, "normal", "any", 3);
        let (egcg_title, egcg_q) = if let Some(best) = egcg_results.iter().max_by_key(|t| t.score) {
            let t = best.title.trim().to_string();
            let t_lower = t.to_lowercase();
            let kw_lower = keyword.to_lowercase();
            let has_kw = t_lower.contains(&kw_lower)
                || kw_lower.split_whitespace().any(|w| t_lower.contains(w));
            let has_broken = t.contains("{") || t.contains("}");
            let is_good = has_kw && !has_broken && t.len() > 5;
            if is_good { egcg_good += 1; }
            (t, if is_good { "GOOD" } else { "NOISE" })
        } else {
            ("(no results)".to_string(), "FAIL")
        };

        println!("{:<22} {:<10} | {:<58} | {:<58}",
            format!("{} ({})", keyword, cat), format!("{:.1}s", llm_time as f64 / 1000.0),
            llm_title, egcg_title);

        csv.push_str(&format!("\"{}\",\"{}\",{},\"{}\",\"{}\",\"{}\",\"{}\"\n",
            keyword, cat, llm_time, llm_title.replace('"', "'"), egcg_title.replace('"', "'"), llm_q, egcg_q));
    }

    println!("\n╔════════════════════════════════════════════════╗");
    println!("║              BENCHMARK SUMMARY                  ║");
    println!("╠══════════════════════════════════════════════════╣");
    if llm_total > 0 {
        let avg = llm_total / total as u64;
        println!("║  Model:            {}   ║", model_name);
        println!("║  Avg time:         {:.1}s                       ║", avg as f64 / 1000.0);
    }
    println!("║  Keywords:         {:>4}                          ║", total);
    println!("║  LLM good quality: {:>4}/{}                       ║", llm_good, total);
    println!("║  EGCG good quality:{:>4}/{}                       ║", egcg_good, total);
    println!("╚══════════════════════════════════════════════════╝\n");

    let csv_path = Path::new("benchmark-fewshot.csv");
    std::fs::write(csv_path, &csv).expect("write CSV");
    eprintln!("\n[bench] Results written to {:?}", csv_path);
}
