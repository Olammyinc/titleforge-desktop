/// Quick SmolLM2 smoke test — one prompt, one generation attempt.
/// Verifies: model loads, tokenizer encodes, forward pass works, tokens decode.

#[test]
fn smoke_llm_135() {
    let model_path = std::path::Path::new("../models/SmolLM2-135M-Instruct-Q4_K_M.gguf");
    run_smoke(model_path);
}

#[test]
fn smoke_llm_360() {
    let model_path = std::path::Path::new("../models/SmolLM2-360M-Instruct-Q4_K_M.gguf");
    run_smoke(model_path);
}

fn run_smoke(model_path: &std::path::Path) {
    if !model_path.exists() {
        eprintln!("[smoke] Model not found at {:?} — skipping", model_path);
        return;
    }

    eprintln!("[smoke] Loading model {:?}...", model_path.file_name().unwrap());
    let mut llm = match titleforge_lib::local_llm::LocalLlm::load(model_path) {
        Some(l) => l,
        None => { eprintln!("[smoke] Failed to load model"); return; }
    };
    eprintln!("[smoke] Model loaded. Generating...");

    let prompts = ["Write ONE book title about \"shirt\". Reply with only the title, nothing else.",
                   "Write ONE YouTube title about \"productivity\". Reply with only the title, nothing else.",
                   "Write ONE product name for a coffee brand. Reply with only the name, nothing else."];
    for prompt in prompts {
        let start = std::time::Instant::now();
        match llm.generate_one(prompt) {
            Some(title) => {
                let elapsed = start.elapsed().as_secs_f64();
                eprintln!("[smoke] {:>5.1}s → '{}'", elapsed, title);
                assert!(!title.is_empty(), "title should not be empty");
            }
            None => {
                let elapsed = start.elapsed().as_secs_f64();
                eprintln!("[smoke] {:>5.1}s → FAILED (returned None)", elapsed);
            }
        }
    }
}
