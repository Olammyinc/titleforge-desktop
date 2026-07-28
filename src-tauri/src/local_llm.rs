use std::path::Path;
use candle_core::{Device, Tensor, IndexOp};
use tokenizers::Tokenizer;

pub struct LocalLlm {
    model: candle_transformers::models::quantized_llama::ModelWeights,
    tokenizer: Tokenizer,
    device: Device,
    pub loaded: bool,
}

impl LocalLlm {
    pub fn load(model_path: &Path) -> Option<Self> {
        let device = Device::Cpu;
        let model_dir = model_path.parent()?;
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !model_path.exists() {
            eprintln!("[local_llm] Model file not found: {:?}", model_path);
            return None;
        }
        if !tokenizer_path.exists() {
            eprintln!("[local_llm] Tokenizer not found: {:?}", tokenizer_path);
            return None;
        }

        let tokenizer = match Tokenizer::from_file(&tokenizer_path) {
            Ok(t) => t,
            Err(e) => { eprintln!("[local_llm] Tokenizer load failed: {}", e); return None; }
        };

        eprintln!("[local_llm] Loading model from {:?}...", model_path);
        let mut file = match std::fs::File::open(model_path) {
            Ok(f) => f,
            Err(e) => { eprintln!("[local_llm] Failed to open model file: {}", e); return None; }
        };

        let content = match candle_core::quantized::gguf_file::Content::read(&mut file) {
            Ok(c) => c,
            Err(e) => { eprintln!("[local_llm] Failed to read GGUF content: {}", e); return None; }
        };

        let model = match candle_transformers::models::quantized_llama::ModelWeights::from_gguf(content, &mut file, &device) {
            Ok(m) => m,
            Err(e) => { eprintln!("[local_llm] Failed to load model weights: {}", e); return None; }
        };

        eprintln!("[local_llm] Model loaded successfully");
        Some(Self { model, tokenizer, device, loaded: true })
    }

    /// Generate one title from a prompt.
    /// Uses separate prefill and decode steps for correct KV cache usage.
    pub fn generate_one(&mut self, prompt: &str) -> Option<String> {
        // Build chat-formatted prompt matching SmolLM2-Instruct template
        let full_prompt = format!(
            "<|im_start|>system\nYou are TitleForge, an elite title generator. Generate exactly one title — no explanation, no preamble.<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
            prompt
        );

        let encoded = self.tokenizer.encode(full_prompt.as_str(), true).ok()?;
        let prompt_ids = encoded.get_ids().to_vec();
        let prompt_len = prompt_ids.len();
        let eos = self.tokenizer.token_to_id("<|im_end|>").unwrap_or(u32::MAX);
        let eos2 = self.tokenizer.token_to_id("<|endoftext|>").unwrap_or(u32::MAX);
        let bos = self.tokenizer.token_to_id("<|im_start|>").unwrap_or(u32::MAX);
        eprintln!("[local_llm] prompt_len={} eos={} eos2={} bos={}", prompt_len, eos, eos2, bos);
        let mut all_tokens = prompt_ids.clone();

        // Prefill: feed the full prompt at position 0
        let input = match Tensor::from_vec(prompt_ids, (1, prompt_len), &self.device) {
            Ok(t) => t,
            Err(e) => { eprintln!("[local_llm] Tensor::from_vec failed: {:?}", e); return None; }
        };
        let logits = match self.model.forward(&input, 0) {
            Ok(l) => l,
            Err(e) => { eprintln!("[local_llm] model.forward failed: {:?}", e); return None; }
        };
        let mut next = match sample_token(&logits) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[local_llm] sample_token failed: {:?}, logits shape={:?}", e, logits.dims());
                // Try fallback: argmax token
                match logits.i((0, logits.dim(1).unwrap_or(1) - 1)) {
                    Ok(last) => {
                        let flat: Vec<f32> = last.to_vec1().unwrap_or_default();
                        if flat.is_empty() { return None; }
                        flat.iter().enumerate().max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)).map(|(i, _)| i as u32).unwrap_or(0)
                    }
                    Err(e2) => { eprintln!("[local_llm] fallback argmax also failed: {:?}", e2); return None; }
                }
            }
        };
        eprintln!("[local_llm] first generated token: {} (eos={} eos2={} match_eos={} match_eos2={})",
            next, eos, eos2, next == eos, next == eos2);
        if next == eos || next == eos2 {
            eprintln!("[local_llm] EOS as first token — model gave up immediately");
            return None;
        }
        all_tokens.push(next);

        // Decode: feed one token at a time. `next` must be reassigned (not
        // shadowed with `let`) each iteration — a `let next = ...` here would
        // only live for that single loop iteration, silently leaving every
        // decode step after the first feeding the same first-generated token
        // back into the model instead of the token it just produced, which
        // breaks autoregressive generation entirely.
        for _step in 0..49usize {
            let input = Tensor::from_vec(vec![next], (1, 1), &self.device).ok()?;
            let logits = self.model.forward(&input, all_tokens.len() as usize - 1).ok()?;
            next = sample_token(&logits).ok()?;
            if next == eos || next == eos2 { break; }
            all_tokens.push(next);
        }

        // Decode only the newly generated tokens
        let output = self.tokenizer.decode(&all_tokens[prompt_len..], true).ok()?;
        let trimmed = output.trim().to_string();

        eprintln!("[local_llm] raw output ({} chars, {} words): '{}'",
            trimmed.len(), trimmed.split_whitespace().count(), trimmed);

        // QC gate — accepts even single-word output during development
        if trimmed.is_empty() {
            eprintln!("[local_llm] QC rejected (empty output)");
            return None;
        }

        Some(trimmed)
    }
}

fn sample_token(logits: &Tensor) -> Result<u32, candle_core::Error> {
    // logits shape: [1, vocab] (prefill) or [1, 1, vocab] (decode)
    // Always take the last position in the sequence.
    let ndim = logits.dims().len();
    let vocab_logits = if ndim == 2 {
        // [batch, vocab] — prefill return
        logits.i(0)?
    } else if ndim >= 3 {
        // [batch, seq_len, vocab] — decode, take last position
        let seq_len = logits.dim(ndim - 2)?;
        logits.i((0, seq_len - 1))?
    } else {
        return Err(candle_core::Error::Msg(format!("Unexpected logits shape: {:?}", logits.dims())));
    };
    let temperature = 0.7f64;
    let top_p = 0.9f32;
    let logits_scaled = (&vocab_logits / temperature)?;
    let probs = candle_nn::ops::softmax(&logits_scaled, 0)?;
    let probs_vec: Vec<f32> = probs.to_vec1()?;

    // Top-p (nucleus) sampling
    let mut sorted: Vec<(usize, f32)> = probs_vec.iter().enumerate().map(|(i, p)| (i, *p)).collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut cum = 0.0f32;
    let mut candidates: Vec<(usize, f32)> = Vec::new();
    for &(idx, p) in &sorted {
        cum += p;
        candidates.push((idx, p));
        if cum >= top_p { break; }
    }

    if candidates.is_empty() { return Ok(probs_vec.len() as u32 - 1); }

    let total: f32 = candidates.iter().map(|(_, p)| p).sum();
    if total <= 0.0 { return Ok(candidates[0].0 as u32); }

    use rand::Rng;
    let mut rng = rand::thread_rng();
    let r: f32 = rng.gen::<f32>() * total;
    let mut c = 0.0f32;
    for (idx, p) in &candidates {
        c += p;
        if r <= c { return Ok(*idx as u32); }
    }
    Ok(candidates.last().map(|(i, _)| *i as u32).unwrap_or(0))
}
