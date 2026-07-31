/// Diagnostic: are Qwen's silent keywords caused by context-window overflow?
///
/// `generate_chat_raw` uses `LlamaContextParams::default()`, which leaves n_ctx
/// at llama.cpp's default (512). The prompt is system + up to 3 few-shot
/// examples + instruction; `ctx.decode()` fails via `.ok()?` when
/// n_prompt + max_new exceeds the window, producing silent failure.
///
/// Tokenises only — no generation, so this runs in seconds.
///
/// Usage:
///   cargo test --release diag_prompt_len -- --nocapture

use std::path::Path;
use rusqlite::Connection;

const MAX_NEW: usize = 60;
const ASSUMED_N_CTX: usize = 512;

// The 23 keywords Qwen produced nothing for, plus 6 it succeeded on (controls).
const SILENT: &[(&str, &str)] = &[
    ("laptop", "product"), ("fitness", "youtube"), ("travel", "blog"),
    ("meditation", "podcast"), ("investing", "article"), ("photography", "youtube"),
    ("music", "song"), ("AI", "article"), ("creativity", "book"),
    ("negotiation", "book"), ("writing", "book"), ("marketing", "article"),
    ("data science", "article"), ("blockchain", "article"), ("electric cars", "youtube"),
    ("podcasting", "blog"), ("gaming", "youtube"), ("intermittent fasting", "book"),
    ("bitcoin", "article"), ("tennis", "youtube"), ("board games", "product"),
    ("solo travel", "blog"), ("home office", "product"),
];

const SUCCEEDED: &[(&str, &str)] = &[
    ("coffee", "product"), ("parenting", "book"), ("wine", "blog"),
    ("yoga", "youtube"), ("jazz", "song"), ("climate change", "article"),
];

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

fn build_prompts(kw: &str, cat: &str, examples: &[String]) -> (String, String) {
    let system = format!(
        "You are TitleForge, an elite title generator. Generate ONE creative, clickable {} title about \"{}\". CRITICAL RULE: the title MUST contain the word \"{}\" somewhere in it. Output ONLY the title text — no explanation, no preamble, no markdown, no quotes.",
        cat, kw, kw
    );
    let mut user = String::new();
    if !examples.is_empty() {
        user.push_str(&format!("Examples of normal {} titles:\n", cat));
        for ex in examples.iter().take(3) { user.push_str(&format!("- \"{}\"\n", ex)); }
        user.push('\n');
    }
    user.push_str(&format!(
        "Write a normal {} title. The word \"{}\" MUST appear in the title. 3-15 words, creative, clickable.",
        cat, kw
    ));
    (system, user)
}

#[test]
fn diag_prompt_len() {
    let conn = init_db();
    let generator = titleforge_lib::title_gen::Generator::build(&conn);

    let models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("models");
    let p = models_dir.join("qwen2.5-1.5b-instruct-q4_k_m.gguf");
    let llm = match titleforge_lib::local_llm::LocalLlm::load(&p) {
        Some(m) => m,
        None => { eprintln!("Qwen model not found — skipping."); return; }
    };

    println!("\n╔════════════════════════════════════════════════════════════════════╗");
    println!("║  Prompt length vs context window (assumed n_ctx = {})           ║", ASSUMED_N_CTX);
    println!("║  needed = n_prompt + max_new({})                                  ║", MAX_NEW);
    println!("╚════════════════════════════════════════════════════════════════════╝\n");

    let mut over_silent = 0usize;
    let mut over_ok = 0usize;

    for (label, set) in [("SILENT", SILENT), ("SUCCEEDED", SUCCEEDED)] {
        println!("── {} ──", label);
        for (kw, cat) in set {
            let examples = generator.retrieve_similar(kw, cat, 3);
            let (sys, usr) = build_prompts(kw, cat, &examples);
            let n = llm.debug_prompt_tokens(&sys, &usr).unwrap_or(0);
            let needed = n + MAX_NEW;
            let over = needed > ASSUMED_N_CTX;
            if over {
                if label == "SILENT" { over_silent += 1; } else { over_ok += 1; }
            }
            println!("  {:22} ex={} n_prompt={:4} needed={:4} {}",
                kw, examples.len().min(3), n, needed,
                if over { "*** OVER ***" } else { "ok" });
        }
        println!();
    }

    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║  RESULT                                                            ║");
    println!("╚════════════════════════════════════════════════════════════════════╝");
    println!("  SILENT keywords over the window:    {}/{}", over_silent, SILENT.len());
    println!("  SUCCEEDED keywords over the window: {}/{}", over_ok, SUCCEEDED.len());
    println!();
    if over_silent > SILENT.len() / 2 && over_ok == 0 {
        println!("  => CONFIRMED: context overflow explains the silence.");
    } else if over_silent == 0 {
        println!("  => RULED OUT: prompts fit. Silence has another cause.");
    } else {
        println!("  => PARTIAL: overflow explains some but not all.");
    }
}
