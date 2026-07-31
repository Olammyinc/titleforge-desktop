use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;
use rand::Rng;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::{LlamaChatMessage, AddBos, Special};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::token::LlamaToken;

/// Sampling temperature for autoregressive generation.
/// Argmax produced identical titles for every call — this is what makes
/// batch generation possible. Tune by measurement: 0.6 / 0.8 / 1.0.
///
/// Override at runtime with `TF_LLM_TEMP` (benchmark sweep without rebuilds).
fn temperature() -> f32 {
    static TEMP: OnceLock<f32> = OnceLock::new();
    *TEMP.get_or_init(|| {
        std::env::var("TF_LLM_TEMP")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|t| (0.05..=2.0).contains(t))
            .unwrap_or(0.8)
    })
}

/// Top-K candidates by logit before softmax.
const TOP_K: usize = 40;

static BACKEND: OnceLock<Option<LlamaBackend>> = OnceLock::new();

pub struct LocalLlm {
    model: LlamaModel,
    pub loaded: bool,
    /// Precomputed token IDs whose text starts an instruction-echo prefix
    /// ("Here", "Sure!", "Certainly", ...). Applied only at the first
    /// generated position. Compared by ID, not string — the string form is
    /// far too slow to check against ~150k candidates on every token.
    banned_first: HashSet<LlamaToken>,
}

impl LocalLlm {
    pub fn load(model_path: &Path) -> Option<Self> {
        if !model_path.exists() {
            eprintln!("[local_llm] Model file not found: {:?}", model_path);
            return None;
        }
        let backend_opt = BACKEND.get_or_init(|| {
            match LlamaBackend::init() {
                Ok(b) => Some(b),
                Err(e) => { eprintln!("[local_llm] Backend init failed: {:?}", e); None }
            }
        });
        let backend = backend_opt.as_ref()?;
        eprintln!("[local_llm] Loading model from {:?}...", model_path);
        let model = LlamaModel::load_from_file(backend, model_path, &LlamaModelParams::default()).ok()?;
        eprintln!("[local_llm] Model loaded successfully");

        // Precompute banned token IDs once (echo-prefix suppression).
        // Scanning the full vocab is a one-time ~50ms cost at load; doing the
        // string check per-token in the generation loop is ~10x slower per call.
        let banned_first = build_banned_first(&model);

        Some(Self { model, loaded: true, banned_first })
    }

    fn generate_chat_raw(&self, system: &str, user: &str) -> Option<String> {
        // Set TF_LLM_DIAG=1 to trace exactly where generation bails out.
        let diag = std::env::var("TF_LLM_DIAG").is_ok();
        macro_rules! bail {
            ($($a:tt)*) => {{ if diag { eprintln!("[llm-diag] {}", format!($($a)*)); } return None; }};
        }

        #[allow(unused_must_use)]
        let messages: Vec<LlamaChatMessage> = vec![
            LlamaChatMessage::new("system".into(), system.into()),
            LlamaChatMessage::new("user".into(), user.into()),
        ].into_iter().filter_map(|r| r.ok()).collect();
        if messages.len() < 2 { bail!("A: chat message construction failed"); }
        let tmpl = match self.model.chat_template(None) {
            Ok(t) => t, Err(e) => bail!("B: chat_template: {:?}", e),
        };
        let prompt = match self.model.apply_chat_template(&tmpl, &messages, true) {
            Ok(p) => p, Err(e) => bail!("C: apply_chat_template: {:?}", e),
        };
        let ctx_params = LlamaContextParams::default();
        let backend = match BACKEND.get().and_then(|b| b.as_ref()) {
            Some(b) => b, None => bail!("D: backend unavailable"),
        };
        let mut ctx = match self.model.new_context(backend, ctx_params) {
            Ok(c) => c, Err(e) => bail!("E: new_context: {:?}", e),
        };
        let tokens = match self.model.str_to_token(&prompt, AddBos::Always) {
            Ok(t) => t, Err(e) => bail!("F: str_to_token: {:?}", e),
        };
        let n_prompt = tokens.len();
        let eos = self.model.token_eos();
        let max_new = 60;

        if diag { eprintln!("[llm-diag] n_prompt={} n_ctx={}", n_prompt, ctx.n_ctx()); }

        // Batched prefill
        let max_tokens = n_prompt + max_new as usize;
        {
            let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(max_tokens, 1);
            for (i, &tok) in tokens.iter().enumerate() {
                if let Err(e) = batch.add(tok, i as i32, &[0], i == n_prompt - 1) {
                    bail!("G: batch.add failed at token {}/{}: {:?}", i, n_prompt, e);
                }
            }
            if let Err(e) = ctx.decode(&mut batch) {
                bail!("H: prefill decode failed (n_prompt={}, n_ctx={}): {:?}", n_prompt, ctx.n_ctx(), e);
            }
        }

        // Sample first token — suppress instruction-echo prefixes.
        // Qwen2.5-1.5B frequently starts with "Here is...", "Sure!...", etc.
        // `banned_first` is applied only at this position.
        let mut rng = rand::thread_rng();
        let first_tok: Option<llama_cpp_2::token::LlamaToken> = {
            let mut n_cand = 0usize;
            for _cd in ctx.candidates() { n_cand += 1; }
            let sampled = sample_token(
                ctx.candidates().map(|cd| (cd.id(), cd.logit())),
                Some(&self.banned_first),
                &mut rng,
            );
            if diag {
                eprintln!("[llm-diag] first-token: candidates={} banned={} sampled={:?}",
                    n_cand, self.banned_first.len(), sampled);
            }
            sampled
        };
        if first_tok.is_none() { bail!("I: no viable first token (all candidates eos/banned/NaN)"); }
        let mut next = first_tok;

        // Track generated tokens
        let mut gen_tokens: Vec<llama_cpp_2::token::LlamaToken> = Vec::new();

        // Autoregressive decode: one token per step
        for pos in n_prompt as i32..(n_prompt + max_new) as i32 {
            let tok = match next { Some(t) => t, None => break };
            gen_tokens.push(tok);
            let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(1, 1);
            if let Err(e) = batch.add(tok, pos, &[0], true) {
                if diag { eprintln!("[llm-diag] decode-loop batch.add failed at pos {}: {:?}", pos, e); }
                break;
            }
            if let Err(e) = ctx.decode(&mut batch) {
                if diag { eprintln!("[llm-diag] decode-loop decode failed at pos {}: {:?}", pos, e); }
                break;
            }
            // Sample the next token (no banned set — only EOS terminates)
            next = sample_token(
                ctx.candidates().map(|cd| (cd.id(), cd.logit())),
                None,
                &mut rng,
            );
            if next == Some(eos) { break; }
        }

        if gen_tokens.is_empty() { bail!("J: no tokens generated"); }

        // Decode token-by-token into a byte buffer rather than using
        // `tokens_to_str`, whose internal buffer is sized too small and fails with
        // InsufficientBufferSpace on ordinary titles. That single call was silently
        // discarding ~46% of successfully generated output (13/14 in the 2026-07-31
        // trace) — the model was never the problem.
        //
        // Bytes, not per-token Strings: BPE tokens can split a multi-byte UTF-8
        // character, so each token is not independently valid UTF-8. Accumulate the
        // raw bytes and decode once at the end.
        let mut buf: Vec<u8> = Vec::with_capacity(gen_tokens.len() * 4);
        for &t in &gen_tokens {
            match self.model.token_to_bytes(t, Special::Tokenize) {
                Ok(b) => buf.extend_from_slice(&b),
                Err(e) => {
                    if diag { eprintln!("[llm-diag] token_to_bytes skipped token: {:?}", e); }
                }
            }
        }
        if buf.is_empty() { bail!("K: all {} tokens failed to decode", gen_tokens.len()); }
        let result = String::from_utf8_lossy(&buf).to_string();

        let trimmed = result.trim().to_string();
        if trimmed.is_empty() { bail!("L: decoded to empty string ({} tokens)", gen_tokens.len()); }
        Some(trimmed)
    }

    /// Diagnostics only — exposes the unfiltered model output so tests can
    /// attribute rejections to a specific filter. Not used in production paths.
    pub fn debug_raw(&self, system: &str, user: &str) -> Option<String> {
        self.generate_chat_raw(system, user)
    }

    /// Diagnostics only — token count of the fully chat-templated prompt.
    /// Used to detect context-window overflow without running generation.
    pub fn debug_prompt_tokens(&self, system: &str, user: &str) -> Option<usize> {
        let messages: Vec<LlamaChatMessage> = vec![
            LlamaChatMessage::new("system".into(), system.into()),
            LlamaChatMessage::new("user".into(), user.into()),
        ].into_iter().filter_map(|r| r.ok()).collect();
        if messages.len() < 2 { return None; }
        let tmpl = self.model.chat_template(None).ok()?;
        let prompt = self.model.apply_chat_template(&tmpl, &messages, true).ok()?;
        self.model.str_to_token(&prompt, AddBos::Always).ok().map(|t| t.len())
    }

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
            // Stronger system prompt — keyword inclusion is MANDATORY
            let system = format!(
                "You are TitleForge, an elite title generator. Generate ONE creative, clickable {} title about \"{}\". CRITICAL RULE: the title MUST contain the word \"{}\" somewhere in it. Output ONLY the title text — no explanation, no preamble, no markdown, no quotes.",
                category, keyword, keyword
            );

            let mut user_prompt = String::new();
            if !examples.is_empty() {
                user_prompt.push_str(&format!("Examples of {} {} titles:\n", style_label, category));
                for ex in examples.iter().take(3) { user_prompt.push_str(&format!("- \"{}\"\n", ex)); }
                user_prompt.push('\n');
            }
            user_prompt.push_str(&format!(
                "Write a {} {} title. The word \"{}\" MUST appear in the title. 3-15 words, creative, clickable.",
                style_label, category, keyword
            ));
            if attempt > 1 { user_prompt.push_str(&format!("\n(Retry {} — DIFFERENT title. MUST include \"{}\".)", attempt, keyword)); }

            let raw = match self.generate_chat_raw(&system, &user_prompt) {
                Some(r) => r,
                None => continue,
            };
            let cleaned = clean_output(&raw);
            #[cfg(debug_assertions)]
            eprintln!("[local_llm] attempt {}: '{}' -> '{}'", attempt, raw, cleaned);
            if cleaned.len() < 3 || cleaned.split_whitespace().count() < 2 { continue; }
            let cl = cleaned.to_lowercase();

            // Keyword QC: strict on attempt 1, relaxed on retries
            // Qwen-1.5B can't reliably force keyword inclusion.
            // When it fails, the creative alternative is better than nothing.
            let keyword_ok = match attempt {
                1 => cl.contains(&kw_lower) || kw_tokens.iter().any(|&w| cl.contains(w))
                    || (kw_tokens.len() > 1 && {
                        let m = kw_tokens.iter().filter(|&w| cl.contains(w)).count();
                        m * 2 >= kw_tokens.len()
                    }),
                _ => cl.len() >= 4, // relax: accept any coherent output on retry
            };
            if !keyword_ok { continue; }

            if examples.iter().any(|e| e.eq_ignore_ascii_case(&cleaned)) { continue; }
            if is_instruction_echo(&cl) { continue; }
            return Some(cleaned);
        }
        None
    }
}

/// Scan the full vocabulary once and collect token IDs whose text starts an
/// instruction-echo prefix. Qwen2.5-1.5B tends to begin with "Here is...",
/// "Sure!...", "Certainly!...", markdown fences, etc. — suppressing these at
/// the first position forces a creative opening word.
fn build_banned_first(model: &LlamaModel) -> HashSet<LlamaToken> {
    let prefixes = ["here", "sure", "cert", "please", "write", "note", "based", "using", "```"];
    let mut banned = HashSet::new();
    let n_vocab = model.n_vocab();
    for id in 0..n_vocab {
        let tok = LlamaToken::new(id);
        let text = model.token_to_str(tok, Special::Tokenize).unwrap_or_default();
        let trimmed = text.trim().to_lowercase();
        if prefixes.iter().any(|p| trimmed.starts_with(p)) {
            banned.insert(tok);
        }
    }
    // EOS must never be the FIRST token of a response — the title would be
    // empty. (EOS is allowed from position 1 onward; that is how the model
    // terminates.) Also skip the BOS token if present.
    banned.insert(model.token_eos());
    let bos = model.token_bos();
    if bos != model.token_eos() { banned.insert(bos); }
    #[cfg(debug_assertions)]
    eprintln!("[local_llm] banned_first: {} of {} vocab tokens", banned.len(), n_vocab);
    banned
}

/// Sample one token from the current candidate distribution.
/// - Top-K by raw logit (keeps the strongest TOP_K candidates)
/// - Softmax with temperature, shifted by max for numerical stability
/// - Inverse-CDF sample via `rng` — this is what breaks determinism
///
/// `banned` is applied only at the first generated position (echo
/// suppression). Pass `None` for every subsequent position.
///
/// EOS is NOT filtered here — it must remain sampleable so the model can
/// terminate naturally. The decode loop breaks when EOS is sampled. Only the
/// first position forbids EOS (it is included in `banned_first`).
///
/// Returns `None` only if every candidate is banned/degenerate — the caller
/// treats that as end-of-sequence.
fn sample_token<R: Rng>(
    cands_iter: impl Iterator<Item = (llama_cpp_2::token::LlamaToken, f32)>,
    banned: Option<&HashSet<llama_cpp_2::token::LlamaToken>>,
    rng: &mut R,
) -> Option<llama_cpp_2::token::LlamaToken> {
    let mut cands: Vec<(llama_cpp_2::token::LlamaToken, f32)> = cands_iter
        .filter(|(id, _)| banned.map_or(true, |b| !b.contains(id)))
        .collect();
    if cands.is_empty() { return None; }

    // Top-K by logit
    cands.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    cands.truncate(TOP_K);

    // Softmax with temperature, shifted by max for numerical stability
    let max_l = cands[0].1;
    let temp = temperature();
    let mut sum = 0.0f32;
    let probs: Vec<f32> = cands.iter().map(|(_, l)| {
        let p = ((l - max_l) / temp).exp();
        sum += p;
        p
    }).collect();
    if !(sum > 0.0) || !sum.is_finite() { return Some(cands[0].0); } // degenerate → argmax

    // Inverse-CDF sample
    let r = rng.gen::<f32>() * sum;
    let mut acc = 0.0f32;
    for (i, p) in probs.iter().enumerate() {
        acc += p;
        if acc >= r { return Some(cands[i].0); }
    }
    Some(cands[cands.len() - 1].0)
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


