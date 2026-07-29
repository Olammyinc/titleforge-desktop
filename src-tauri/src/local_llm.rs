use std::path::Path;
use std::sync::OnceLock;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::{LlamaChatMessage, AddBos};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::model::params::LlamaModelParams;

static BACKEND: OnceLock<LlamaBackend> = OnceLock::new();

pub struct LocalLlm {
    model: LlamaModel,
    #[allow(dead_code)]
    backend: &'static LlamaBackend,
    pub loaded: bool,
}

impl LocalLlm {
    pub fn load(model_path: &Path) -> Option<Self> {
        if !model_path.exists() {
            eprintln!("[local_llm] Model file not found: {:?}", model_path);
            return None;
        }
        let backend = BACKEND.get_or_init(|| {
            LlamaBackend::init().expect("llama.cpp backend init failed")
        });
        eprintln!("[local_llm] Loading model from {:?}...", model_path);
        let model = LlamaModel::load_from_file(backend, model_path, &LlamaModelParams::default()).ok()?;
        eprintln!("[local_llm] Model loaded successfully");
        Some(Self { model, backend, loaded: true })
    }

    fn generate_chat_raw(&self, system: &str, user: &str) -> Option<String> {
        let messages: Vec<LlamaChatMessage> = vec![
            LlamaChatMessage::new("system".into(), system.into()),
            LlamaChatMessage::new("user".into(), user.into()),
        ].into_iter().filter_map(|r| r.ok()).collect();
        if messages.len() < 2 { return None; }
        let tmpl = self.model.chat_template(None).ok()?;
        let prompt = self.model.apply_chat_template(&tmpl, &messages, true).ok()?;
        let ctx_params = LlamaContextParams::default();
        let mut ctx = self.model.new_context(self.backend, ctx_params).ok()?;
        let tokens = self.model.str_to_token(&prompt, AddBos::Always).ok()?;
        let n_prompt = tokens.len();
        let eos = self.model.token_eos();
        let max_new = 60;
        let mut all_tokens = tokens;

        // Batched prefill: feed ALL prompt tokens in one batch decode.
        // Size the batch for the full prompt + decode window so there's no
        // reallocation pressure during the decode loop.
        let max_tokens = n_prompt + max_new as usize;
        {
            let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(max_tokens, 1);
            for (i, &tok) in all_tokens.iter().enumerate() {
                let last = i == n_prompt - 1;
                batch.add(tok, i as i32, &[0], last);
            }
            ctx.decode(&mut batch).ok()?;
        }

        // Sample first generated token from the prefill logits
        let mut next: Option<llama_cpp_2::token::LlamaToken> = {
            let mut best_tok = eos;
            let mut best_logit = f32::NEG_INFINITY;
            for cd in ctx.candidates() {
                if cd.logit() > best_logit { best_logit = cd.logit(); best_tok = cd.id(); }
            }
            if best_tok == eos { None } else { Some(best_tok) }
        };

        // Autoregressive decode: one token per step
        for pos in n_prompt as i32..(n_prompt as i32 + max_new) {
            let tok = match next { Some(t) => t, None => break };
            all_tokens.push(tok);
            let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(1, 1);
            batch.add(tok, pos, &[0], true);
            if ctx.decode(&mut batch).is_err() { break; }
            let mut best_tok = eos;
            let mut best_logit = f32::NEG_INFINITY;
            for cd in ctx.candidates() {
                if cd.logit() > best_logit { best_logit = cd.logit(); best_tok = cd.id(); }
            }
            if best_tok == eos { break; }
            next = Some(best_tok);
        }

        let generated = &all_tokens[n_prompt..];
        if generated.is_empty() { return None; }
        #[allow(deprecated)]
        let result = self.model.tokens_to_str(generated, llama_cpp_2::model::Special::Tokenize).ok()?;
        let trimmed = result.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    }

    pub fn generate_one_clean(
        &self,
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
                for ex in examples.iter().take(4) { user_prompt.push_str(&format!("- \"{}\"\n", ex)); }
                user_prompt.push('\n');
            }
            user_prompt.push_str(&format!(
                "Write ONE {} {} title about \"{}\". 3-15 words, must contain the keyword \"{}\", creative and clickable.",
                style_label, category, keyword, keyword
            ));
            if attempt > 1 { user_prompt.push_str(&format!("\n(Retry {} — write a DIFFERENT title.)", attempt)); }

            let raw = match self.generate_chat_raw(system, &user_prompt) {
                Some(r) => r,
                None => continue,
            };
            let cleaned = clean_output(&raw);
            eprintln!("[local_llm] attempt {}: '{}' -> '{}'", attempt, raw, cleaned);
            if cleaned.len() < 3 || cleaned.split_whitespace().count() < 2 { continue; }
            let cl = cleaned.to_lowercase();
            if !cl.contains(&kw_lower) && !kw_tokens.iter().any(|w| cl.contains(w)) { continue; }
            if examples.iter().any(|e| e.eq_ignore_ascii_case(&cleaned)) { continue; }
            if is_instruction_echo(&cl) { continue; }
            return Some(cleaned);
        }
        None
    }
}

fn clean_output(raw: &str) -> String {
    let text = raw.trim();
    let text = text.strip_prefix("```json").unwrap_or(text).strip_prefix("```").unwrap_or(text);
    let text = text.strip_suffix("```").unwrap_or(text);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        if let Some(col_pos) = trimmed.find(':') {
            if is_echo_line(&trimmed[..col_pos].to_lowercase()) {
                let after = trimmed[col_pos + 1..].trim();
                if after.len() >= 3 { return clean_title(after); }
                continue;
            }
        }
        if !is_echo_line(&trimmed.to_lowercase()) { return clean_title(trimmed); }
    }
    String::new()
}

fn is_echo_line(lower: &str) -> bool {
    ["here", "i would", "sure", "let me", "title:", "here is", "here's",
     "i'm", "i can", "i think", "i'll", "please", "certainly",
     "of course", "i am", "note:", "based on", "using the"]
        .iter().any(|e| lower.starts_with(e))
}

fn is_instruction_echo(lower: &str) -> bool {
    lower.contains("title:") || lower.contains("reply with") || lower.contains("one title")
        || lower.contains("example") || lower.starts_with("write")
}

fn clean_title(s: &str) -> String {
    let mut t = s.trim().to_string();
    t = t.trim_matches(|c: char| matches!(c, '"' | '\'' | '\u{201c}' | '\u{201d}' | '`')).to_string();
    t = t.trim_matches(|c: char| matches!(c, '\u{2018}' | '\u{2019}')).to_string();
    t = t.replace("**", "").replace("__", "").replace('*', "").replace('#', "").replace('`', "");
    t = t.trim_start_matches(|c: char| matches!(c, '-' | '•' | '*')).trim().to_string();
    if let Some(pos) = t.find(". ") {
        let prefix = &t[..pos];
        if prefix.chars().all(|c| c.is_ascii_digit()) && pos <= 3 { t = t[pos + 2..].to_string(); }
    }
    t.trim().to_string()
}
