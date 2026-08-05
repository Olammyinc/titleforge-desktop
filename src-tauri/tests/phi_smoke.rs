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

    // HARNESS CHECK BEFORE ANY QUALITY CLAIM (brief hard rule #1, and the
    // explicit warning in PHI-3.5-MIGRATION.md §3.3). generate_chat_raw builds
    // its prompt via model.chat_template() + apply_chat_template(). If a GGUF
    // carries no template, that returns Err and generation silently produces
    // NOTHING — which looks identical to "the model is bad". The first run of
    // this harness reported 1/3 success with no template check; that is the
    // signature of a format mismatch, so verify it before blaming the model.
    match llm.debug_prompt_tokens("You are a title generator.", "Write one book title about coffee.") {
        Some(n) => eprintln!("[phi_smoke] CHAT TEMPLATE OK — prompt tokenised to {} tokens", n),
        None => {
            eprintln!("[phi_smoke] *** CHAT TEMPLATE MISSING/UNUSABLE ***");
            eprintln!("[phi_smoke] Every 'failure' below is a HARNESS defect, not a model verdict.");
            eprintln!("[phi_smoke] Do NOT record a quality or speed conclusion from this run.");
        }
    }

    // Build mode matters more than anything else here: llama.cpp inference in a
    // debug build is routinely 5-20x slower, which is the same magnitude as the
    // '12x slower than Qwen' figure this harness produced.
    #[cfg(debug_assertions)]
    eprintln!("[phi_smoke] *** DEBUG BUILD — timings are MEANINGLESS. Re-run with --release. ***");
    #[cfg(not(debug_assertions))]
    eprintln!("[phi_smoke] release build — timings are valid");

    let tests: &[(&str, &str)] = &[
        ("laptop", "product"),
        ("self-help", "book"),
        ("heartbreak", "song"),
    ];

    // FAIR-CONDITIONS CASES. The three cases above run with NO few-shot
    // examples and with single-word/hyphenated keywords. Both are unlike
    // production and both bias the result against the model:
    //   * production always supplies exemplars (retrieve_similar, then
    //     fetch_top_appeal_fewshot as a fallback) — never an empty slice.
    //   * `curated_is_relevant` requires a >=4-char keyword word IN the title,
    //     so "self-help" demands "self"/"help" and "heartbreak" demands
    //     "heartbreak". A good book or song title legitimately contains
    //     neither, so the drift guard rejects correct output. CONTEXT.md
    //     records this as a known limitation for single-word keywords.
    // Without these controls a QC rejection is indistinguishable from "the
    // model cannot generate" — which is the claim this harness is used to make.
    let fair_examples: Vec<String> = vec![
        "The Quiet Revolution".to_string(),
        "Atomic Habits".to_string(),
        "Deep Work".to_string(),
    ];
    let fair: &[(&str, &str)] = &[("remote work", "blog"), ("coffee", "product")];

    let mut failures = 0u32;
    let total_start = Instant::now();

    for (kw, cat) in fair {
        let start = Instant::now();
        let r = llm.generate_one_clean(kw, cat, "normal", "any", &fair_examples, None, &Default::default());
        eprintln!("[phi_smoke] FAIR {:>6.2}s  {} {:>12}  {}",
            start.elapsed().as_secs_f64(), cat, kw,
            r.unwrap_or_else(|| "NONE".to_string()));
    }

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
