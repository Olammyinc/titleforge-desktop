/// Benchmark the PRODUCTION path — `engine::generate`, exactly what the app calls.
///
/// Why this exists: `bench_judge.rs` calls `generate_one_clean` directly, which
/// bypasses three of the five quality changes shipped 2026-08-02:
///   - Task 2 best-of-N pool + ranking   (engine.rs)
///   - Task 3 few-shot fallback           (engine.rs)
///   - Task 4 constraint rotation         (engine.rs)
/// Only Tasks 1 and 5 live inside `generate_one_clean`. So the old benchmark
/// cannot answer "did the quality sprint work?" — it tests a path users never hit.
///
/// This harness calls `engine::generate(..., quantity=1, tier)` per keyword, so
/// every shipped improvement is exercised. Judge + rubric + cache are identical
/// to bench_judge.rs so numbers are directly comparable to the 80.0 baseline.
///
/// Usage:
///   cargo test --release bench_production -- --nocapture
/// Key: BENCH_JUDGE_API_KEY env var, or ../.bench-key

use std::path::Path;
use std::collections::HashMap;
use rusqlite::Connection;

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

// ── Identical gate + judge to bench_judge.rs so results are comparable ──

fn is_readable(title: &str) -> bool {
    if title.contains('{') || title.contains('}') { return false; }
    if title.len() < 3 || title.len() > 150 { return false; }
    let wc = title.split_whitespace().count();
    if wc < 2 || wc > 25 { return false; }
    let lower = title.to_lowercase();
    let echoes = ["here is", "i would", "sure", "let me", "title:", "i'm", "i can", "i think", "```", "here's"];
    !echoes.iter().any(|e| lower.starts_with(e))
}

fn hash_key(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn cache_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("bench-cache.json")
}

fn load_cache() -> HashMap<String, serde_json::Value> {
    std::fs::read_to_string(cache_path()).ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn read_api_key() -> String {
    if let Ok(k) = std::env::var("BENCH_JUDGE_API_KEY") { return k; }
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(".bench-key");
    // PowerShell's `echo >` writes UTF-16LE; decode defensively.
    std::fs::read(&p).ok().and_then(|b| {
        let s = match b.as_slice() {
            [0xFF, 0xFE, rest @ ..] => String::from_utf16(
                &rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()).ok(),
            [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8(rest.to_vec()).ok(),
            other => String::from_utf8(other.to_vec()).ok(),
        }?;
        let t = s.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
    }).unwrap_or_default()
}

fn call_judge(title: &str, keyword: &str, category: &str, api_key: &str) -> Option<u32> {
    let rubric = format!(
        "You are grading titles for a title generator. The user asked for a title about \"{keyword}\" in the {category} category.\n\nHere is a candidate: \"{title}\"\n\nRate 0-100 on USABILITY: would a real creator publish this without editing?\n\nDeduct heavily for:\n- Not on-topic for the keyword\n- Grammatical errors, awkward phrasing, template shrapnel\n- Vague and generic (\"The Peak Truth About X\")\n- Category mismatch\n- Boring, clichéd, or noise\n\nReward:\n- Specific, concrete language\n- Curiosity or emotional hook\n- Category-appropriate voice and length\n- Something a human would actually click / buy / read\n\nOutput ONLY a single integer 0-100. No explanation."
    );
    let body = serde_json::json!({
        "model": "deepseek-v4-flash",
        "messages": [{"role": "user", "content": rubric}],
        "temperature": 0.0, "max_tokens": 64,
        "thinking": {"type": "disabled"},
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30)).build().ok()?;
    for _ in 0..3u32 {
        let resp = match client.post("https://api.deepseek.com/v1/chat/completions")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body).send() { Ok(r) => r, Err(_) => continue };
        if !resp.status().is_success() { std::thread::sleep(std::time::Duration::from_secs(2)); continue; }
        let data: serde_json::Value = match resp.json() { Ok(d) => d, Err(_) => continue };
        if let Some(t) = data["choices"][0]["message"]["content"].as_str() {
            let n: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
            if let Ok(v) = n.parse::<u32>() { if v > 0 && v <= 100 { return Some(v); } }
        }
    }
    None
}

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
fn bench_production() {
    let api_key = read_api_key();
    if api_key.is_empty() { eprintln!("No judge API key — skipping."); return; }

    let conn = init_db();
    let generator = titleforge_lib::title_gen::Generator::build(&conn);
    let mut cache = load_cache();

    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("models")
        .join("qwen2.5-1.5b-instruct-q4_k_m.gguf");
    let mut llm = match titleforge_lib::local_llm::LocalLlm::load(&p) {
        Some(m) => m,
        None => { eprintln!("Qwen model not found — skipping."); return; }
    };

    println!("\n╔═══════════════════════════════════════════════════════════════════╗");
    println!("║  PRODUCTION-PATH benchmark — engine::generate (all 5 tasks live)  ║");
    println!("║  Baseline to beat: mean 80.0 (07-31, pre-quality-sprint)          ║");
    println!("╚═══════════════════════════════════════════════════════════════════╝\n");

    let mut csv = String::from("keyword,category,title,source,readable,judge_score,usable\n");
    let (mut scores, mut usable, mut empty, mut cliche) = (Vec::<u32>::new(), 0usize, 0usize, 0usize);
    let cliche_words = ["ultimate", "unlock", "unleash", "revolutioniz", "game changer", "mind-blowing", "life-changing"];
    let total = BENCH_KEYWORDS.len();
    let t0 = std::time::Instant::now();

    for (i, (kw, cat)) in BENCH_KEYWORDS.iter().enumerate() {
        eprintln!("[{}/{}] {}", i + 1, total, kw);
        let cats = vec![cat.to_string()];
        // quantity=1, tier=pro — the real orchestrator, all five tasks active.
        let out = titleforge_lib::engine::generate(
            &conn, &generator, Some(&mut llm), kw, &cats, "normal", "any", 1, "pro",
            &Default::default(),
        ).unwrap_or_default();

        let (title, source) = match out.first() {
            Some(t) => (t.title.clone(), t.source.clone().unwrap_or_default()),
            None => (String::new(), String::new()),
        };
        if title.trim().is_empty() { empty += 1; }
        let lower = title.to_lowercase();
        if cliche_words.iter().any(|c| lower.contains(c)) { cliche += 1; }

        let readable = !title.trim().is_empty() && is_readable(&title);
        let score = if readable {
            let k = hash_key(&format!("{}|{}|{}", title, kw, cat));
            match cache.get(&k).and_then(|v| v.as_u64()).filter(|&s| s > 0) {
                Some(s) => s as u32,
                None => {
                    let s = call_judge(&title, kw, cat, &api_key).unwrap_or(0);
                    if s > 0 { cache.insert(k, serde_json::json!(s)); }
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    s
                }
            }
        } else { 0 };

        if score > 0 { scores.push(score); }
        if score >= 70 { usable += 1; }
        csv.push_str(&format!("\"{}\",\"{}\",\"{}\",\"{}\",{},{},{}\n",
            kw, cat, title.replace('"', "'"), source, readable as u8, score, (score >= 70) as u8));
    }

    let _ = std::fs::write(cache_path(), serde_json::to_string_pretty(&cache).unwrap_or_default());
    let out_csv = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("bench-production.csv");
    let _ = std::fs::write(&out_csv, &csv);

    let mean = if scores.is_empty() { 0.0 } else { scores.iter().sum::<u32>() as f64 / scores.len() as f64 };
    let mut sorted = scores.clone(); sorted.sort_unstable();
    let median = if sorted.is_empty() { 0 } else { sorted[sorted.len() / 2] };
    let drift = sorted.iter().filter(|&&s| s < 50).count();

    println!("\n╔═══════════════════════════════════════════════════════════════════╗");
    println!("║  PRODUCTION PATH RESULTS                                          ║");
    println!("╠═══════════════════════════════════════════════════════════════════╣");
    println!("║  Titles produced : {:>2}/{:<2}   (empty: {:<2})                          ║", total - empty, total, empty);
    println!("║  Mean score      : {:>5.1}    (baseline 80.0)                      ║", mean);
    println!("║  Median          : {:>5}                                          ║", median);
    println!("║  Usable >=70     : {:>2}/{:<2} = {:>3}%                                 ║", usable, total, usable * 100 / total);
    println!("║  Drift (<50)     : {:>2}                                             ║", drift);
    println!("║  Cliche titles   : {:>2}/{:<2}  (was 21/50 pre-sprint)                ║", cliche, total);
    println!("║  Wall clock      : {:>5.1} min                                      ║", t0.elapsed().as_secs_f64() / 60.0);
    println!("╚═══════════════════════════════════════════════════════════════════╝");
    println!("\nCSV: {:?}", out_csv);
}
