/// Diagnostic: why does Qwen stay silent on 46% of keywords?
///
/// Runs the 23 keywords Qwen produced nothing for in the 2026-07-31 benchmark,
/// calls the REAL generation path, and reports the raw model output plus which
/// filter in `generate_one_clean` would have rejected it.
///
/// Read-only — changes no production behaviour.
///
/// Usage:
///   cargo test --release diag_qwen_silence -- --nocapture

use std::path::Path;
use rusqlite::Connection;

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

// ── Mirrors of the private filters in local_llm.rs (kept in sync manually) ──

fn is_echo_line(lower: &str) -> bool {
    ["here", "i would", "sure", "let me", "title:", "here is", "here's",
     "i'm", "i can", "i think", "please", "certainly",
     "of course", "i am", "note:", "based on", "using the"]
        .iter().any(|e| lower.starts_with(e))
}

fn is_instruction_echo(lower: &str) -> bool {
    lower.contains("title:") || lower.contains("reply with") || lower.contains("one title")
        || lower.contains("example") || lower.starts_with("write")
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
fn diag_qwen_silence() {
    let conn = init_db();
    let generator = titleforge_lib::title_gen::Generator::build(&conn);

    let models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("models");
    let mut llm = None;
    for name in &["qwen2.5-1.5b-instruct-q4_k_m.gguf"] {
        let p = models_dir.join(name);
        if p.exists() { llm = titleforge_lib::local_llm::LocalLlm::load(&p); }
    }
    let mut llm = match llm {
        Some(m) => m,
        None => { eprintln!("Qwen model not found — skipping diagnostic."); return; }
    };

    println!("\n╔═══════════════════════════════════════════════════════════════════╗");
    println!("║  Qwen silence diagnostic — 23 keywords that produced nothing      ║");
    println!("╚═══════════════════════════════════════════════════════════════════╝\n");

    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    let mut recovered = 0usize;

    for (kw, cat) in SILENT {
        let examples = generator.retrieve_similar(kw, cat, 3);

        // Call the real public path first — does it still fail?
        let real = llm.generate_one_clean(kw, cat, "normal", "any", &examples, None, &Default::default());

        if let Some(t) = &real {
            recovered += 1;
            println!("  RECOVERED  {:22} -> {}", kw, t);
            *counts.entry("now succeeds (run variance)").or_default() += 1;
            continue;
        }

        // Still fails. Reproduce the pipeline to find the blocking filter.
        // Use the same prompt shape as attempt 1.
        let system = format!(
            "You are TitleForge, an elite title generator. Generate ONE creative, clickable {} title about \"{}\". CRITICAL RULE: the title MUST contain the word \"{}\" somewhere in it. Output ONLY the title text — no explanation, no preamble, no markdown, no quotes.",
            cat, kw, kw
        );
        let mut user_prompt = String::new();
        if !examples.is_empty() {
            user_prompt.push_str(&format!("Examples of normal {} titles:\n", cat));
            for ex in examples.iter().take(3) { user_prompt.push_str(&format!("- \"{}\"\n", ex)); }
            user_prompt.push('\n');
        }
        user_prompt.push_str(&format!(
            "Write a normal {} title. The word \"{}\" MUST appear in the title. 3-15 words, creative, clickable.",
            cat, kw
        ));

        let raw = llm.debug_raw(&system, &user_prompt);
        let raw_s = raw.clone().unwrap_or_default();

        // Walk the filters in production order.
        let reason = if raw.is_none() || raw_s.trim().is_empty() {
            "1. model returned nothing"
        } else {
            // clean_output equivalent
            let mut cleaned = String::new();
            let text = raw_s.trim();
            let text = text.strip_prefix("```json").unwrap_or(text).strip_prefix("```").unwrap_or(text);
            let text = text.strip_suffix("```").unwrap_or(text);
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() { continue; }
                if let Some(cp) = trimmed.find(':') {
                    if is_echo_line(&trimmed[..cp].to_lowercase()) {
                        let after = trimmed[cp + 1..].trim();
                        if after.len() >= 3 { cleaned = after.to_string(); break; }
                        continue;
                    }
                }
                if !is_echo_line(&trimmed.to_lowercase()) { cleaned = trimmed.to_string(); break; }
            }

            if cleaned.is_empty() {
                "2. clean_output ate every line (is_echo_line)"
            } else if cleaned.len() < 3 || cleaned.split_whitespace().count() < 2 {
                "3. too short (<3 chars or <2 words)"
            } else {
                let cl = cleaned.to_lowercase();
                let kw_lower = kw.to_lowercase();
                let kw_tokens: Vec<&str> = kw_lower.split_whitespace().collect();
                let keyword_ok = cl.contains(&kw_lower)
                    || kw_tokens.iter().any(|w| cl.contains(w));
                if !keyword_ok {
                    "4. LITERAL KEYWORD MISSING"
                } else if is_instruction_echo(&cl) {
                    "5. is_instruction_echo"
                } else {
                    "6. passed here but failed in prod (retry/dup)"
                }
            }
        };

        *counts.entry(reason).or_default() += 1;
        let preview: String = raw_s.chars().take(70).collect();
        println!("  {:46} {:20} raw: {}", reason, kw, preview.replace('\n', " ⏎ "));
    }

    println!("\n╔═══════════════════════════════════════════════════════════════════╗");
    println!("║  REJECTION BREAKDOWN                                              ║");
    println!("╚═══════════════════════════════════════════════════════════════════╝");
    for (reason, n) in &counts {
        println!("  {:3}  {}", n, reason);
    }
    println!("\n  {} of {} recovered on this run (stochastic variance)", recovered, SILENT.len());
}
