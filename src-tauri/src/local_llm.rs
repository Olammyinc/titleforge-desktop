use std::path::Path;
use candle_core::{Device, Tensor};
use tokenizers::Tokenizer;

pub struct LocalLlm {
    model: std::cell::RefCell<candle_transformers::models::quantized_llama::ModelWeights>,
    tokenizer: Tokenizer,
    device: Device,
    pub loaded: bool,
}

impl LocalLlm {
    pub fn load(model_path: &Path) -> Option<Self> {
        let device = Device::Cpu;
        let model_dir = model_path.parent()?;
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !model_path.exists() || !tokenizer_path.exists() {
            eprintln!("[local_llm] Model or tokenizer not found");
            return None;
        }

        let tokenizer = Tokenizer::from_file(&tokenizer_path).ok()?;
        eprintln!("[local_llm] Loading model...");

        let mut file = std::fs::File::open(model_path).ok()?;
        let content = candle_core::quantized::gguf_file::Content::read(&mut file).ok()?;
        let model = candle_transformers::models::quantized_llama::ModelWeights::from_gguf(content, &mut file, &device).ok()?;
        eprintln!("[local_llm] Model loaded");

        Some(Self { model: std::cell::RefCell::new(model), tokenizer, device, loaded: true })
    }

    pub fn generate_one(&self, prompt: &str) -> Option<String> {
        let encoded = self.tokenizer.encode(prompt, true).ok()?;
        let input_ids = encoded.get_ids();
        let mut all_tokens = input_ids.to_vec();
        let eos = self.tokenizer.token_to_id("<|im_end|>").unwrap_or(0);
        let eos2 = self.tokenizer.token_to_id("<|endoftext|>").unwrap_or(0);

        for step in 0..50usize {
            let input = Tensor::new(all_tokens.as_slice(), &self.device).ok()?.unsqueeze(0).ok()?;
            let mut model = self.model.borrow_mut();
            let logits = model.forward(&input, all_tokens.len()).ok()?;
            let next = sample_token(&logits, step as u64).ok()?;
            if next == eos || next == eos2 { break; }
            all_tokens.push(next);
        }

        let output = self.tokenizer.decode(&all_tokens[input_ids.len()..], true).ok()?;
        let trimmed = output.trim().to_string();

        // QC gate: reject empty, too short, or verbatim prompt echo
        if trimmed.len() < 5 || trimmed.split_whitespace().count() < 3 {
            return None;
        }
        // Check if model just echoed the prompt
        let prompt_stripped = prompt.trim_end_matches(|c: char| !c.is_alphanumeric());
        if trimmed.to_lowercase() == prompt_stripped.to_lowercase() {
            return None;
        }

        Some(trimmed)
    }
}

fn sample_token(logits: &Tensor, seed: u64) -> Result<u32, candle_core::Error> {
    use rand::Rng;
    let logits = logits.squeeze(0)?;
    let temperature = 0.7f64;
    let top_p = 0.9f32;
    let logits = (&logits / temperature)?;
    let probs = candle_nn::ops::softmax(&logits, 0)?;
    let probs_vec: Vec<f32> = probs.to_vec1()?;

    // Top-p filtering
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

    let mut rng: rand::rngs::StdRng = rand::SeedableRng::seed_from_u64(seed);
    let r: f32 = rng.gen::<f32>() * total;
    let mut c = 0.0f32;
    for (idx, p) in &candidates {
        c += p;
        if r <= c { return Ok(*idx as u32); }
    }
    Ok(candidates.last().map(|(i, _)| *i as u32).unwrap_or(0))
}
