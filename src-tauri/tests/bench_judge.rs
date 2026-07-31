/// Task 1 — Benchmark v2 with LLM-judge
///
/// Every title that passes mechanical gates is scored 0-100 by a cloud AI
/// on usability: "Would a real creator publish this without editing?"
///
/// REQUIREMENTS:
///   Setenv BENCH_JUDGE_API_KEY  (your DeepSeek API key)
///   Optionally set BENCH_JUDGE_PROVIDER (deepseek|openai|anthropic, default: deepseek)
///
/// USAGE:
///   cargo test --release benchmark_judge -- --nocapture 2>&1 | tee bench-judge.txt
///
/// Outputs:
///   bench-usability.csv  — 150 rows (50 keywords × 3 engines) with judge scores
///   bench-cache.json     — cached judge responses (re-run costs $0 on same data)

use std::time::Instant;
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
    // Strip punctuation so "Shirt:" / "Startup's" / "coffee." match their keywords.
    // Previously failed because " shirt " wasn't found in " shirt: " (colon glued to word).
    let clean = |s: &str| -> String {
        s.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' })
            .collect()
    };
    let tl = format!(" {} ", clean(title));
    let kl = clean(kw).trim().to_string();
    tl.contains(&format!(" {} ", &kl))
        || kl.split_whitespace().any(|w| tl.contains(&format!(" {} ", w)))
}

fn is_readable(title: &str) -> bool {
    if title.contains('{') || title.contains('}') { return false; }
    if title.len() < 3 || title.len() > 150 { return false; }
    let wc = title.split_whitespace().count();
    if wc < 2 || wc > 25 { return false; }
    let lower = title.to_lowercase();
    let echoes = ["here is", "i would", "sure", "let me", "title:", "i'm", "i can", "i think", "```", "here's"];
    if echoes.iter().any(|e| lower.starts_with(e)) { return false; }
    true
}

fn sha256_short(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn cache_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("bench-cache.json")
}

fn csv_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("bench-usability.csv")
}

fn load_cache() -> HashMap<String, serde_json::Value> {
    let path = cache_path();
    if !path.exists() { return HashMap::new(); }
    std::fs::read_to_string(path).ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_cache(cache: &HashMap<String, serde_json::Value>) {
    let json = serde_json::to_string_pretty(cache).unwrap_or_default();
    let _ = std::fs::write(cache_path(), &json);
}

fn call_judge(title: &str, keyword: &str, category: &str, api_key: &str) -> Option<u32> {
    let rubric = format!(
        "You are grading titles for a title generator. The user asked for a title about \"{keyword}\" in the {category} category.\n\nHere is a candidate: \"{title}\"\n\nRate 0-100 on USABILITY: would a real creator publish this without editing?\n\nDeduct heavily for:\n- Not on-topic for the keyword\n- Grammatical errors, awkward phrasing, template shrapnel\n- Vague and generic (\"The Peak Truth About X\")\n- Category mismatch\n- Boring, clichéd, or noise\n\nReward:\n- Specific, concrete language\n- Curiosity or emotional hook\n- Category-appropriate voice and length\n- Something a human would actually click / buy / read\n\nOutput ONLY a single integer 0-100. No explanation."
    );

    let body = serde_json::json!({
        "model": "deepseek-v4-flash",
        "messages": [
            {"role": "user", "content": rubric}
        ],
        "temperature": 0.0,
        "max_tokens": 64,
        "thinking": {"type": "disabled"},
    });

    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build() {
        Ok(c) => c,
        Err(e) => { eprintln!("  [judge] client build error: {:?}", e); return None; }
    };

    // Retry up to 3 times for transient failures
    for attempt in 1..=3u32 {
        let resp = match client
            .post("https://api.deepseek.com/v1/chat/completions")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send() {
            Ok(r) => r,
            Err(e) => { eprintln!("  [judge] HTTP error (attempt {}): {:?}", attempt, e); continue; }
        };

        if resp.status() == 429 {
            eprintln!("  [judge] rate limited, backing off...");
            std::thread::sleep(std::time::Duration::from_secs(2));
            continue;
        }

        if !resp.status().is_success() {
            eprintln!("  [judge] API error {} for '{}'", resp.status(), title);
            return None;
        }

        let data: serde_json::Value = match resp.json() {
            Ok(d) => d,
            Err(e) => { eprintln!("  [judge] JSON parse error: {:?}", e); continue; }
        };

        let finish_reason = data["choices"][0]["finish_reason"].as_str().unwrap_or("unknown");
        let content = data["choices"][0]["message"]["content"].as_str();

        match content {
            Some(t) if !t.is_empty() => {
                let score: u32 = t
                    .split(|c: char| !c.is_ascii_digit())
                    .filter(|s| !s.is_empty())
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0)
                    .min(100);
                return Some(score);
            }
            Some(t) => {
                eprintln!("  [judge] empty content (finish={}) for '{}'. raw response: {}", finish_reason, title, data);
                continue;
            }
            None => {
                eprintln!("  [judge] no content field (finish={}) for '{}'. raw response: {}", finish_reason, title, data);
                return None;
            }
        }
    }
    None
}

/// Generate one title via cloud AI (DeepSeek) using the web app's prompt format.
/// Mirrors titleforge/netlify/functions/generate.js::buildPrompt + callOpenAICompatible.
fn generate_title_cloud(keyword: &str, category: &str, api_key: &str) -> Option<String> {
    let cat_label = match category {
        "book" => "books", "article" => "articles", "blog" => "blog posts",
        "youtube" => "YouTube videos", "song" => "songs", "movie" => "movies/films",
        "podcast" => "podcast episodes", "product" => "products",
        "speech" => "speeches", "newsletter" => "newsletters",
        _ => category,
    };

    let system = format!(
        "You are TitleForge — an elite title generator for authors, marketers, and creators. Generate titles that people actually click. Before you write each title, ask: 'Would I click this?' If the answer is no, replace it. Return ONLY valid JSON."
    );

    let prompt = format!(
        "Generate 1 title for a {cat_label} named \"{keyword}\".

The keyword is \"{keyword}\". Every title must be about this keyword — not about the category itself.

Communication style: normal

QUALITY RULES (these are what separate a great title from a forgettable one):
- EMOTIONAL PULL: Make the reader feel something. Curiosity, surprise, aspiration, or urgency. A title that evokes nothing is wasted.
- SPECIFICITY: Use concrete details — numbers, names, vivid specifics. \"7 Habits\" beats \"Good Habits.\" \"The $1.2 Million Typo\" beats \"An Expensive Mistake.\" Always choose the specific over the abstract.
- CURIOSITY GAP: The reader should NEED to click to satisfy an open question. If the reader can guess the full story from the title, rewrite it.
- NO FILLER: Every title must be genuinely strong.
- VARIETY: Mix structures — a question, a declaration, a numbered list, a story hook, a counterintuitive statement.
- NO CLICHÉS: Never use: \"unlock the secrets,\" \"ultimate guide,\" \"everything you need to know,\" \"game changer,\" \"mind-blowing,\" \"life-changing,\" \"revolutionize,\" \"master the art,\" \"unleash your potential,\" \"X is the new Y.\"

Respond with exactly: {{\"titles\":[{{\"title\":\"Your Title Here\",\"score\":85,\"breakdown\":{{\"curiosityGap\":\"High\",\"emotionalTrigger\":\"aspiration\",\"powerWords\":[\"word1\",\"word2\"],\"lengthAnalysis\":\"Optimal (8 words)\",\"specificity\":\"Concrete\"}}}}]}}",
        cat_label = cat_label, keyword = keyword
    );

    let body = serde_json::json!({
        "model": "deepseek-v4-flash",
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.85,
        "max_tokens": 512,
        "thinking": {"type": "disabled"},
        "response_format": {"type": "json_object"},
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build().ok()?;

    let resp = client
        .post("https://api.deepseek.com/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send().ok()?;

    let data: serde_json::Value = resp.json().ok()?;
    let content = data["choices"][0]["message"]["content"].as_str()?;
    
    // Extract the first title from the JSON response
    let parsed: serde_json::Value = {
        let cleaned = content
            .strip_prefix("```json").unwrap_or(content)
            .strip_prefix("```").unwrap_or(content)
            .strip_suffix("```").unwrap_or(content);
        serde_json::from_str(cleaned).ok()?
    };

    let title = parsed["titles"][0]["title"].as_str()?.trim().to_string();
    if title.is_empty() { None } else { Some(title) }
}

#[test]
fn benchmark_judge() {
    // ── API key: env var first, then .bench-key file, then error ──
    let api_key = std::env::var("BENCH_JUDGE_API_KEY").ok()
        .or_else(|| {
            let key_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(".bench-key");
            eprintln!("API key env var not set, trying file: {:?}", key_path);
            if key_path.exists() {
                eprintln!("  Found .bench-key file, reading key...");
                // PowerShell's `echo key > file` writes UTF-16LE with a BOM on Windows,
                // which is NOT valid UTF-8 — read_to_string silently returns Err and the
                // key looks "unset". Decode defensively: UTF-16 LE/BE, UTF-8 BOM, or plain.
                let key = std::fs::read(&key_path).ok().and_then(|bytes| {
                    let s = match bytes.as_slice() {
                        [0xFF, 0xFE, rest @ ..] => {
                            let u16s: Vec<u16> = rest.chunks_exact(2)
                                .map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                            String::from_utf16(&u16s).ok()
                        }
                        [0xFE, 0xFF, rest @ ..] => {
                            let u16s: Vec<u16> = rest.chunks_exact(2)
                                .map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                            String::from_utf16(&u16s).ok()
                        }
                        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8(rest.to_vec()).ok(),
                        other => String::from_utf8(other.to_vec()).ok(),
                    }?;
                    let t = s.trim().to_string();
                    if t.is_empty() { None } else { Some(t) }
                });
                if key.is_none() {
                    eprintln!("  .bench-key found but could not be decoded or was empty.");
                }
                key
            } else {
                eprintln!("  .bench-key file NOT found at that path");
                None
            }
        })
        .unwrap_or_else(|| {
            eprintln!("\n╔══════════════════════════════════════════════════════════════╗");
            eprintln!("║  BENCH_JUDGE_API_KEY not set                                ║");
            eprintln!("║                                                             ║");
            eprintln!("║  Option A: Run in a single PowerShell command:               ║");
            eprintln!("║    $env:BENCH_JUDGE_API_KEY='sk-...'; cargo test ...        ║");
            eprintln!("║                                                             ║");
            eprintln!("║  Option B: Put your key in ../../.bench-key (no env needed):║");
            eprintln!("║    Set-Content -Path ../../.bench-key -Value 'sk-...' `     ║");
            eprintln!("║      -Encoding utf8 -NoNewline                              ║");
            eprintln!("║    (plain `echo >` writes UTF-16 on Windows)                 ║");
            eprintln!("║                                                             ║");
            eprintln!("║  Then run:                                                  ║");
            eprintln!("║    cargo test --release benchmark_judge -- --nocapture      ║");
            eprintln!("║                                                             ║");
            eprintln!("║  Cost: ~$0.10 per full run (150 titles)                     ║");
            eprintln!("╚══════════════════════════════════════════════════════════════╝\n");
            "".to_string()
        });
    if api_key.is_empty() { return; }

    // ── Pre-flight: verify API key works before burning compute ──
    eprintln!("Verifying API key with test call...");
    let pre = call_judge("Test Title", "test", "book", &api_key);
    match pre {
        Some(s) if s > 0 => eprintln!("Pre-flight OK (test score: {})", s),
        Some(s) => panic!("Pre-flight returned score {} — API is responding but producing zeros. Check thinking mode / max_tokens / response format.", s),
        None => panic!("PRE-FLIGHT FAILED: API call returned no score at all. Check [judge] error messages above for HTTP/parse errors."),
    }

    let conn = init_db();
    let generator = titleforge_lib::title_gen::Generator::build(&conn);
    let mut cache = load_cache();

    // Load Qwen
    let mut llm = None;
    let models_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("models");
    for name in &["qwen2.5-1.5b-instruct-q4_k_m.gguf", "SmolLM2-360M-Instruct-Q4_K_M.gguf", "SmolLM2-135M-Instruct-Q4_K_M.gguf"] {
        let p = models_dir.join(name);
        if p.exists() {
            eprintln!("Loading {}...", name);
            llm = titleforge_lib::local_llm::LocalLlm::load(&p);
            if llm.is_some() { break; }
        }
    }
    let model_name = if llm.is_some() { "Qwen2.5-1.5B" } else { "none" };

    println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
    println!("║  Benchmark v2 — LLM-Judge Usability Scores — {}                ║", format!("{:<18}", model_name));
    println!("║  50 keywords × 4 engines (Cloud/Qwen/EGCG/Curated)                  ║");
    println!("║  ≥70 = Usable. <70 = Rubbish.                                      ║");
    println!("╚═══════════════════════════════════════════════════════════════════════╝\n");

    #[derive(Default)]
    struct EngineStats { count: usize, sum: u64, sum_sq: u64, usable: usize, times: Vec<f32> }
    let mut cloud_s = EngineStats::default();
    let mut qwen_s = EngineStats::default();
    let mut egcg_s = EngineStats::default();
    let mut cur_s = EngineStats::default();

    // Engine filter for sweeps: BENCH_ENGINE=qwen runs ONLY the Qwen column,
    // so a temperature sweep is one variable per run and costs no cloud calls.
    let only = std::env::var("BENCH_ENGINE").ok().filter(|s| !s.is_empty());

    let mut csv = String::from("keyword,category,engine,title,mechanical_pass,kw_literal,judge_score,usable,cached\n");

    let total = BENCH_KEYWORDS.len();
    for (i, (keyword, category)) in BENCH_KEYWORDS.iter().enumerate() {
        eprintln!("[{}/{}] {}", i + 1, total, keyword);

        // ── Cloud AI (DeepSeek — web app generation prompt) ──
        let cloud_title = if only.as_deref() != Some("qwen") {
            generate_title_cloud(keyword, category, &api_key).unwrap_or_default()
        } else { String::new() };

        // ── Qwen ──
        let qwen_title = if let Some(ref mut m) = llm {
            let ex = generator.retrieve_similar(keyword, category, 3);
            let start = Instant::now();
            let t = m.generate_one_clean(keyword, category, "normal", &ex);
            let elapsed = start.elapsed().as_secs_f32();
            qwen_s.times.push(elapsed);
            t.unwrap_or_default()
        } else { String::new() };

        // ── EGCG ──
        let egcg_title = if only.as_deref() != Some("qwen") {
            let cats = vec![category.to_string()];
            let results = generator.generate(keyword, &cats, "normal", "any", 1);
            results.first().map(|r| r.title.clone()).unwrap_or_default()
        } else { String::new() };

        // ── Curated ──
        let cur_title = if only.as_deref() != Some("qwen") {
            generator.retrieve_similar(keyword, category, 1).first().cloned().unwrap_or_default()
        } else { String::new() };

        // ── Judge each title ──
        for (engine_name, title) in &[("cloud", &cloud_title), ("qwen", &qwen_title), ("egcg", &egcg_title), ("curated", &cur_title)] {
            if only.is_some() && only.as_deref() != Some(engine_name) { continue; }
            let stats: &mut EngineStats = match *engine_name {
                "cloud" => &mut cloud_s,
                "qwen" => &mut qwen_s,
                "egcg" => &mut egcg_s,
                _ => &mut cur_s,
            };

            // Gate on READABILITY ONLY. Keyword relevance is a semantic judgement and the
            // judge rubric already penalises off-topic titles ("Deduct heavily for: not
            // on-topic for the keyword"). A string match cannot tell that "VR" means
            // "virtual reality", that "100 Workouts" is a fitness title, or that
            // "Meditate" is the verb form of "meditation" — and in the 2026-07-31 run it
            // wrongly rejected 17/50 excellent cloud titles, understating the ceiling by
            // roughly 25 points. Literal keyword presence is now advisory only.
            let mech_pass = is_readable(title);
            let kw_literal = keyword_present(title, keyword);
            let (judge_score, cached) = if mech_pass && !title.is_empty() {
                let cache_key = sha256_short(&format!("{}|{}|{}", title, keyword, category));
                match cache.get(&cache_key).and_then(|v| v.as_u64()).filter(|&s| s > 0) {
                    Some(s) => (s as u32, true),
                    None => {
                        match call_judge(title, keyword, category, &api_key) {
                            Some(s) if s > 0 => {
                                cache.insert(cache_key, serde_json::json!(s));
                                (s, false)
                            }
                            Some(s) => {
                                eprintln!("  [judge] got score 0 for '{}' — not caching", title);
                                (0, false)
                            }
                            None => {
                                eprintln!("  [judge] FAILED for '{}' — not cached", title);
                                (0, false)
                            }
                        }
                    }
                }
            } else {
                (0, false)
            };

            let usable = judge_score >= 70;
            stats.count += 1;
            stats.sum += judge_score as u64;
            stats.sum_sq += (judge_score as u64).pow(2);
            if usable { stats.usable += 1; }

            csv.push_str(&format!("\"{}\",\"{}\",\"{}\",\"{}\",{},{},{},{},{}\n",
                keyword, category, engine_name,
                title.replace('"', "'"),
                mech_pass as u8, kw_literal as u8, judge_score, usable as u8,
                cached as u8));

            // Small delay between API calls to avoid rate limits
            if !cached && judge_score > 0 {
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        }
    }

    // ── Print statistics ──
    save_cache(&cache);

    let fmt_eng = |name: &str, s: &EngineStats| {
        let n = s.count as f64;
        if n == 0.0 { return format!("{:>8} {:>8} {:>8} {:>8}   {:>8}", name, "N/A", "N/A", "N/A", "N/A"); }
        let mean = s.sum as f64 / n;
        let variance = (s.sum_sq as f64 / n) - (mean * mean);
        let std_dev = variance.max(0.0).sqrt();
        let usable_pct = (s.usable as f64 / n) * 100.0;
        format!("{:>8} {:>8.1} {:>8.1} {:>8.1}%  {:>8.1}",
            name, mean, std_dev, usable_pct,
            if name == "Qwen" && !s.times.is_empty() { s.times.iter().sum::<f32>() / s.times.len() as f32 } else { 0.0 })
    };

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  JUDGE RESULTS — \"Would a creator publish this without editing?\"   ║");
    println!("╠══════════════════════════════════════════════════════════════════════╣");
    println!("║  Engine     Mean     StdDev   %Usable    AvgTime                   ║");
    println!("╠══════════════════════════════════════════════════════════════════════╣");
    println!("║  {} ║", fmt_eng("Cloud", &cloud_s));
    println!("║  {} ║", fmt_eng("Qwen", &qwen_s));
    println!("║  {} ║", fmt_eng("EGCG", &egcg_s));
    println!("║  {} ║", fmt_eng("Curated", &cur_s));
    println!("╚══════════════════════════════════════════════════════════════════════╝\n");

    println!("Usability threshold: ≥70 = usable. <70 = not publishable.");
    println!();

    // Write CSV
    let csv_path = csv_path();
    std::fs::write(&csv_path, &csv).expect("write CSV");
    eprintln!("Results written to {:?}", csv_path);
    eprintln!("Cache saved to {:?}", cache_path());

    // ── Assertions: validate the benchmark actually produced results ──
    assert!(cloud_s.sum > 0 || qwen_s.sum > 0 || egcg_s.sum > 0 || cur_s.sum > 0,
        "ALL judge scores are 0 across all engines — API calls are failing silently. Check [judge] errors above.");
    assert!(qwen_s.usable + egcg_s.usable + cur_s.usable > 0,
        "ZERO usable titles found (threshold ≥70). Either the engines produce garbage or the judge is broken.");
}
