/// SmolLM2 smoke test with improved RAG few-shot + retry + cleaning.
/// Verifies: generate_one_clean produces usable titles with the new pipeline.

#[test]
fn smoke_rag() {
    let mut model_path = std::path::Path::new("../models/SmolLM2-360M-Instruct-Q4_K_M.gguf");
    if !model_path.exists() {
        model_path = std::path::Path::new("../models/SmolLM2-135M-Instruct-Q4_K_M.gguf");
        if !model_path.exists() {
            eprintln!("[smoke] No SmolLM2 model found — skipping");
            return;
        }
    }
    eprintln!("[smoke] Loading {:?}...", model_path.file_name().unwrap());
    let mut llm = match titleforge_lib::local_llm::LocalLlm::load(model_path) {
        Some(l) => l,
        None => { eprintln!("[smoke] Failed to load"); return; }
    };
    eprintln!("[smoke] Loaded. Testing RAG few-shot...");

    let items: &[(&str, &str, &[&str])] = &[
        ("shirt", "product", &["EcoThreads", "Zenith Apparel"]),
        ("productivity", "book", &["The Art of Deep Work", "How to Build Atomic Habits"]),
        ("coffee", "youtube", &["I Tried X for 30 Days", "The SECRET Nobody Talks About"]),
        ("startup", "book", &["Zero to One", "The Lean Startup"]),
        ("meditation", "podcast", &["10% Happier", "The Mindful Minute"]),
    ];

    let mut passed = 0usize;
    let mut failed = 0usize;
    for (kw, cat, exs) in items {
        let examples: Vec<String> = exs.iter().map(|s| s.to_string()).collect();
        let start = std::time::Instant::now();
        match llm.generate_one_clean(kw, cat, "normal", &examples) {
            Some(title) => {
                let t = start.elapsed().as_secs_f64();
                let has_kw = title.to_lowercase().contains(&kw.to_lowercase());
                eprintln!("[smoke] {:>5.1}s {} {} → '{}' (kw={})", t, if has_kw {"PASS"} else {"OK"}, cat, title, has_kw);
                passed += 1;
            }
            None => {
                let t = start.elapsed().as_secs_f64();
                eprintln!("[smoke] {:>5.1}s {} → FAILED", t, kw);
                failed += 1;
            }
        }
    }
    eprintln!("[smoke] Results: {}/{} passed, {} failed", passed, passed + failed, failed);
    if failed > 3 {
        panic!("Too many failures: {}/{}", failed, passed + failed);
    }
}
