//! EGCG: Exemplar-Guided Coherence Generator
//!
//! A new offline title generation algorithm for TitleForge Desktop.
//! Replaces the sparse Markov chain model with a coherence-scored
//! constraint-based generator that degrades gracefully on sparse data.
//!
//! Three generation modes (80/10/10 proportional):
//!   A — Exemplar-Guided Template Fill
//!   B — Phrase Stitching (intro + keyword + closer fragments)
//!   C — Keyword-Embedded Exemplar (swap topic in curated title)

use std::collections::{HashMap, HashSet};

use rand::seq::SliceRandom;
use rand::Rng;
use rusqlite::Connection;

use crate::TitleResult;

// ── Constants ──

/// Minimum coherence score for a candidate slot filler.
const MIN_COHERENCE: f64 = 0.05;

/// Window size for pairwise co-occurrence in curated titles.
const AFFINITY_WINDOW: usize = 5;

/// Temperature for softmax sampling (lower = greedier, higher = more diverse).
const SOFTMAX_TEMP: f64 = 0.7;

/// Number of top candidates retained for softmax sampling.
const TOP_K: usize = 12;

// ── Data Structures ──

#[derive(Clone, serde::Deserialize)]
struct SlotDef {
    name: String,
    pool: Option<String>,
    #[allow(dead_code)]
    pos: Option<String>,
}

#[derive(Clone)]
struct TemplateInfo {
    template: String,
    slots: Vec<SlotDef>,
    genre: String,
}

/// The EGCG generator — all indices are pre-built at startup for fast generation.
pub struct Generator {
    /// word → integer ID (for efficient affinity lookups)
    word2id: HashMap<String, usize>,
    /// ID → word
    id2word: Vec<String>,
    /// (word_id_a, word_id_b) → co-occurrence count within window=5 in curated titles
    affinity: HashMap<(usize, usize), u32>,
    /// (word_id, category) → unigram frequency
    unigram_cat: HashMap<(usize, String), u32>,
    /// category → list of templates
    templates: HashMap<String, Vec<TemplateInfo>>,
    /// pool_name → words
    pools: HashMap<String, Vec<String>>,
    /// category → list of curated titles
    #[allow(dead_code)]
    curated: HashMap<String, Vec<String>>,
    /// Intro fragments mined from curated titles (first 2-3 words)
    intro_fragments: Vec<String>,
    /// Closer fragments mined from curated titles (last 2-3 words)
    closer_fragments: Vec<String>,
    /// per-category exemplar vocabulary (words appearing in curated titles)
    exemplar_vocab: HashMap<String, HashSet<String>>,
    /// All curated titles for mode C (title, category, genre)
    all_curated: Vec<(String, String, String)>,
}

// ── Public API ──

impl Generator {
    /// Return the number of unique words in the vocabulary.
    pub fn word_count(&self) -> usize {
        self.id2word.len()
    }

    /// Return whether the generator has data (curated titles loaded).
    pub fn is_empty(&self) -> bool {
        self.all_curated.is_empty()
    }

    /// Build the EGCG generator from the SQLite database.
    ///
    /// Loads curated titles, word pools, and templates, then builds
    /// all indices needed for fast offline generation.
    pub fn build(conn: &Connection) -> Self {
        // ── Load curated titles ──
        let mut curated: HashMap<String, Vec<String>> = HashMap::new();
        let mut all_curated: Vec<(String, String, String)> = Vec::new();

        {
            let rows: Vec<(String, String, String)> = match conn.prepare("SELECT title, category, COALESCE(genre, 'any') FROM curated_titles") {
                Ok(mut stmt) => stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))
                    .ok()
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default(),
                Err(e) => { eprintln!("Warning: failed to load curated_titles: {}", e); vec![] }
            };

            for (title, category, genre) in rows {
                all_curated.push((title.clone(), category.clone(), genre));
                curated
                    .entry(category)
                    .or_default()
                    .push(title);
            }
        }

        // ── Load word pools ──
        let mut pools: HashMap<String, Vec<String>> = HashMap::new();
        {
            let rows: Vec<(String, String)> = match conn.prepare("SELECT pool_name, word FROM word_pools") {
                Ok(mut stmt) => stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                    .ok()
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default(),
                Err(e) => { eprintln!("Warning: failed to load word_pools: {}", e); vec![] }
            };

            for (pool_name, word) in rows {
                pools.entry(pool_name).or_default().push(word);
            }
        }

        // ── Load templates ──
        let mut templates: HashMap<String, Vec<TemplateInfo>> = HashMap::new();
        {
            let rows: Vec<(String, String, String, String)> = match conn.prepare("SELECT category, template, slots, COALESCE(genre, 'any') FROM patterns") {
                Ok(mut stmt) => stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?))
                })
                    .ok()
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default(),
                Err(e) => { eprintln!("Warning: failed to load patterns: {}", e); vec![] }
            };

            for (category, template, slots_json, genre) in rows {
                let slots: Vec<SlotDef> =
                    serde_json::from_str(&slots_json).unwrap_or_default();
                templates
                    .entry(category)
                    .or_default()
                    .push(TemplateInfo { template, slots, genre });
            }
        }

        // ── Build vocabulary from curated titles ──
        let mut vocab_set: HashSet<String> = HashSet::new();
        for (_cat, titles) in &curated {
            for title in titles {
                for word in tokenize(title) {
                    if word.len() > 1 {
                        vocab_set.insert(word);
                    }
                }
            }
        }

        // Add pool words to vocabulary as well (they might not appear in curated titles)
        for (_name, words) in &pools {
            for word in words {
                let lower = word.to_lowercase();
                if lower.len() > 1 {
                    vocab_set.insert(lower);
                }
            }
        }

        let word2id: HashMap<String, usize> = vocab_set
            .iter()
            .enumerate()
            .map(|(i, w)| (w.clone(), i))
            .collect();

        let id2word: Vec<String> = {
            let mut v = vec![String::new(); vocab_set.len()];
            for (word, &id) in &word2id {
                v[id] = word.clone();
            }
            v
        };

        // ── Build pairwise affinity matrix ──
        let mut affinity: HashMap<(usize, usize), u32> = HashMap::new();
        for (_cat, titles) in &curated {
            for title in titles {
                let tokens: Vec<&str> = title
                    .split_whitespace()
                    .map(|w| w.trim_matches(|c: char| c.is_ascii_punctuation() && c != '\'' && c != '-'))
                    .filter(|w| !w.is_empty())
                    .collect();

                for (i, &wa) in tokens.iter().enumerate() {
                    let wa_lower = wa.to_lowercase();
                    if let Some(&id_a) = word2id.get(&wa_lower) {
                        for (j, &wb) in tokens.iter().enumerate() {
                            if i >= j || (j - i) > AFFINITY_WINDOW {
                                continue;
                            }
                            let wb_lower = wb.to_lowercase();
                            if let Some(&id_b) = word2id.get(&wb_lower) {
                                // Store both directions for symmetric lookups
                                *affinity.entry((id_a, id_b)).or_insert(0) += 1;
                                *affinity.entry((id_b, id_a)).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
        }

        // ── Build unigram per (word, category) ──
        let mut unigram_cat: HashMap<(usize, String), u32> = HashMap::new();
        for (cat, titles) in &curated {
            for title in titles {
                let seen: HashSet<String> = tokenize(title)
                    .into_iter()
                    .filter(|w| w.len() > 1)
                    .collect();
                for word in seen {
                    if let Some(&id) = word2id.get(&word) {
                        *unigram_cat.entry((id, cat.clone())).or_insert(0) += 1;
                    }
                }
            }
        }

        // ── Build exemplar vocabulary per category ──
        let mut exemplar_vocab: HashMap<String, HashSet<String>> = HashMap::new();
        for (cat, titles) in &curated {
            let mut set = HashSet::new();
            for title in titles {
                for word in tokenize(title) {
                    if word.len() > 1 {
                        set.insert(word);
                    }
                }
            }
            exemplar_vocab.insert(cat.clone(), set);
        }

        // ── Mine intro and closer fragments ──
        // Words that should NOT appear at fragment boundaries (they create incoherent seams)
        const SEAM_STOP_WORDS: &[&str] = &[
            "a", "an", "the", "in", "on", "at", "by", "for", "with", "to", "of", "from",
            "and", "or", "but", "so", "as", "if", "than", "that", "about", "into",
            "through", "during", "before", "after", "above", "below", "between",
            "under", "over", "up", "down", "out", "off", "is", "be", "was", "are",
            "were", "been", "being", "have", "has", "had", "do", "does", "did",
            "will", "would", "can", "could", "may", "might", "shall", "should",
        ];

        let mut intro_set: HashSet<String> = HashSet::new();
        let mut closer_set: HashSet<String> = HashSet::new();
        for (_cat, titles) in &curated {
            for title in titles {
                let words: Vec<&str> = title
                    .split_whitespace()
                    .map(|w| w.trim_matches(|c: char| c.is_ascii_punctuation() && c != '\'' && c != '-'))
                    .filter(|w| !w.is_empty())
                    .collect();

                if words.len() >= 2 {
                    // Intro: first 2 words — reject if last word is a seam stop word
                    {
                        let intro2 = words[..2].join(" ");
                        let last_word_lower = words[1].to_lowercase();
                        if !SEAM_STOP_WORDS.contains(&last_word_lower.as_str()) {
                            intro_set.insert(intro2);
                        }
                    }
                    if words.len() >= 3 {
                        // Intro: first 3 words — reject if last word is a seam stop word
                        let intro3 = words[..3].join(" ");
                        let last_word_lower = words[2].to_lowercase();
                        if !SEAM_STOP_WORDS.contains(&last_word_lower.as_str()) {
                            intro_set.insert(intro3);
                        }
                    }
                    // Closer: last 2 words — reject if first word is a seam stop word
                    {
                        let closer2 = words[words.len() - 2..].join(" ");
                        let first_word_lower = words[words.len() - 2].to_lowercase();
                        if !SEAM_STOP_WORDS.contains(&first_word_lower.as_str()) {
                            closer_set.insert(closer2);
                        }
                    }
                    if words.len() >= 3 {
                        // Closer: last 3 words — reject if first word is a seam stop word
                        let closer3 = words[words.len() - 3..].join(" ");
                        let first_word_lower = words[words.len() - 3].to_lowercase();
                        if !SEAM_STOP_WORDS.contains(&first_word_lower.as_str()) {
                            closer_set.insert(closer3);
                        }
                    }
                }
            }
        }

        let intro_fragments: Vec<String> = intro_set.into_iter().collect();
        let closer_fragments: Vec<String> = closer_set.into_iter().collect();

        Generator {
            word2id,
            id2word,
            affinity,
            unigram_cat,
            templates,
            pools,
            curated,
            intro_fragments,
            closer_fragments,
            exemplar_vocab,
            all_curated,
        }
    }

    /// Generate titles using the EGCG algorithm.
    ///
    /// Produces up to `quantity` titles distributed across three modes:
    /// - Mode A (80%): exemplar-guided template fill
    /// - Mode B (10%): phrase stitching
    /// - Mode C (10%): keyword-embedded exemplar
    pub fn generate(
        &self,
        keyword: &str,
        categories: &[String],
        _style: &str,
        genre: &str,
        quantity: u32,
    ) -> Vec<TitleResult> {
        if categories.is_empty() || keyword.chars().count() <= 2 {
            return Vec::new();
        }

        let mut rng = rand::thread_rng();
        let kw_lower = keyword.to_lowercase().trim().to_string();
        let q = quantity as usize;

        let mode_a_target = (q as f64 * 0.80).ceil() as usize;
        let mode_b_target = (q as f64 * 0.10).ceil() as usize;
        let mode_c_target = q.saturating_sub(mode_a_target + mode_b_target);

        let mut results: Vec<TitleResult> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // ── Mode A: Exemplar-guided template fill ──
        for category in categories {
            let target = mode_a_target / categories.len().max(1);
            let attempts = target * 3;
            for _ in 0..attempts {
                if results.len() >= mode_a_target {
                    break;
                }
                if let Some(title) = self.fill_template_mode(
                    &kw_lower, category, genre, &mut rng,
                ) {
                    let lower = title.to_lowercase();
                    if !seen.contains(&lower)
                        && lower.contains(&kw_lower)
                        && title.split_whitespace().count() >= 3
                    {
                        let (score, breakdown) = self.score_title(&title, &kw_lower, category);
                        seen.insert(lower);
                        results.push(TitleResult {
                            title,
                            score,
                            categories: vec![category.clone()],
                            breakdown: Some(breakdown),
                        });
                    }
                }
            }
        }

        // ── Mode B: Phrase stitching ──
        for _ in 0..(mode_b_target * 3) {
            if results.len() >= mode_a_target + mode_b_target {
                break;
            }
            let cat = match categories.choose(&mut rng) {
                Some(c) => c.clone(),
                None => break,
            };
            if let Some(title) = self.stitch_mode(&kw_lower, &cat, genre, &mut rng) {
                let lower = title.to_lowercase();
                if !seen.contains(&lower)
                    && lower.contains(&kw_lower)
                    && title.split_whitespace().count() >= 3
                {
                    let (score, breakdown) = self.score_title(&title, &kw_lower, &cat);
                    seen.insert(lower);
                    results.push(TitleResult {
                        title,
                        score,
                        categories: vec![cat],
                        breakdown: Some(breakdown),
                    });
                }
            }
        }

        // ── Mode C: Keyword-embedded exemplar ──
        for _ in 0..(mode_c_target * 3) {
            if results.len() >= q {
                break;
            }
            if let Some((title, exemplar_cat)) = self.embed_mode(
                &kw_lower, categories, genre, &mut rng,
            ) {
                let lower = title.to_lowercase();
                if !seen.contains(&lower)
                    && lower.contains(&kw_lower)
                    && title.split_whitespace().count() >= 3
                {
                    let (score, breakdown) = self.score_title(&title, &kw_lower, &exemplar_cat);
                    seen.insert(lower);
                    results.push(TitleResult {
                        title,
                        score,
                        categories: vec![exemplar_cat.clone()],
                        breakdown: Some(breakdown),
                    });
                }
            }
        }

        // Sort by score, deduplicate
        results.sort_by(|a, b| b.score.cmp(&a.score));
        results.dedup_by(|a, b| a.title.eq_ignore_ascii_case(&b.title));
        results.truncate(q);
        results
    }
}

// ── Mode A: Exemplar-Guided Template Fill ──

impl Generator {
    fn fill_template_mode(
        &self,
        keyword: &str,
        category: &str,
        genre: &str,
        rng: &mut impl Rng,
    ) -> Option<String> {
        let tmpls = self.templates.get(category)?;
        let template_info: &TemplateInfo = if genre == "any" || genre.is_empty() {
            tmpls.choose(rng)?
        } else {
            // Filter by genre (exact match or template marked 'any')
            let matching: Vec<&TemplateInfo> = tmpls
                .iter()
                .filter(|t| t.genre == genre || t.genre == "any")
                .collect();
            if matching.is_empty() {
                // Fall back to any-genre templates if no exact match
                tmpls.choose(rng)?
            } else {
                matching.choose(rng).copied()?
            }
        };

        let mut filled_words: Vec<String> = Vec::new();

        for slot in &template_info.slots {
            match slot.name.as_str() {
                "keyword" | "topic" => {
                    filled_words.push(keyword.to_string());
                }
                "number" => {
                    filled_words.push(format!("{}", rng.gen_range(3..=12)));
                }
                _ => {
                    // Get candidates and score them
                    let mut candidates = self.get_candidates(slot, category);
                    if candidates.is_empty() {
                        return None;
                    }

                    // Filter out words already used in this title (Issue 3: duplicate prevention)
                    candidates.retain(|c| !filled_words.contains(c));
                    if candidates.is_empty() {
                        return None;
                    }

                    // Score candidates and pick the best one above threshold
                    let scored = self.score_candidates(
                        &candidates,
                        &filled_words,
                        keyword,
                        category,
                        rng,
                    );
                    match scored {
                        Some((word, _)) => filled_words.push(word),
                        None => {
                            // Fallback: pick randomly from candidates
                            if let Some(w) = candidates.choose(rng) {
                                filled_words.push(w.clone());
                            } else {
                                return None;
                            }
                        }
                    }
                }
            }
        }

        // Assemble the title from template with filled words
        let title = Self::assemble_title(&template_info.template, &filled_words, &template_info.slots, keyword);
        Some(title)
    }

    /// Get candidate words for a slot.
    /// Priority: exemplar vocabulary for the category → pool words → generic nouns.
    fn get_candidates(&self, slot: &SlotDef, category: &str) -> Vec<String> {
        let pool_name = slot
            .pool
            .as_deref()
            .map(|p| resolve_pool_name(p))
            .unwrap_or_else(|| resolve_pool_name(&slot.name));

        // First: exemplar words for this category that also match the pool
        let mut candidates: Vec<String> = Vec::new();
        if let Some(exemplars) = self.exemplar_vocab.get(category) {
            if let Some(pool_words) = self.pools.get(pool_name) {
                for word in pool_words {
                    let lower = word.to_lowercase();
                    if exemplars.contains(&lower) {
                        candidates.push(lower);
                    }
                }
            }
        }

        // If not enough exemplar words, add pool words
        if candidates.len() < 5 {
            if let Some(pool_words) = self.pools.get(pool_name) {
                for word in pool_words {
                    let lower = word.to_lowercase();
                    if !candidates.contains(&lower) {
                        candidates.push(lower);
                    }
                }
            }
        }

        // Fallback: generic nouns pool
        if candidates.is_empty() {
            if let Some(nouns) = self.pools.get("nouns") {
                for word in nouns {
                    candidates.push(word.to_lowercase());
                }
            }
        }

        candidates
    }

    /// Score candidates against filled context and select via softmax.
    fn score_candidates(
        &self,
        candidates: &[String],
        filled_words: &[String],
        keyword: &str,
        category: &str,
        rng: &mut impl Rng,
    ) -> Option<(String, f64)> {
        let mut scored: Vec<(String, f64)> = Vec::new();

        for cand in candidates {
            let score = self.candidate_coherence(cand, filled_words, keyword, category);
            if score >= MIN_COHERENCE {
                scored.push((cand.clone(), score));
            }
        }

        if scored.is_empty() {
            return None;
        }

        // Sort by score descending, keep top K
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(TOP_K);

        // Softmax sample from top K
        let chosen = softmax_sample(&scored, SOFTMAX_TEMP, rng);
        scored.into_iter().find(|(w, _)| w == &chosen)
    }

    /// Compute candidate coherence score against filled context.
    fn candidate_coherence(
        &self,
        candidate: &str,
        filled_words: &[String],
        keyword: &str,
        category: &str,
    ) -> f64 {
        let cand_id = match self.word2id.get(candidate) {
            Some(&id) => id,
            None => return 0.1, // unknown word gets low baseline
        };

        // Affinity with already-filled words (left context)
        let mut total_affinity = 0.0;
        let mut count = 0;
        for prev in filled_words.iter() {
            if let Some(&prev_id) = self.word2id.get(prev) {
                if let Some(&aff) = self.affinity.get(&(cand_id, prev_id)) {
                    total_affinity += aff as f64;
                    count += 1;
                }
            }
        }
        let avg_affinity = if count > 0 {
            total_affinity / count as f64
        } else {
            0.0
        };

        // Affinity with keyword
        let kw_affinity = if let Some(&kw_id) = self.word2id.get(keyword) {
            self.affinity.get(&(cand_id, kw_id)).copied().unwrap_or(0) as f64
        } else {
            0.0
        };

        // Unigram frequency in category
        let unigram = self
            .unigram_cat
            .get(&(cand_id, category.to_string()))
            .copied()
            .unwrap_or(0) as f64;

        // Stem-based lexical affinity with keyword
        let stem_score = if stem(candidate) == stem(keyword) {
            2.0
        } else {
            0.0
        };

        2.0 * (avg_affinity + 0.5 * kw_affinity)
            + 0.5 * (1.0 + unigram).ln()
            + 1.0 * stem_score
    }

    /// Assemble a title by replacing slot placeholders with filled words.
    fn assemble_title(
        template: &str,
        filled: &[String],
        slots: &[SlotDef],
        _keyword: &str,
    ) -> String {
        /// Small words that should remain lowercase in title-case when not leading.
        const SMALL_WORDS: &[&str] = &[
            "a", "an", "the", "in", "of", "to", "for", "and", "or", "but",
            "by", "with", "at", "from", "on", "as", "is", "it",
        ];

        /// Capitalize a word for title-case display.
        fn title_case_word(word: &str, is_first: bool) -> String {
            if word.is_empty() {
                return word.to_string();
            }
            let lower = word.to_lowercase();
            if !is_first && SMALL_WORDS.contains(&lower.as_str()) {
                return lower;
            }
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => word.to_string(),
            }
        }

        let mut result = template.to_string();
        for (i, slot) in slots.iter().enumerate() {
            if i < filled.len() {
                let placeholder = format!("{{{}}}", slot.name);
                let word = &filled[i];
                // Capitalize every filled word using title-case rules
                let is_first_word = result.starts_with(&placeholder);
                let replacement = title_case_word(word, is_first_word);
                result = result.replace(&placeholder, &replacement);
            }
        }
        // Fix spacing around punctuation
        result = result
            .replace(" ,", ",")
            .replace(" .", ".")
            .replace(" !", "!")
            .replace(" ?", "?")
            .replace(" :", ":");
        result
    }
}

// ── Mode B: Phrase Stitching ──

impl Generator {
    fn stitch_mode(
        &self,
        keyword: &str,
        _category: &str,
        _genre: &str,
        rng: &mut impl Rng,
    ) -> Option<String> {
        let intro = self.intro_fragments.choose(rng)?;
        let closer = self.closer_fragments.choose(rng)?;

        // Clean up intro and closer fragments
        let intro_clean = intro
            .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace());
        let closer_clean = closer
            .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace());

        let title = format!("{} {} {}", intro_clean, keyword, closer_clean);

        // Validate: title should have reasonable length and not be too repetitive
        if title.split_whitespace().count() < 3 || title.len() > 200 {
            return None;
        }

        // Capitalize first letter
        let mut chars = title.chars();
        let capitalized = match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => title,
        };

        Some(capitalized)
    }
}

// ── Mode C: Keyword-Embedded Exemplar ──

impl Generator {
    fn embed_mode(
        &self,
        keyword: &str,
        categories: &[String],
        genre: &str,
        rng: &mut impl Rng,
    ) -> Option<(String, String)> {
        // Filter curated titles to matching categories AND matching genre
        let cat_set: HashSet<&String> = categories.iter().collect();
        let relevant: Vec<&(String, String, String)> = self
            .all_curated
            .iter()
            .filter(|(_, cat, g)| {
                cat_set.contains(cat) && 
                (genre == "any" || genre.is_empty() || g == genre || g == "any")
            })
            .collect();

        if relevant.is_empty() {
            // Fall back to any genre within matching categories
            let fallback: Vec<&(String, String, String)> = self
                .all_curated
                .iter()
                .filter(|(_, cat, _)| cat_set.contains(cat))
                .collect();
            if fallback.is_empty() {
                return None;
            }
            return self.embed_from_relevant(keyword, &fallback, rng);
        }

        self.embed_from_relevant(keyword, &relevant, rng)
    }

    fn embed_from_relevant(
        &self,
        keyword: &str,
        relevant: &[&(String, String, String)],
        rng: &mut impl Rng,
    ) -> Option<(String, String)> {

        // Handle multi-word keywords: use first token for lookup
        let kw_tokens: Vec<&str> = keyword.split_whitespace().collect();
        let kw_id = match self.word2id.get(kw_tokens[0]) {
            Some(&id) => id,
            None => return None,
        };
        let mut best_title: Option<&str> = None;
        let mut best_cat: &str = "";
        let mut best_score: f64 = f64::NEG_INFINITY;

        // Sample a subset for performance (up to 50)
        let sample: Vec<&&(String, String, String)> = if relevant.len() > 50 {
            relevant.choose_multiple(rng, 50).collect()
        } else {
            relevant.iter().collect()
        };

        for (title, cat, _g) in &sample {
            let tokens = tokenize(title);
            let mut total_aff = 0.0;
            let mut count = 0;

            for word in &tokens {
                if let Some(&id) = self.word2id.get(word) {
                    if let Some(&aff) = self.affinity.get(&(kw_id, id)) {
                        total_aff += aff as f64;
                        count += 1;
                    }
                }
            }

            if count > 0 {
                let score = total_aff / count as f64;
                if score > best_score {
                    best_score = score;
                    best_title = Some(title);
                    best_cat = cat;
                }
            }
        }

        let curated_title = best_title?;
        let exemplar_category = best_cat.to_string();

        // Find the best position to swap the keyword
        let words: Vec<&str> = curated_title
            .split_whitespace()
            .collect();

        // Try each position: replace the word with the keyword
        let mut best_variant: Option<String> = None;
        let mut best_variant_score: f64 = f64::NEG_INFINITY;

        for i in 0..words.len() {
            let mut new_words: Vec<String> = words
                .iter()
                .take(i)
                .map(|w| {
                    w.trim_matches(|c: char| c.is_ascii_punctuation() && c != '\'' && c != '-')
                        .to_string()
                })
                .collect();

            new_words.push(keyword.to_string());

            for w in &words[i + 1..] {
                new_words.push(
                    w.trim_matches(|c: char| c.is_ascii_punctuation() && c != '\'' && c != '-')
                        .to_string(),
                );
            }

            let candidate = new_words.join(" ");

            // Score this variant
            let score = self.variant_affinity(&candidate, &kw_id);
            if score > best_variant_score {
                best_variant_score = score;
                best_variant = Some(candidate);
            }
        }

        best_variant.map(|v| (v, exemplar_category))
    }

    fn variant_affinity(&self, title: &str, kw_id: &usize) -> f64 {
        let tokens = tokenize(title);
        let mut total = 0.0;
        let mut count = 0;

        for i in 0..tokens.len() {
            if let Some(&id_a) = self.word2id.get(&tokens[i]) {
                for j in (i + 1)..tokens.len().min(i + AFFINITY_WINDOW + 1) {
                    if let Some(&id_b) = self.word2id.get(&tokens[j]) {
                        if id_a == *kw_id || id_b == *kw_id {
                            if let Some(&aff) = self.affinity.get(&(id_a, id_b)) {
                                total += aff as f64;
                                count += 1;
                            }
                        }
                    }
                }
            }
        }

        if count > 0 {
            total / count as f64
        } else {
            0.0
        }
    }
}

// ── Scoring ──

impl Generator {
    /// Score a generated title using the EGCG coherence formula
    /// combined with traditional heuristics, normalized to 0-100.
    fn score_title(
        &self,
        title: &str,
        keyword: &str,
        category: &str,
    ) -> (u32, serde_json::Value) {
        let lower = title.to_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();
        let word_count = words.len();

        // ── EGCG coherence score ──
        let mut total_affinity = 0.0;
        let mut affinity_pairs = 0;

        let tokens = tokenize(title);
        for i in 0..tokens.len() {
            if let Some(&id_a) = self.word2id.get(&tokens[i]) {
                for j in (i + 1)..tokens.len().min(i + AFFINITY_WINDOW + 1) {
                    if let Some(&id_b) = self.word2id.get(&tokens[j]) {
                        if let Some(&aff) = self.affinity.get(&(id_a, id_b)) {
                            total_affinity += aff as f64;
                            affinity_pairs += 1;
                        }
                    }
                }
            }
        }

        let avg_affinity = if affinity_pairs > 0 {
            total_affinity / affinity_pairs as f64
        } else {
            0.0
        };

        // Unigram sum
        let mut unigram_sum = 0.0;
        if let Some(&kw_id) = self.word2id.get(keyword) {
            unigram_sum += self
                .unigram_cat
                .get(&(kw_id, category.to_string()))
                .copied()
                .unwrap_or(0) as f64;
        }
        for word in tokenize(title) {
            if let Some(&id) = self.word2id.get(&word) {
                unigram_sum += self
                    .unigram_cat
                    .get(&(id, category.to_string()))
                    .copied()
                    .unwrap_or(0) as f64;
            }
        }

        // Repeat penalty
        let mut word_freq: HashMap<&str, usize> = HashMap::new();
        for w in &words {
            *word_freq.entry(w).or_insert(0) += 1;
        }
        let repeat_penalty: f64 = word_freq
            .values()
            .map(|&c| if c > 1 { (c - 1) as f64 } else { 0.0 })
            .sum();

        // EGCG raw score
        let egcg_raw = 2.0 * avg_affinity
            + 0.5 * (1.0 + unigram_sum).ln()
            - 1.5 * repeat_penalty;

        // Normalize to base score (0-65 range)
        let base_score = (egcg_raw.max(0.0) * 10.0).min(65.0) as u32;

        // ── Heuristic bonuses ──
        let mut score = base_score;

        // Keyword presence
        if lower.contains(keyword) {
            score += 15;
        } else if keyword
            .split_whitespace()
            .any(|w| lower.contains(w))
        {
            score += 8;
        }

        // Numbers
        if title.chars().any(|c| c.is_ascii_digit()) {
            score += 8;
        }

        // Curiosity markers
        if title.contains('?') || title.contains(':') || title.contains("...") {
            score += 8;
        }

        // Emotional words
        let emotional = [
            "secret", "hidden", "truth", "never", "wrong", "best", "worst",
            "ultimate", "essential", "proven", "easy", "fast", "simple",
        ];
        if emotional.iter().any(|w| lower.contains(w)) {
            score += 6;
        }

        // Power words
        let power = [
            "why", "how", "what", "when", "stop", "start", "transform",
            "unlock", "master", "hack", "build", "create",
        ];
        if power.iter().any(|w| lower.contains(w)) {
            score += 4;
        }

        // Word count sweet spot
        if word_count >= 4 && word_count <= 12 {
            score += 8;
        } else if word_count >= 2 && word_count <= 16 {
            score += 4;
        } else {
            score = score.saturating_sub(6);
        }

        // Cap at 100
        score = score.min(100);

        // ── Build breakdown ──
        let curiosity_gap = if title.contains('?')
            || title.contains(':')
            || title.contains("...")
        {
            "High"
        } else if title.chars().any(|c| c.is_ascii_digit()) {
            "Medium"
        } else {
            "Low"
        };

        let emotional_trigger = if emotional.iter().any(|w| lower.contains(w)) {
            if lower.contains("secret") || lower.contains("hidden") {
                "curiosity"
            } else if lower.contains("truth") || lower.contains("never") || lower.contains("wrong")
            {
                "surprise"
            } else if lower.contains("best")
                || lower.contains("ultimate")
                || lower.contains("essential")
                || lower.contains("easy")
                || lower.contains("fast")
                || lower.contains("simple")
            {
                "aspiration"
            } else {
                "curiosity"
            }
        } else if title.chars().any(|c| c.is_ascii_digit()) {
            "curiosity"
        } else {
            "neutral"
        };

        let mut power_words_found: Vec<&str> = Vec::new();
        for w in &power {
            if lower.contains(w) {
                power_words_found.push(w);
            }
        }
        for w in &emotional {
            if lower.contains(w) && !power_words_found.contains(w) {
                power_words_found.push(w);
            }
        }

        let specificity = if lower.contains(keyword) || title.chars().any(|c| c.is_ascii_digit())
        {
            "Concrete"
        } else {
            "Abstract"
        };

        let length_analysis = if word_count <= 3 {
            format!("Short ({} words)", word_count)
        } else if word_count <= 8 {
            format!("Optimal ({} words)", word_count)
        } else {
            format!("Long ({} words)", word_count)
        };

        let breakdown = serde_json::json!({
            "curiosityGap": curiosity_gap,
            "emotionalTrigger": emotional_trigger,
            "powerWords": power_words_found,
            "lengthAnalysis": length_analysis,
            "specificity": specificity,
            "egcgCoherence": format!("{:.2}", egcg_raw),
        });

        (score, breakdown)
    }
}

// ── Utility Functions ──

/// Tokenize a title into lowercase words, stripping most punctuation.
pub fn tokenize(title: &str) -> Vec<String> {
    title
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| c.is_ascii_punctuation() && c != '\'' && c != '-')
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Crude stemmer: strip common English suffixes for lexical affinity matching.
pub fn stem(word: &str) -> String {
    let w = word.to_lowercase();
    let suffixes = [
        "ingly", "edly", "ment", "tion", "sion", "ness", "able", "ible", "ical",
        "ful", "less", "ous", "ive", "ies",
        "ing", "ed", "ly", "es", "er", "est", "or", "al", "ic", "s",
    ];

    for suffix in &suffixes {
        if w.ends_with(suffix) && w.len() > suffix.len() + 1 {
            return w[..w.len() - suffix.len()].to_string();
        }
    }

    w
}

/// Softmax-weighted sampling from scored candidates.
///
/// Returns the chosen word string.
fn softmax_sample(
    scored: &[(String, f64)],
    temperature: f64,
    rng: &mut impl Rng,
) -> String {
    if scored.is_empty() {
        return String::new();
    }

    if scored.len() == 1 {
        return scored[0].0.clone();
    }

    // Compute softmax probabilities
    let max_score = scored
        .iter()
        .map(|(_, s)| *s)
        .fold(f64::NEG_INFINITY, f64::max);

    let weights: Vec<f64> = scored
        .iter()
        .map(|(_, s)| ((s - max_score) / temperature.max(0.01)).exp())
        .collect();

    let total: f64 = weights.iter().sum();

    if total <= 0.0 {
        // Uniform fallback
        return scored[rng.gen_range(0..scored.len())].0.clone();
    }

    let mut r = rng.gen::<f64>() * total;
    for (i, w) in weights.iter().enumerate() {
        r -= w;
        if r <= 0.0 {
            return scored[i].0.clone();
        }
    }

    // Fallback: last item
    scored.last().unwrap().0.clone()
}

// ── Pool Name Resolution ──

/// Map specialized pool names to the standard pool names used in the database.
/// Duplicate of the function in engine.rs to avoid circular dependency.
pub fn resolve_pool_name(slot_name: &str) -> &'static str {
    match slot_name {
        "verb" | "verbs" | "action_verb" | "action_verbs" | "action_verbs_ing"
        | "action_verbs_past" | "actions_positive" | "positive_action"
        | "positive_action_verb" | "positive_action_verbs" | "comparison_verb"
        | "comparison_verbs" | "imperative_verb" | "imperative_verbs"
        | "negative_action_verb" | "negative_action_verbs" | "transformational_verb"
        | "transformational_verbs" | "gerund_verb" | "gerund_verbs" | "gerund_verb_2"
        | "gerund" | "gerunds" | "verb_ing" | "verbing" | "verb_ing2" | "gerund2"
        | "right_verb" | "wrong_verb" | "verb_alt" | "verb_2" | "verb_past" => "action_verbs",

        "adjective" | "adjectives" | "power_adjective" | "power_adjectives"
        | "positive_adjective" | "positive_adjectives" | "negative_adjective"
        | "negative_adjectives" | "overused_adjective" | "overused_adjectives"
        | "contrarian_adjective" | "contrarian_adjectives" | "comparative_adjective"
        | "comparative_adjectives" | "descriptive_adjective" | "descriptive_adjectives"
        | "opinion_adjective" | "opinion_adjectives" | "adjectives_describing_movies"
        | "character_adjective" | "character_adjectives" | "superlative_adjective"
        | "superlative_adjectives" | "superlative" | "adjective1" | "adjective2"
        | "adjective_2" | "adjective_alt" | "adjective_opinion" => "power_adjectives",

        "noun" | "nouns" | "common_noun" | "common_nouns" | "abstract_noun"
        | "abstract_nouns" | "nouns_contrast" | "nouns_identity" | "nouns_opposite"
        | "nouns_persona" | "nouns_plural" | "concept" | "concepts" | "theme"
        | "themes" | "scenario" | "scenarios" | "movie_topic" | "movie_topics"
        | "street_topic" | "street_topics" | "trend" | "trends" | "life_domain"
        | "life_domains" | "life_lesson" | "life_lessons" | "movie_element"
        | "movie_elements" | "profession_or_role" | "professions_or_roles"
        | "profession" | "professions" | "experience" | "experiences" | "audience"
        | "audiences" | "audience_type" | "audience_types" | "audience2" | "name"
        | "names" | "pronoun" | "pronouns" | "actor" | "actors" | "director"
        | "directors" | "director1" | "director2" | "genre" | "genres"
        | "different_genre" | "film_achievement" | "film_achievements"
        | "production_event" | "production_events" | "movie_title" | "movie_titles"
        | "adverb" | "adverbs" | "character_element" | "character_elements"
        | "common_pitfall" | "common_pitfalls" | "pitfall" | "pitfalls" | "topic"
        | "topics" | "topic1" | "topic2" | "topic_2" | "subject" | "another_movie"
        | "movie_aspect" | "movie_genre" | "life_aspect" | "event" | "lesson"
        | "opposite_noun" | "noun_a" | "noun_b" | "noun_alt" | "noun1" | "noun2"
        | "noun3" | "negative_trait" | "negative_traits" | "positive_trait"
        | "positive_traits" | "positive_emotion" | "positive_emotions"
        | "negative_emotion" | "negative_emotions" => "nouns",

        "result" | "results" | "outcome" | "outcomes" | "desired_outcome"
        | "desired_outcomes" | "desired_result" | "desired_results" | "benefit"
        | "benefits" | "achievement" => "results",

        "timeframe" | "timeframes" | "time" | "times" | "time_day" | "time_of_day"
        | "times_day" | "decade" | "decades" | "decade2" => "timeframes",

        "emotion" | "emotions" | "emotional_state" | "emotional_states"
        | "emotions_adj" | "emotion2" | "emotions_negative" => "emotions",

        "number" | "numbers" | "list_number" | "list_numbers" => "numbers",

        "hook" | "hooks" | "question_word" | "question_words" | "alternative"
        | "alternatives" => "hooks",

        _ => "nouns",
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn test_tokenize_basic() {
        let tokens = tokenize("The Art of Productivity");
        assert_eq!(tokens, vec!["the", "art", "of", "productivity"]);
    }

    #[test]
    fn test_tokenize_lowercases() {
        let tokens = tokenize("HELLO WORLD");
        assert_eq!(tokens, vec!["hello", "world"]);
    }

    #[test]
    fn test_tokenize_strips_punctuation() {
        let tokens = tokenize("What's in a name?");
        assert!(tokens.contains(&"name".to_string()));
        assert!(!tokens.contains(&"?".to_string()));
    }

    #[test]
    fn test_stem_basic() {
        assert_eq!(stem("running"), "runn");
        assert_eq!(stem("transformation"), "transforma");
        assert_eq!(stem("happiness"), "happi");
        assert_eq!(stem("create"), "create");
    }

    #[test]
    fn test_stem_preserves_short_words() {
        assert_eq!(stem("go"), "go");
        assert_eq!(stem("be"), "be");
    }

    #[test]
    fn test_softmax_sample_single() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let scored = vec![("only".to_string(), 1.0)];
        let result = softmax_sample(&scored, 0.7, &mut rng);
        assert_eq!(result, "only");
    }

    #[test]
    fn test_softmax_sample_multiple() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let scored = vec![
            ("a".to_string(), 10.0),
            ("b".to_string(), 5.0),
            ("c".to_string(), 1.0),
        ];
        let result = softmax_sample(&scored, 0.7, &mut rng);
        assert!(!result.is_empty());
        assert!(result == "a" || result == "b" || result == "c");
    }

    #[test]
    fn test_softmax_sample_empty() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let scored: Vec<(String, f64)> = vec![];
        let result = softmax_sample(&scored, 0.7, &mut rng);
        assert_eq!(result, "");
    }

    /// Test that Generator can be built, even without a real database.
    #[test]
    fn test_generator_build_empty_db() {
        // Use an in-memory database with the expected schema
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE curated_titles (id INTEGER PRIMARY KEY, title TEXT, category TEXT, genre TEXT, tone TEXT, appeal_score INTEGER, notes TEXT);
             CREATE TABLE word_pools (id INTEGER PRIMARY KEY, pool_name TEXT, word TEXT);
             CREATE TABLE patterns (id INTEGER PRIMARY KEY, category TEXT, template TEXT, slots TEXT, genre TEXT, tone TEXT, quality_score REAL);",
        )
        .unwrap();

        let gen = Generator::build(&conn);
        assert!(gen.id2word.is_empty());
        assert!(gen.all_curated.is_empty());
    }

    /// Test that the Generator is Send (compile-time check).
    #[test]
    fn test_generator_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Generator>();
    }
}
