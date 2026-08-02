/// Task 0 verification: sampling must be non-deterministic.
/// Same keyword + category generated twice must produce DIFFERENT titles.
/// This is what unblocks batch generation (25/100/500 titles per request).

use titleforge_lib::local_llm::LocalLlm;

#[test]
fn qwen_non_deterministic() {
    let model_path = std::path::Path::new("../models/qwen2.5-1.5b-instruct-q4_k_m.gguf");
    if !model_path.exists() {
        eprintln!("[nondet] Qwen model not found — skipping");
        return;
    }
    let mut llm = match LocalLlm::load(model_path) {
        Some(l) => l,
        None => { eprintln!("[nondet] Failed to load Qwen"); return; }
    };
    let examples: Vec<String> = vec![];

    // Generate the same keyword twice (plus a couple of others to sample more).
    let kw = "coffee";
    let cat = "youtube";
    let mut titles = Vec::new();
    for i in 0..5 {
        match llm.generate_one_clean(kw, cat, "normal", &examples, None) {
            Some(t) => {
                eprintln!("[nondet] run {}: '{}'", i + 1, t);
                titles.push(t);
            }
            None => eprintln!("[nondet] run {}: FAILED", i + 1),
        }
    }

    let unique: std::collections::HashSet<&String> = titles.iter().collect();
    eprintln!("[nondet] {} generated, {} unique", titles.len(), unique.len());
    assert!(
        unique.len() >= 2,
        "Deterministic generation: same keyword produced {} title(s) — batch generation impossible. Expected >= 2 distinct titles.",
        unique.len()
    );
    eprintln!("[nondet] PASS — sampling is non-deterministic");
}
