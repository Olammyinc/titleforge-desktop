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
        let mut all_tokens = prompt_ids.clone();

        // Prefill: feed the full prompt at position 0
        let input = Tensor::from_vec(prompt_ids, (1, prompt_len), &self.device).ok()?;
        let logits = self.model.forward(&input, 0).ok()?;
        let next = sample_token(&logits).ok()?;
        if next == eos || next == eos2 { return None; }
        all_tokens.push(next);

        // Decode: feed one token at a time
        for _step in 0..49usize {
            let input = Tensor::from_vec(vec![next], (1, 1), &self.device).ok()?;
            let logits = self.model.forward(&input, all_tokens.len() as usize - 1).ok()?;
            let next = sample_token(&logits).ok()?;
            if next == eos || next == eos2 { break; }
            all_tokens.push(next);
        }

        // Decode only the newly generated tokens
        let output = self.tokenizer.decode(&all_tokens[prompt_len..], true).ok()?;
        let trimmed = output.trim().to_string();

        // QC gate
        if trimmed.len() < 5 || trimmed.split_whitespace().count() < 3 {
            return None;
        }

        Some(trimmed)
    }
}

fn sample_token(logits: &Tensor) -> Result<u32, candle_core::Error> {
    // logits shape: [1, seq_len, vocab] — take the last position
    let seq_len = logits.dim(1)?;
    let logits = logits.i((0, seq_len - 1))?; // [vocab]
    let temperature = 0.7f64;
    let top_p = 0.9f32;
    let logits = (&logits / temperature)?;
    let probs = candle_nn::ops::softmax(&logits, 0)?; // softmax over vocab
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
