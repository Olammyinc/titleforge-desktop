use std::path::Path;
use std::collections::HashSet;
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

        // Try model-appropriate tokenizer. SmolLM2 tokenizer first (default),
        // then fall back to Qwen/Llama tokenizers if present.
        let tokenizer = ["tokenizer.json", "tokenizer_qwen.json", "tokenizer_llama.json"]
            .iter()
            .find_map(|name| {
                let p = model_dir.join(name);
                if p.exists() { Tokenizer::from_file(&p).ok() } else { None }
            })?;

        if !model_path.exists() {
            eprintln!("[local_llm] Model file not found: {:?}", model_path);
            return None;
        }

        eprintln!("[local_llm] Loading model from {:?}...", model_path);
        let mut file = std::fs::File::open(model_path).ok()?;
        let content = candle_core::quantized::gguf_file::Content::read(&mut file).ok()?;
        let model = candle_transformers::models::quantized_llama::ModelWeights::from_gguf(content, &mut file, &device).ok()?;

        eprintln!("[local_llm] Model loaded successfully");
        Some(Self { model, tokenizer, device, loaded: true })
    }

    /// Generate one raw completion from a chat-formatted prompt.
    fn generate_raw(&mut self, system: &str, user: &str) -> Option<String> {
        let full_prompt = format!(
            "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
            system, user
        );

        let encoded = self.tokenizer.encode(full_prompt.as_str(), true).ok()?;
        let prompt_ids = encoded.get_ids().to_vec();
        let prompt_len = prompt_ids.len();
        let eos = self.tokenizer.token_to_id("<|im_end|>").unwrap_or(u32::MAX);
        let eos2 = self.tokenizer.token_to_id("<|endoftext|>").unwrap_or(u32::MAX);
        let mut all_tokens = prompt_ids.clone();

        let input = Tensor::from_vec(prompt_ids, (1, prompt_len), &self.device).ok()?;
        let logits = self.model.forward(&input, 0).ok()?;
        let mut next = sample_token(&logits).ok()?;
        if next == eos || next == eos2 { return None; }
        all_tokens.push(next);

        for _step in 0..59usize {
            let input = Tensor::from_vec(vec![next], (1, 1), &self.device).ok()?;
            let logits = self.model.forward(&input, all_tokens.len() as usize - 1).ok()?;
            next = sample_token(&logits).ok()?;
            if next == eos || next == eos2 { break; }
            all_tokens.push(next);
        }

        let output = self.tokenizer.decode(&all_tokens[prompt_len..], true).ok()?;
        let trimmed = output.trim().to_string();
        if trimmed.is_empty() { return None; }
        Some(trimmed)
    }

    /// Generate a single title with RAG few-shot, retry, and post-cleaning.
    pub fn generate_one_clean(
        &mut self,
        keyword: &str,
        category: &str,
        style: &str,
        examples: &[String],
    ) -> Option<String> {
        let kw_lower = keyword.to_lowercase();
        let kw_tokens: Vec<&str> = kw_lower.split_whitespace().collect();
        let style_label = if style.is_empty() || style == "any" { "normal" } else { style };

        for attempt in 1..=3u32 {
            let system = "You are TitleForge, an elite title generator. Generate ONE creative, clickable title. Output ONLY the title text — no explanation, no preamble, no markdown formatting, no quotes around the title.";

            let mut user_prompt = String::new();
            if !examples.is_empty() {
                user_prompt.push_str(&format!("Examples of {} {} titles:\n", style_label, category));
                for ex in examples.iter().take(4) {
                    user_prompt.push_str(&format!("- \"{}\"\n", ex));
                }
                user_prompt.push('\n');
            }
            user_prompt.push_str(&format!(
                "Write ONE {} {} title about \"{}\". 3-15 words, must contain the keyword \"{}\", creative and clickable.",
                style_label, category, keyword, keyword
            ));
            if attempt > 1 {
                user_prompt.push_str(&format!("\n(Retry {} — write a DIFFERENT title.)", attempt));
            }

            let raw = match self.generate_raw(system, &user_prompt) {
                Some(r) => r,
                None => continue,
            };

            let cleaned = clean_output(&raw);
            eprintln!("[local_llm] attempt {}: '{}' -> '{}'", attempt, raw, cleaned);

            if cleaned.is_empty() || cleaned.len() < 3 || cleaned.split_whitespace().count() < 2 { continue; }
            let cl = cleaned.to_lowercase();
            let has_kw = cl.contains(&kw_lower) || kw_tokens.iter().any(|w| cl.contains(w));
            if !has_kw { continue; }
            let is_echo = examples.iter().any(|e| e.eq_ignore_ascii_case(&cleaned));
            if is_echo { continue; }
            if is_instruction_echo(&cl) { continue; }
            return Some(cleaned);
        }
        None
    }
}

/// Strip instruction-echo patterns, extract first clean title.
fn clean_output(raw: &str) -> String {
    let text = raw.trim();
    let text = text.strip_prefix("```json").unwrap_or(text);
    let text = text.strip_prefix("```").unwrap_or(text);
    let text = text.strip_suffix("```").unwrap_or(text);

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        // Colon salvage: "Title: Foo" -> "Foo"
        if let Some(col_pos) = trimmed.find(':') {
            let prefix = trimmed[..col_pos].to_lowercase();
            if is_echo_line(&prefix) {
                let after = trimmed[col_pos + 1..].trim();
                if !after.is_empty() && after.len() >= 3 { return clean_title(after); }
                continue;
            }
        }
        if is_echo_line(&trimmed.to_lowercase()) { continue; }
        return clean_title(trimmed);
    }
    String::new()
}

fn is_echo_line(lower: &str) -> bool {
    let echoes: &[&str] = &[
        "here", "i would", "sure", "let me", "title:", "here is", "here's",
        "i'm", "i can", "i think", "i'll", "please", "certainly",
        "of course", "i am", "note:", "based on", "using the",
    ];
    echoes.iter().any(|e| lower.starts_with(e))
}

fn is_instruction_echo(lower: &str) -> bool {
    lower.contains("title:") || lower.contains("reply with") || lower.contains("one title")
        || lower.contains("example") || lower.starts_with("write")
}

fn clean_title(s: &str) -> String {
    let mut t = s.trim().to_string();
    t = t.trim_matches(|c: char| c == '"' || c == '\'' || c == '\u{201c}' || c == '\u{201d}' || c == '`').to_string();
    t = t.trim_matches(|c: char| c == '\u{2018}' || c == '\u{2019}').to_string();
    t = t.replace("**", "").replace("__", "").replace('*', "").replace('#', "").replace('`', "");
    t = t.trim_start_matches(|c: char| c == '-' || c == '•' || c == '*').trim().to_string();
    if let Some(pos) = t.find(". ") {
        let prefix = &t[..pos];
        if prefix.chars().all(|c| c.is_ascii_digit()) && pos <= 3 { t = t[pos + 2..].to_string(); }
    }
    t.trim().to_string()
}

fn sample_token(logits: &Tensor) -> Result<u32, candle_core::Error> {
    let ndim = logits.dims().len();
    let vocab_logits = if ndim == 2 {
        logits.i(0)?
    } else if ndim >= 3 {
        let seq_len = logits.dim(ndim - 2)?;
        logits.i((0, seq_len - 1))?
    } else {
        return Err(candle_core::Error::Msg(format!("Unexpected logits shape: {:?}", logits.dims())));
    };
    let logits_scaled = (&vocab_logits / 0.7f64)?;
    let probs = candle_nn::ops::softmax(&logits_scaled, 0)?;
    let probs_vec: Vec<f32> = probs.to_vec1()?;

    let mut sorted: Vec<(usize, f32)> = probs_vec.iter().enumerate().map(|(i, p)| (i, *p)).collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut cum = 0.0f32;
    let mut candidates = Vec::new();
    for &(idx, p) in &sorted { cum += p; candidates.push((idx, p)); if cum >= 0.9f32 { break; } }
    if candidates.is_empty() { return Ok(probs_vec.len() as u32 - 1); }

    let total: f32 = candidates.iter().map(|(_, p)| p).sum();
    if total <= 0.0 { return Ok(candidates[0].0 as u32); }

    use rand::Rng;
    let mut rng = rand::thread_rng();
    let r: f32 = rng.gen::<f32>() * total;
    let mut c = 0.0f32;
    for (idx, p) in &candidates { c += p; if r <= c { return Ok(*idx as u32); } }
    Ok(candidates.last().map(|(i, _)| *i as u32).unwrap_or(0))
}
