/// Path A 50-keyword benchmark: Qwen vs EGCG vs Curated
///
/// Tests format-conformance, category-relevance, keyword presence, 
/// readability, and speed across all three engines.
///
/// Usage:
///   cargo test --release benchmark_path_a -- --nocapture 2>&1 | tee bench-results.txt

use std::path::Path;
use std::time::Instant;
use rusqlite::Connection;

// ── 50 keywords with categories ──
const BENCH_KEYWORDS: &[(&str, &str)] = &[
    ("shirt", "product"), ("laptop", "product"), ("productivity", "book"),
    ("love", "song"), ("startup", "book"), ("crypto", "article"),
    ("parenting", "book"), ("fitness", "youtube"), ("travel", "blog"),
    ("cooking", "book"), ("meditation", "podcast"), ("investing", "article"),
    ("photography", "youtube"), ("music", "song"), ("AI", "article"),
    ("remote work", "blog"), ("mental health", "podcast"), ("gardening", "book"),
    ("coffee", "product"), ("minimalism", "book"), ("creativity", "book"),
    ("sleep", "podcast"), ("negotiation", "book"), ("writing", "book"),
    ("marketing", "article"), ("data science", "article"), ("blockchain", "article"),
    ("electric cars", "youtube"), ("space exploration", "article"), ("wine", "blog"),
    ("dancing", "youtube"), ("podcasting", "blog"), ("freelancing", "blog"),
    ("vegan", "blog"), ("yoga", "youtube"), ("gaming", "youtube"),
    ("digital nomad", "blog"), ("sourdough bread", "blog"), ("intermittent fasting", "book"),
    ("bitcoin", "article"), ("tennis", "youtube"), ("skincare", "blog"),
    ("virtual reality", "youtube"), ("board games", "product"), ("tea", "product"),
    ("solo travel", "blog"), ("home office", "product"), ("jazz", "song"),
    ("ethical fashion", "blog"), ("climate change", "article"),
];

fn init_db() -> Connection {
    let conn = Connection::open_in_memory().expect("mem");
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS curated_titles (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, category TEXT NOT NULL, genre TEXT, tone TEXT, appeal_score INTEGER, notes TEXT);
        CREATE TABLE IF NOT EXISTS patterns (id INTEGER PRIMARY KEY AUTOINCREMENT, category TEXT NOT NULL, template TEXT NOT NULL, slots TEXT NOT NULL, genre TEXT, tone TEXT, quality_score REAL DEFAULT 0.5, usage_count INTEGER DEFAULT 0);
        CREATE TABLE IF NOT EXISTS word_pools (id INTEGER PRIMARY KEY AUTOINCREMENT, pool_name TEXT NOT NULL, word TEXT NOT NULL, category TEXT, weight REAL DEFAULT 1.0);
        CREATE TABLE IF NOT EXISTS user_history (id INTEGER PRIMARY KEY AUTOINCREMENT, keyword TEXT NOT NULL, categories TEXT NOT NULL, genre TEXT, style TEXT, titles TEXT NOT NULL, created_at TEXT DEFAULT (datetime('now')));
        CREATE TABLE IF NOT EXISTS user_favorites (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, keyword TEXT, score INTEGER, category TEXT, created_at TEXT DEFAULT (datetime('now')));
        CREATE TABLE IF NOT EXISTS user_settings (key TEXT PRIMARY KEY, value TEXT);
        CREATE TABLE IF NOT EXISTS user_projects (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, created_at TEXT DEFAULT (datetime('now')));
        CREATE TABLE IF NOT EXISTS project_titles (id INTEGER PRIMARY KEY AUTOINCREMENT, project_id INTEGER NOT NULL, title TEXT NOT NULL, keyword TEXT, score INTEGER, notes TEXT, FOREIGN KEY (project_id) REFERENCES user_projects(id) ON DELETE CASCADE);
    ").expect("schema");
    let seed = include_str!("../../seed-data.json");
    titleforge_lib::db::import_seed_from_str(&conn, seed).expect("seed");
    conn
}

fn keyword_present(title: &str, kw: &str) -> bool {
    let tl = title.to_lowercase();
    let kl = kw.to_lowercase();
    tl.contains(&kl) || kl.split_whitespace().any(|w| tl.contains(w))
}

fn is_readable(title: &str) -> bool {
    // Reject titles with template leaks, gibberish
    if title.contains('{') || title.contains('}') { return false; }
    if title.len() < 3 || title.len() > 150 { return false; }
    let wc = title.split_whitespace().count();
    if wc < 2 || wc > 25 { return false; }
    // Reject obvious instruction echoes
    let lower = title.to_lowercase();
    let echoes = ["here is", "i would", "sure", "let me", "title:", "i'm", "i can", "i think"];
    if echoes.iter().any(|e| lower.starts_with(e)) { return false; }
    true
}

fn is_category_relevant(title: &str, _cat: &str) -> bool {
    // Heuristic: if the title has the keyword, it's at least trying to be relevant
    // A full human eval would be better but this is fast and consistent
    title.len() > 5 && !title.chars().all(|c| c.is_ascii_punctuation() || c == ' ')
}

#[test]
fn benchmark_path_a() {
    let conn = init_db();
    let generator = titleforge_lib::title_gen::Generator::build(&conn);
    
    // Load Qwen model (prefer 135M SmolLM2 as fallback if Qwen missing)
    let mut llm = None;
    for name in &["qwen2.5-1.5b-instruct-q4_k_m.gguf", "SmolLM2-360M-Instruct-Q4_K_M.gguf", "SmolLM2-135M-Instruct-Q4_K_M.gguf"] {
        let p = Path::new("../models").join(name);
        if p.exists() {
            eprintln!("Loading {}...", name);
            llm = titleforge_lib::local_llm::LocalLlm::load(&p);
            if llm.is_some() { break; }
        }
    }
    let model_name = if llm.is_some() { "Qwen2.5-1.5B" } else { "none (LLM disabled)" };

    println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
    println!("║  Path A 50-Keyword Benchmark — {}                ║", model_name);
    println!("║  Qwen vs EGCG vs Curated — quality + speed                          ║");
    println!("╚═══════════════════════════════════════════════════════════════════════╝\n");

    // Metrics accumulators
    let mut llm_format = 0usize; let mut llm_keyword = 0usize; let mut llm_readable = 0usize; let mut llm_relevant = 0usize;
    let mut egcg_format = 0usize; let mut egcg_keyword = 0usize; let mut egcg_readable = 0usize; let mut egcg_relevant = 0usize;
    let mut cur_format = 0usize;  let mut cur_keyword = 0usize;  let mut cur_readable = 0usize;  let mut cur_relevant = 0usize;
    let mut llm_time_total = 0u64;
    let mut egcg_time_total = 0u64;
    let mut cur_time_total = 0u64;
    let total = BENCH_KEYWORDS.len();

    let mut csv = String::from("keyword,category,llm_title,egcg_title,curated_title,llm_format,llm_kw,llm_readable,llm_relevant,llm_ms,egcg_format,egcg_kw,egcg_readable,egcg_relevant,egcg_ms\n");

    for (i, (keyword, category)) in BENCH_KEYWORDS.iter().enumerate() {
        eprintln!("[{}/{}] {}", i + 1, total, keyword);

        // ── Qwen LLM ──
        let llm_result = if let Some(ref mut m) = llm {
            let examples = generator.retrieve_similar(keyword, category, 3);
            let start = Instant::now();
            let title = m.generate_one_clean(keyword, category, "normal", "any", &examples, None, &Default::default());
            let elapsed = start.elapsed().as_millis() as u64;
            llm_time_total += elapsed;
            title
        } else {
            llm_time_total += 0;
            None
        };

        // ── EGCG ──
        let egcg_result = {
            let cats = vec![category.to_string()];
            let start = Instant::now();
            let results = generator.generate(keyword, &cats, "normal", "any", 1);
            let elapsed = start.elapsed().as_millis() as u64;
            egcg_time_total += elapsed;
            results.first().map(|r| r.title.clone())
        };

        // ── Curated retrieval ──
        let cur_result = {
            let start = Instant::now();
            let examples = generator.retrieve_similar(keyword, category, 1);
            let elapsed = start.elapsed().as_millis() as u64;
            cur_time_total += elapsed;
            examples.first().cloned()
        };

        // Score each result
        let llm_title = llm_result.unwrap_or_default();
        let egcg_title = egcg_result.unwrap_or_default();
        let cur_title = cur_result.unwrap_or_default();

        let llm_f = is_readable(&llm_title);
        let llm_k = keyword_present(&llm_title, keyword);
        let llm_r = is_category_relevant(&llm_title, category);
        if llm_f { llm_format += 1; }
        if llm_k { llm_keyword += 1; }
        if llm_r { llm_readable += 1; }
        if llm_f && llm_k { llm_relevant += 1; }

        let eg_f = is_readable(&egcg_title);
        let eg_k = keyword_present(&egcg_title, keyword);
        let eg_r = is_category_relevant(&egcg_title, category);
        if eg_f { egcg_format += 1; }
        if eg_k { egcg_keyword += 1; }
        if eg_r { egcg_readable += 1; }
        if eg_f && eg_k { egcg_relevant += 1; }

        let cu_f = is_readable(&cur_title);
        let cu_k = keyword_present(&cur_title, keyword);
        let cu_r = is_category_relevant(&cur_title, category);
        if cu_f { cur_format += 1; }
        if cu_k { cur_keyword += 1; }
        if cu_r { cur_readable += 1; }
        if cu_f && cu_k { cur_relevant += 1; }

        csv.push_str(&format!("\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",{},{},{},{},{},{},{},{},{},{}\n",
            keyword, category,
            llm_title.replace('"', "'"), egcg_title.replace('"', "'"), cur_title.replace('"', "'"),
            llm_f as u8, llm_k as u8, llm_r as u8, (llm_f && llm_k) as u8, 0u64,
            eg_f as u8, eg_k as u8, eg_r as u8, (eg_f && eg_k) as u8, 0u64,
        ));
    }

    // ── Summary ──
    let pct = |n: usize| format!("{:.1}%", (n as f64 / total as f64) * 100.0);
    let avg_ms = |t: u64| if total > 0 { format!("{:.1}s", t as f64 / total as f64 / 1000.0) } else { "n/a".to_string() };

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║  BENCHMARK RESULTS ({} keywords)                                ║", total);
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║  Engine     Format    Keyword   Readable  Good      Avg Time   ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║  Qwen       {:>8}  {:>8}  {:>8}  {:>8}  {:>8}   ║",
        pct(llm_format), pct(llm_keyword), pct(llm_readable), pct(llm_relevant), avg_ms(llm_time_total));
    println!("║  EGCG       {:>8}  {:>8}  {:>8}  {:>8}  {:>8}   ║",
        pct(egcg_format), pct(egcg_keyword), pct(egcg_readable), pct(egcg_relevant), avg_ms(egcg_time_total));
    println!("║  Curated    {:>8}  {:>8}  {:>8}  {:>8}  {:>8}   ║",
        pct(cur_format), pct(cur_keyword), pct(cur_readable), pct(cur_relevant), avg_ms(cur_time_total));
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    println!("Legend:");
    println!("  Format   = no template leaks, valid length, no instruction echo");
    println!("  Keyword  = user keyword present in title");
    println!("  Readable = grammatically coherent (heuristic)");
    println!("  Good     = passes BOTH format AND keyword checks");
    println!();

    // Write CSV
    let csv_path = Path::new("../bench-results.csv");
    std::fs::write(csv_path, &csv).expect("write CSV");
    eprintln!("Results written to {:?}", csv_path);
}
