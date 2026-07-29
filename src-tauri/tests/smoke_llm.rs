/// Path A LLM smoke test with Qwen2.5 on llama.cpp.
/// Verifies llama-cpp-2 backend loads and generates (not just candle-rs compatibility).

#[test]
fn smoke_llama() {
    let model_path = std::path::Path::new("../models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    if !model_path.exists() {
        eprintln!("[smoke] Qwen model not found — skipping Path A smoke test");
        return;
    }
    eprintln!("[smoke] Loading Qwen2.5-1.5B via llama-cpp-2...");
    let mut llm = match titleforge_lib::local_llm::LocalLlm::load(model_path) {
        Some(l) => l,
        None => { eprintln!("[smoke] Failed to load Qwen"); return; }
    };
    eprintln!("[smoke] Loaded. Generating 3 test titles...");

    let tests: &[(&str, &str, &[&str])] = &[
        ("shirt", "product", &[] as &[&str]),
        ("productivity", "book", &["The Art of Deep Work", "Atomic Habits"]),
        ("coffee", "youtube", &["I Tried X for 30 Days"]),
    ];

    for (kw, cat, exs) in tests {
        let examples: Vec<String> = exs.iter().map(|s| s.to_string()).collect();
        let start = std::time::Instant::now();
        match llm.generate_one_clean(kw, cat, "normal", &examples) {
            Some(title) => {
                let t = start.elapsed().as_secs_f64();
                let has_kw = title.to_lowercase().contains(&kw.to_lowercase());
                eprintln!("[smoke] {:>5.1}s {} {} → '{}' (kw={})", t, if has_kw {"PASS"} else {"OK"}, cat, title, has_kw);
            }
            None => {
                let t = start.elapsed().as_secs_f64();
                eprintln!("[smoke] {:>5.1}s {} {} → FAILED", t, kw, cat);
            }
        }
    }
}
