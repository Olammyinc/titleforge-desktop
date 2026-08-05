/// Phi-3.5-mini-instruct smoke test.
/// Reads model path from TF_MODEL_PATH env var. Fails clearly if unset or missing.
/// Generates 3 fixed titles (product, book, song), one attempt each via generate_one_clean.
/// Prints per-title elapsed seconds and total. Exits success even if a title returns None.

use std::time::Instant;

#[test]
fn phi_smoke() {
    let model_path = std::env::var("TF_MODEL_PATH")
        .expect("TF_MODEL_PATH not set - set TF_MODEL_PATH=/path/to/phi.gguf");

    let path = std::path::PathBuf::from(&model_path);
    assert!(path.exists(), "Model file not found: {}", path.display());

    eprintln!("[phi_smoke] Loading model from {} ...", path.display());
    let load_start = Instant::now();
    let mut llm = match titleforge_lib::local_llm::LocalLlm::load(&path) {
        Some(l) => l,
        None => panic!("Failed to load model from {}", path.display()),
    };
    eprintln!("[phi_smoke] Loaded in {:.1}s", load_start.elapsed().as_secs_f64());

    let tests: &[(&str, &str)] = &[
        ("laptop", "product"),
        ("self-help", "book"),
        ("heartbreak", "song"),
    ];

    let mut failures = 0u32;
    let total_start = Instant::now();

    for (kw, cat) in tests {
        let start = Instant::now();
        let result =
            llm.generate_one_clean(kw, cat, "normal", "any", &[], None, &Default::default());
        let elapsed = start.elapsed().as_secs_f64();

        match result {
            Some(title) => {
                eprintln!("[phi_smoke] {:>6.2}s  {} {:>8}  {}", elapsed, cat, kw, title);
            }
            None => {
                eprintln!("[phi_smoke] {:>6.2}s  {} {:>8}  NONE", elapsed, cat, kw);
                failures += 1;
            }
        }
    }

    let total = total_start.elapsed().as_secs_f64();
    eprintln!("[phi_smoke] Done. {}/3 succeeded, {} failed. Total: {:.1}s", 3 - failures, failures, total);
}
