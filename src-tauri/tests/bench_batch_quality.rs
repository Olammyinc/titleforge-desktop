/// Judge a REAL batch — the regime the product actually sells.
///
/// Every quality number in this repo so far is k=1 (one title per keyword).
/// The product sells 25 / 50 / 200 per request. At batch scale two effects that
/// dominate k=1 measurements largely dissolve:
///   - best-of-N generates ~4x candidates for the slots, so occasional empties
///     are absorbed rather than counting as a failed keyword
///   - ranking discards the low scorers, so a fat bottom tail never reaches the user
/// Brief rule #6: measure at the batch size the product actually sells.
///
/// A/B: set TF_NO_CONSTRAINTS=1 to disable Task 4 constraint rotation.
///
/// Usage:
///   cargo test --release bench_batch_quality -- --nocapture
/// Key: BENCH_JUDGE_API_KEY, or ../.bench-key

use std::path::Path;
use std::collections::HashMap;
use rusqlite::Connection;

/// (keyword, category). Two keywords keeps runtime sane: Core tier uses a 4x
/// best-of-N multiplier, so 25 titles is ~100 LLM calls at ~7s each.
const BATCH_KEYWORDS: &[(&str, &str)] = &[
    ("coffee", "product"),
    ("remote work", "blog"),
];
const QUANTITY: u32 = 25;
const TIER: &str = "core";

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

/// Identical rubric to bench_production.rs / bench_judge.rs — comparable numbers.
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
fn bench_batch_quality() {
    let api_key = read_api_key();
    if api_key.is_empty() { eprintln!("No judge API key — skipping."); return; }
    let constraints_off = std::env::var("TF_NO_CONSTRAINTS").is_ok();

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
    println!("║  BATCH QUALITY — {} x {} titles, tier={}                        ║", BATCH_KEYWORDS.len(), QUANTITY, TIER);
    println!("║  Task 4 constraint rotation: {}                              ║",
        if constraints_off { "OFF" } else { "ON " });
    println!("╚═══════════════════════════════════════════════════════════════════╝\n");

    let mut csv = String::from("keyword,category,rank,title,source,judge_score,usable\n");
    let mut all_scores: Vec<u32> = Vec::new();
    let mut total_returned = 0usize;
    let mut total_unique = 0usize;
    let mut total_usable = 0usize;
    let cliche_words = ["ultimate", "unlock", "unleash", "revolutioniz", "game changer", "mind-blowing", "life-changing"];
    let mut total_cliche = 0usize;
    let t0 = std::time::Instant::now();

    for (kw, cat) in BATCH_KEYWORDS {
        eprintln!("[batch] generating {} titles for '{}' ...", QUANTITY, kw);
        let cats = vec![cat.to_string()];
        let gen_start = std::time::Instant::now();
        let results = titleforge_lib::engine::generate(
            &conn, &generator, Some(&mut llm), kw, &cats, "normal", "any", QUANTITY, TIER,
        ).unwrap_or_default();
        let gen_secs = gen_start.elapsed().as_secs_f64();

        let uniq: std::collections::HashSet<String> =
            results.iter().map(|r| r.title.to_lowercase()).collect();
        total_returned += results.len();
        total_unique += uniq.len();

        eprintln!("[batch]   returned {} ({} unique) in {:.0}s — judging...", results.len(), uniq.len(), gen_secs);

        for (i, r) in results.iter().enumerate() {
            let lower = r.title.to_lowercase();
            if cliche_words.iter().any(|c| lower.contains(c)) { total_cliche += 1; }
            let k = hash_key(&format!("{}|{}|{}", r.title, kw, cat));
            let score = match cache.get(&k).and_then(|v| v.as_u64()).filter(|&s| s > 0) {
                Some(s) => s as u32,
                None => {
                    let s = call_judge(&r.title, kw, cat, &api_key).unwrap_or(0);
                    if s > 0 { cache.insert(k, serde_json::json!(s)); }
                    std::thread::sleep(std::time::Duration::from_millis(120));
                    s
                }
            };
            if score > 0 { all_scores.push(score); }
            if score >= 70 { total_usable += 1; }
            csv.push_str(&format!("\"{}\",\"{}\",{},\"{}\",\"{}\",{},{}\n",
                kw, cat, i + 1, r.title.replace('"', "'"),
                r.source.as_deref().unwrap_or(""), score, (score >= 70) as u8));
        }
    }

    let _ = std::fs::write(cache_path(), serde_json::to_string_pretty(&cache).unwrap_or_default());
    let suffix = if constraints_off { "no-constraints" } else { "constraints" };
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
        .join(format!("bench-batch-{}.csv", suffix));
    let _ = std::fs::write(&out, &csv);

    let mean = if all_scores.is_empty() { 0.0 } else { all_scores.iter().sum::<u32>() as f64 / all_scores.len() as f64 };
    let mut sorted = all_scores.clone(); sorted.sort_unstable();
    let median = if sorted.is_empty() { 0 } else { sorted[sorted.len() / 2] };
    let drift = sorted.iter().filter(|&&s| s < 50).count();
    let requested = BATCH_KEYWORDS.len() * QUANTITY as usize;

    println!("\n╔═══════════════════════════════════════════════════════════════════╗");
    println!("║  BATCH RESULTS — Task 4 {}                                    ║", if constraints_off { "OFF" } else { "ON " });
    println!("╠═══════════════════════════════════════════════════════════════════╣");
    println!("║  Requested        : {:>3}                                           ║", requested);
    println!("║  Returned         : {:>3}  ({} unique)                             ║", total_returned, total_unique);
    println!("║  Mean score       : {:>5.1}                                         ║", mean);
    println!("║  Median           : {:>3}                                           ║", median);
    println!("║  Usable >=70      : {:>3}/{:<3} = {:>3}%                                ║",
        total_usable, total_returned, if total_returned > 0 { total_usable * 100 / total_returned } else { 0 });
    println!("║  Drift (<50)      : {:>3}                                           ║", drift);
    println!("║  Cliche titles    : {:>3}                                           ║", total_cliche);
    println!("║  Wall clock       : {:>5.1} min                                     ║", t0.elapsed().as_secs_f64() / 60.0);
    println!("╚═══════════════════════════════════════════════════════════════════╝");
    println!("\nCSV: {:?}", out);
}
