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
        // n_ctx defaults to 512 in llama.cpp. Current prompts are ~100-166 tokens, but
        // the Task 1 rules experiment pushed them to 351-405 — and with max_new=60 that
        // is 411-465, uncomfortably close to the ceiling. Overflow shows up as a silent
        // `H: prefill decode failed`, which is exactly the class of bug that cost this
        // project weeks. 1024 costs essentially nothing for CPU inference and removes the
        // trap for anyone who lengthens the prompt later.
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZeroU32::new(1024));
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

    /// Locate a bundled/installed model file by name.
    ///
    /// Benchmarks used to hardcode `../models/<name>`, which only works on a
    /// dev checkout that keeps a copy inside the repo. A real install puts the
    /// model in the OS data dir (see `qwen_model_path()` in lib.rs), so on any
    /// machine with an actual install the benches silently skipped with
    /// "model not found". Search order mirrors `lazy_load_llm`, with an env
    /// override so a model on another volume can be used without copying ~1 GB.
    pub fn find_model(name: &str) -> Option<std::path::PathBuf> {
        if let Ok(p) = std::env::var("TF_MODEL_PATH") {
            let p = std::path::PathBuf::from(p);
            let p = if p.is_dir() { p.join(name) } else { p };
            if p.exists() {
                return Some(p);
            }
        }
        let data = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let candidates = [
            std::path::PathBuf::from("../models").join(name),
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("models").join(name),
            data.join("titleforge-desktop").join("models").join(name),
            data.join("com.titleforge.desktop").join("models").join(name),
        ];
        candidates.into_iter().find(|p| p.exists())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn generate_one_clean(
        &mut self,
        keyword: &str,
        category: &str,
        style: &str,
        genre: &str,
        examples: &[String],
        constraint: Option<&str>,
        finetune: &crate::prompt_spec::FineTune,
    ) -> Option<String> {
        // Category conventions REPLACE the old bare-word substitution. Before
        // this, category was a label ("Generate ONE creative, clickable
        // {category} title") and output collapsed to one shape across all 16
        // categories. See prompt_spec.rs for the measured evidence and for the
        // rule about not stacking additional constraints here.
        let spec = crate::prompt_spec::category_spec(category);
        let style_desc = crate::prompt_spec::style_description(style);
        let genre_text = if genre.is_empty() || genre == "any" {
            String::new()
        } else {
            format!(" in the {} genre", genre)
        };
        // At most ONE extra line, and only for the soft fields. mustInclude /
        // avoid are enforced after generation instead — see FineTune docs.
        let ft_line = finetune.soft_prompt_line().unwrap_or_default();

        // Best candidate that failed only a SOFT preference (mood-category
        // shape, exemplar echo). Returned if all 3 attempts fail, so a
        // stylistic preference can never turn into an empty slot.
        let mut soft_reject: Option<String> = None;

        for attempt in 1..=3u32 {
            // System prompt — the output must be ABOUT the topic, but we do NOT
            // force the literal keyword. Forcing it (brief: "inversely
            // correlated with quality") produces stuffed, uncreative output.
            // Encourage naturally weaving in the keyword OR a close variant,
            // and rank creativity above literal inclusion.
            //
            // Name categories are a DIFFERENT TASK: the output is a name, not a
            // headline about the topic. Saying so plainly is the whole fix for
            // the user-reported "product titles read like blog titles".
            let system = if spec.is_name {
                format!(
                    "You are TitleForge. Generate ONE {label} inspired by \"{keyword}\"{genre_text}. A {label} is {form}. It is a NAME, not a headline and not a sentence about the topic. Example of the right shape: \"{example}\". Output ONLY the name — no explanation, no preamble, no markdown, no quotes.",
                    label = spec.label, keyword = keyword, genre_text = genre_text,
                    form = spec.form, example = spec.example
                )
            } else {
                format!(
                    "You are TitleForge, an elite title generator. Generate ONE {label} about \"{keyword}\"{genre_text}. A {label} is {form}. Example of the right shape: \"{example}\". Weave in the topic or a close variant where it fits, but never force it. Output ONLY the title text — no explanation, no preamble, no markdown, no quotes.",
                    label = spec.label, keyword = keyword, genre_text = genre_text,
                    form = spec.form, example = spec.example
                )
            };

            let mut user_prompt = String::new();
            if !examples.is_empty() {
                user_prompt.push_str(&format!("Examples of {} {}s:\n", style_desc, spec.label));
                for ex in examples.iter().take(3) { user_prompt.push_str(&format!("- \"{}\"\n", ex)); }
                user_prompt.push('\n');
            }
            // ONE extra constraint per call (rotated across the batch by the
            // caller) to break formula repetition — Qwen 1.5B handles a single
            // constraint fine, not the full 6-rule block (which measured worse).
            // Constraints are structural and only make sense for real titles;
            // "make this one a question" applied to a product name is nonsense.
            let c = if spec.is_name { "" } else { constraint.unwrap_or("") };
            let c_line = if c.is_empty() { String::new() } else { format!(" {}", c) };
            user_prompt.push_str(&format!(
                "Write a {} {} about \"{}\". {}-{} words, {}.{}{}",
                style_desc, spec.label, keyword,
                spec.words.0, spec.words.1,
                if spec.is_name { "distinctive and memorable" } else { "creative and natural" },
                c_line, ft_line
            ));
            if attempt > 1 { user_prompt.push_str(&format!("\n(Retry {} — DIFFERENT {}, still inspired by \"{}\", more creative.{})", attempt, spec.label, keyword, c_line)); }

            let raw = match self.generate_chat_raw(&system, &user_prompt) {
                Some(r) => r,
                None => continue,
            };
            let cleaned = clean_output(&raw);
            #[cfg(debug_assertions)]
            eprintln!("[local_llm] attempt {}: '{}' -> '{}'", attempt, raw, cleaned);
            // Minimum length. Name categories legitimately produce ONE word
            // ("Vivid"), so the blanket >=2-word floor is applied to titles
            // only — it was silently rejecting every correct product name.
            let min_words = if spec.is_name { 1 } else { 2 };
            if cleaned.len() < 3 || cleaned.split_whitespace().count() < min_words { continue; }
            let cl = cleaned.to_lowercase();

            // Drift guard — NOT a literal-keyword gate. We softened the prompt
            // for creativity, but Qwen 1.5B can drift fully off-topic without
            // any guard (investing → "High-Retention Fundraising", scored 12
            // on 07-31). curated_is_relevant() accepts any >=4-char keyword
            // word anywhere in the title — creative titles survive, genuine
            // off-topic drift does not. (brief rule #3, Task 1.)
            //
            // EXEMPT for name categories. A brandable product name inspired by
            // "coffee" ("Ember", "Vivid") deliberately does not contain the
            // keyword; applying the guard here rejected 100% of correct output.
            // The trade-off is real and accepted: names are unguarded against
            // topical drift, so `passes_name_shape` below carries the QC weight
            // instead. Revisit if name-category drift is ever measured.
            if !spec.is_name {
                let keyword_ok = cl.len() >= 4 && crate::engine::curated_is_relevant(&cleaned, keyword);
                if !keyword_ok { continue; }
            }

            // Shape check. Split by severity, because these two cases are NOT
            // equally bad and treating them the same cost 18% of output.
            //
            // NAME categories: HARD. A blog headline returned for `product` is
            // the original user-reported bug; never return it.
            //
            // Mood categories (song/poem/album/movie/book): SOFT. A colon in a
            // song title is wrong but still usable. Measured 2026-08-03: making
            // it a hard reject inside the fixed 3-attempt budget dropped fire
            // rate 100% -> 82%, halving song and poem output — the same failure
            // the cliche filter caused on 2026-08-02 (50 -> 34). Brief §5 rule
            // 4: "Empty output is a failure, not a skip." So we PREFER a clean
            // shape and fall back to the best rejected candidate if the budget
            // runs out.
            if !crate::prompt_spec::passes_name_shape(&cleaned, &spec) {
                #[cfg(debug_assertions)]
                eprintln!("[local_llm] attempt {} rejected (wrong shape for {}): '{}'", attempt, spec.label, cleaned);
                if !spec.is_name && soft_reject.is_none() {
                    soft_reject = Some(cleaned.clone());
                }
                continue;
            }

            // The prompt's exemplar is there to be imitated in SHAPE. Qwen also
            // imitates its content — measured: the YouTube example "I Spent 48
            // Hours in a Silent Retreat" produced "48 Hours in the Silent Remote
            // Office". Reject the echo rather than drop the exemplar.
            // Also SOFT — an echo is a weak title, not an unusable one, and it
            // must not be able to empty a batch on its own.
            if crate::prompt_spec::echoes_example(&cleaned, &spec) {
                #[cfg(debug_assertions)]
                eprintln!("[local_llm] attempt {} rejected (echoes exemplar): '{}'", attempt, cleaned);
                if soft_reject.is_none() {
                    soft_reject = Some(cleaned.clone());
                }
                continue;
            }

            // Fine-tune hard constraints (mustInclude / avoid). Enforced here
            // rather than in the prompt: a 1.5B asked to satisfy a word
            // blocklist burns its 3-attempt budget and returns nothing.
            if !finetune.satisfies_hard_constraints(&cleaned) {
                #[cfg(debug_assertions)]
                eprintln!("[local_llm] attempt {} rejected (fine-tune constraint): '{}'", attempt, cleaned);
                continue;
            }

            if examples.iter().any(|e| e.eq_ignore_ascii_case(&cleaned)) { continue; }
            if is_instruction_echo(&cl) { continue; }
            // Cliché rejection — "Ultimate / Unlock / Unleash / Revolutionize /
            // Secrets" etc. appeared in 21/50 titles. Reject and retry; the
            // loop's 3-attempt budget is the cap, so it can't hang. (brief §4 Task 5)
            if contains_cliche(&cl) {
                #[cfg(debug_assertions)]
                eprintln!("[local_llm] attempt {} rejected (cliché): '{}'", attempt, cleaned);
                continue;
            }
            return Some(cleaned);
        }
        // Budget exhausted. A candidate that failed only a stylistic preference
        // beats returning nothing — an empty slot is a worse product than a
        // song title with a colon in it.
        #[cfg(debug_assertions)]
        if soft_reject.is_some() {
            eprintln!("[local_llm] budget exhausted; returning soft-rejected candidate");
        }
        soft_reject
    }
}

/// True if the title uses an egregious cliché — the words the 07-31 benchmark
/// flagged (21/50) and the user called out as uncreative. Kept NARROW: only the
/// strongest offenders. "secrets"/"master"/"unveil" were dropped because Qwen
/// 1.5B pairs them with the topic so often that rejecting them exhausts the
/// 3-attempt budget and produces empty batches (fire rate dropped 50→34).
fn contains_cliche(lower: &str) -> bool {
    ["ultimate", "unlock", "unleash", "revolutioniz", "game changer",
     "mind-blowing", "life-changing"]
        .iter().any(|c| lower.contains(c))
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
        // Creator-voice echoes Qwen sometimes leaks: "get ready to dive into",
        // "our latest video", "welcome to", "here's a video".
        || lower.starts_with("get ready")
        || lower.starts_with("welcome")
        || lower.contains("our latest video")
        || lower.contains("going to show you")
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


