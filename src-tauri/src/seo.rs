//! SEO Scoring — 9 locally-computed signals for title quality.
//!
//! Every signal is pure math + static lexicons: no API calls, no new crate deps.

use serde::Serialize;
use std::collections::HashSet;

const W_LENGTH: u32 = 20;
const W_KW_PRESENCE: u32 = 20;
const W_KW_DENSITY: u32 = 10;
const W_PATTERN: u32 = 15;
const W_QUESTION: u32 = 5;
const W_NUMBER: u32 = 10;
const W_READING: u32 = 5;
const W_POWER: u32 = 5;
const W_UNIQUENESS: u32 = 10;

#[derive(Serialize, Clone, Debug)]
pub struct SignalDetail {
    pub score: u32,
    pub weight: u32,
    pub value: String,
    pub detail: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct SeoBreakdown {
    pub platform: String,
    pub length_fit: SignalDetail,
    pub keyword_presence: SignalDetail,
    pub keyword_density: SignalDetail,
    pub search_pattern: SignalDetail,
    pub question_format: SignalDetail,
    pub number_year: SignalDetail,
    pub reading_level: SignalDetail,
    pub power_words: SignalDetail,
    pub uniqueness: SignalDetail,
}

pub struct SeoScorer {
    curated_bigrams: HashSet<String>,
    curated_trigrams: HashSet<String>,
}

impl SeoScorer {
    pub fn new() -> Self {
        SeoScorer { curated_bigrams: HashSet::new(), curated_trigrams: HashSet::new() }
    }

    pub fn from_curated(titles: &[String]) -> Self {
        let mut big = HashSet::new();
        let mut tri = HashSet::new();
        for t in titles {
            let toks = tokenize(t);
            for b in bigrams(&toks) { big.insert(b); }
            for t3 in trigrams(&toks) { tri.insert(t3); }
        }
        SeoScorer { curated_bigrams: big, curated_trigrams: tri }
    }

    pub fn score_uniqueness(&self, title: &str) -> SignalDetail {
        if self.curated_bigrams.is_empty() {
            return sig(60, W_UNIQUENESS, "n/a".to_string(), "no reference corpus available".to_string());
        }
        let twords = tokenize(title);
        if twords.len() < 2 {
            return sig(50, W_UNIQUENESS, "n/a".to_string(), "too short for n-gram analysis".to_string());
        }
        let tbi = bigrams(&twords);
        let ttri = trigrams(&twords);
        if tbi.is_empty() {
            return sig(50, W_UNIQUENESS, "n/a".to_string(), "no bigrams".to_string());
        }
        let b_overlap = tbi.iter().filter(|b| self.curated_bigrams.contains(*b)).count() as f64 / tbi.len() as f64;
        let t_overlap = if ttri.is_empty() { b_overlap } else {
            ttri.iter().filter(|t| self.curated_trigrams.contains(*t)).count() as f64 / ttri.len() as f64
        };
        let overlap = (b_overlap + t_overlap) / 2.0;
        let (score, detail) = if overlap <= 0.1 { (100, "very low overlap — highly novel".to_string()) }
        else if overlap <= 0.25 { (85, "low overlap — mostly novel".to_string()) }
        else if overlap <= 0.4 { (65, "moderate overlap".to_string()) }
        else if overlap <= 0.6 { (45, "substantial overlap with corpus".to_string()) }
        else if overlap <= 0.8 { (25, "high overlap — derivative".to_string()) }
        else { (10, "very high overlap — likely unoriginal".to_string()) };
        sig(score, W_UNIQUENESS, format!("{:.0}% novel", (1.0 - overlap) * 100.0), detail)
    }

    pub fn score_seo(&self, title: &str, keyword: &str, category: &str, platform_target: &str) -> (u8, SeoBreakdown) {
        let platform = normalize_platform(platform_target, category).to_string();
        let length_fit = score_length(title, &platform);
        let keyword_presence = score_keyword_presence(title, keyword);
        let keyword_density = score_keyword_density(title, keyword);
        let search_pattern = score_search_patterns(title);
        let question_format = score_question(title);
        let number_year = score_number_year(title, current_year());
        let reading_level = score_reading_level(title);
        let power_words = score_power_words(title);
        let uniqueness = self.score_uniqueness(title);

        let total = (length_fit.score * W_LENGTH + keyword_presence.score * W_KW_PRESENCE
            + keyword_density.score * W_KW_DENSITY + search_pattern.score * W_PATTERN
            + question_format.score * W_QUESTION + number_year.score * W_NUMBER
            + reading_level.score * W_READING + power_words.score * W_POWER
            + uniqueness.score * W_UNIQUENESS) / 100;
        let total = total.min(100) as u8;

        (total, SeoBreakdown { platform: platform.clone(), length_fit, keyword_presence, keyword_density, search_pattern, question_format, number_year, reading_level, power_words, uniqueness })
    }
}

impl Default for SeoScorer { fn default() -> Self { Self::new() } }

#[allow(dead_code)]
pub fn score_seo(title: &str, keyword: &str, category: &str, platform_target: &str) -> (u8, SeoBreakdown) {
    SeoScorer::new().score_seo(title, keyword, category, platform_target)
}

pub fn platform_for_category(category: &str) -> &'static str {
    match category.to_lowercase().as_str() {
        "youtube" => "youtube", "book" | "ebook" | "product" => "amazon", _ => "google",
    }
}

pub fn normalize_platform(platform: &str, category: &str) -> String {
    match platform.to_lowercase().as_str() {
        "google" | "youtube" | "amazon" | "generic" => platform.to_lowercase(),
        _ => platform_for_category(category).to_string(),
    }
}

fn current_year() -> i32 { chrono::Utc::now().format("%Y").to_string().parse::<i32>().unwrap_or(2026) }

fn length_sweet_spot(platform: &str) -> (usize, usize, usize, usize) {
    match platform { "google" => (50, 60, 45, 66), "youtube" => (50, 70, 40, 80), "amazon" => (60, 100, 50, 120), _ => (40, 70, 30, 90) }
}

pub fn score_length(title: &str, platform: &str) -> SignalDetail {
    let chars = title.trim().chars().count();
    let (imin, imax, amin, amax) = length_sweet_spot(platform);
    let (score, detail) = if chars == 0 { (0, "empty title".to_string()) }
    else if chars >= imin && chars <= imax { (100, format!("{} chars — ideal for {}", chars, platform)) }
    else if chars >= amin && chars <= amax { (75, format!("{} chars — acceptable for {}", chars, platform)) }
    else {
        let dist = if chars < amin { amin - chars } else { chars - amax };
        let s = 50u32.saturating_sub((dist as u32) * 2).max(15);
        let dir = if chars < amin { "short" } else { "long" };
        (s, format!("{} chars — too {} for {}", chars, dir, platform))
    };
    sig(score, W_LENGTH, format!("{} chars", chars), detail)
}

pub fn score_keyword_presence(title: &str, keyword: &str) -> SignalDetail {
    let twords = tokenize(title);
    let kwords = tokenize(keyword);
    if kwords.is_empty() { return sig(60, W_KW_PRESENCE, "n/a".to_string(), "no keyword provided".to_string()); }
    let prefix_match = twords.len() >= kwords.len() && twords.iter().zip(kwords.iter()).take(kwords.len()).all(|(a, b)| a == b);
    if prefix_match { return sig(100, W_KW_PRESENCE, "front-loaded".to_string(), "keyword leads the title".to_string()); }
    let tl = title.to_lowercase();
    let kl = keyword.to_lowercase();
    if !kl.is_empty() && tl.contains(&kl) { return sig(75, W_KW_PRESENCE, "present".to_string(), "keyword appears in the title".to_string()); }
    let any = kwords.iter().any(|w| twords.contains(w));
    if any { sig(45, W_KW_PRESENCE, "partial".to_string(), "partial keyword match".to_string()) }
    else { sig(0, W_KW_PRESENCE, "absent".to_string(), "keyword not found".to_string()) }
}

pub fn score_keyword_density(title: &str, keyword: &str) -> SignalDetail {
    let twords = tokenize(title);
    let kwords = tokenize(keyword);
    if twords.is_empty() || kwords.is_empty() { return sig(0, W_KW_DENSITY, "0%".to_string(), "no keyword".to_string()); }
    let occ = twords.iter().filter(|w| kwords.contains(w)).count();
    let density = (occ as f64 / twords.len() as f64) * 100.0;
    let value = format!("{:.0}%", density);
    let (score, detail) = if density >= 10.0 && density <= 25.0 { (100, "density in sweet spot (10-25%)".to_string()) }
    else if (density >= 5.0 && density < 10.0) || (density > 25.0 && density <= 35.0) { (70, "density slightly off sweet spot".to_string()) }
    else if (density >= 1.0 && density < 5.0) || (density > 35.0 && density <= 50.0) { (40, "density low or high".to_string()) }
    else if density > 50.0 { (20, "keyword stuffing risk".to_string()) }
    else { (0, "keyword absent".to_string()) };
    sig(score, W_KW_DENSITY, value, detail)
}

pub fn score_search_patterns(title: &str) -> SignalDetail {
    let lower = title.to_lowercase();
    let mut matches = 0usize;
    let mut matched: Vec<&str> = Vec::new();
    for &p in SEARCH_PATTERNS { if contains_phrase(&lower, p) { matches += 1; matched.push(p); if matches >= 3 { break; } } }
    let (score, detail) = match matches { 0 => (20, "no common search patterns matched".to_string()), 1 => (60, format!("matched: {}", matched.join(", "))), 2 => (85, format!("matched: {}", matched.join(", "))), _ => (100, format!("matched: {}", matched.join(", "))) };
    sig(score, W_PATTERN, format!("{} match(es)", matches), detail)
}

pub fn score_question(title: &str) -> SignalDetail {
    let first_raw = title.trim().split_whitespace().next().unwrap_or("");
    let first_word: String = first_raw.trim_end_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    let q_words = ["who","what","when","where","why","how","which","can","could","should","would","will","do","does","is","are"];
    let is_q = !first_word.is_empty() && q_words.contains(&first_word.as_str());
    let ends_q = title.trim_end().ends_with('?');
    let (score, detail) = if is_q && ends_q { (100, "question word + question mark".to_string()) }
    else if is_q { (85, "starts with a question word".to_string()) }
    else if ends_q { (70, "ends with a question mark".to_string()) }
    else { (0, "not a question".to_string()) };
    sig(score, W_QUESTION, if is_q || ends_q { "yes" } else { "no" }.to_string(), detail)
}

pub fn score_number_year(title: &str, current_year: i32) -> SignalDetail {
    let has_digit = title.chars().any(|c| c.is_ascii_digit());
    if !has_digit { return sig(0, W_NUMBER, "none".to_string(), "no numbers present".to_string()); }
    let mut year_hit = false;
    let mut buf = String::new();
    let flush = |buf: &mut String, hit: &mut bool| {
        if buf.len() == 4 { if let Ok(n) = buf.parse::<i32>() { if n >= current_year - 2 && n <= current_year + 2 { *hit = true; } } }
        buf.clear();
    };
    for c in title.chars() { if c.is_ascii_digit() { buf.push(c); } else { flush(&mut buf, &mut year_hit); } }
    flush(&mut buf, &mut year_hit);
    if year_hit { sig(100, W_NUMBER, format!("{}±2", current_year), format!("contains current year (±2) — {}", current_year)) }
    else { sig(70, W_NUMBER, "number".to_string(), "contains a number".to_string()) }
}

pub fn score_reading_level(title: &str) -> SignalDetail {
    let words: Vec<&str> = title.split_whitespace().filter(|s| !s.is_empty()).collect();
    if words.len() < 3 { return sig(60, W_READING, "n/a".to_string(), "too short for reliable reading-level estimate".to_string()); }
    let f = flesch_reading_ease(title);
    let (score, band) = if f >= 60.0 && f <= 80.0 { (100, "easy (ideal 60-80)") }
    else if (f >= 50.0 && f < 60.0) || (f > 80.0 && f <= 90.0) { (75, "fairly easy / readable") }
    else if (f >= 30.0 && f < 50.0) || (f > 90.0 && f <= 100.0) { (50, "slightly off ideal") }
    else { (25, "hard or anomalous") };
    sig(score, W_READING, format!("Flesch {:.0}", f), band.to_string())
}

fn flesch_reading_ease(title: &str) -> f64 {
    let words: Vec<&str> = title.split_whitespace().filter(|s| !s.is_empty()).collect();
    if words.is_empty() { return 0.0; }
    let word_count = words.len() as f64;
    let sentence_count = words.iter().map(|w| w.chars().filter(|c| matches!(c, '.' | '!' | '?')).count()).sum::<usize>().max(1) as f64;
    let syllable_count: usize = words.iter().map(|w| count_syllables(w)).sum();
    if syllable_count == 0 { return 0.0; }
    206.835 - 1.015 * (word_count / sentence_count) - 84.6 * (syllable_count as f64 / word_count)
}

fn count_syllables(word: &str) -> usize {
    let w: Vec<char> = word.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect();
    if w.is_empty() { return 0; }
    let is_vowel = |c: char| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
    let mut count = 0usize;
    let mut prev_v = false;
    for &c in &w { let v = is_vowel(c); if v && !prev_v { count += 1; } prev_v = v; }
    if count > 1 && *w.last().unwrap() == 'e' { count -= 1; }
    if count == 0 { count = 1; }
    count
}

pub fn score_power_words(title: &str) -> SignalDetail {
    let twords = tokenize(title);
    let count = twords.iter().filter(|w| POWER_WORDS.contains(&w.as_str())).count();
    let (score, detail) = match count { 0 => (20, "no power words".to_string()), 1 => (60, "1 power word".to_string()), 2 => (85, "2 power words".to_string()), _ => (100, format!("{} power words (capped contribution)", count)) };
    sig(score, W_POWER, format!("{} power word(s)", count), detail)
}

fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_string()).collect()
}

fn bigrams(words: &[String]) -> Vec<String> { words.windows(2).map(|w| format!("{}|{}", w[0], w[1])).collect() }
fn trigrams(words: &[String]) -> Vec<String> { words.windows(3).map(|w| format!("{}|{}|{}", w[0], w[1], w[2])).collect() }

fn contains_phrase(hay: &str, needle: &str) -> bool {
    if needle.is_empty() { return false; }
    let hay_l = hay.to_lowercase();
    let needle_l = needle.to_lowercase();
    for (i, _) in hay_l.char_indices() {
        if hay_l[i..].starts_with(&needle_l) {
            let before_ok = i == 0 || !hay_l[..i].chars().last().map(|c| c.is_alphanumeric()).unwrap_or(false);
            let after_idx = i + needle_l.len();
            let after_ok = after_idx >= hay_l.len() || !hay_l[after_idx..].chars().next().map(|c| c.is_alphanumeric()).unwrap_or(false);
            if before_ok && after_ok { return true; }
        }
    }
    false
}

fn sig(score: u32, weight: u32, value: String, detail: String) -> SignalDetail {
    SignalDetail { score: score.min(100), weight, value, detail }
}

const SEARCH_PATTERNS: &[&str] = &[
    "how to","why","what","when","where","which","who","guide to","ultimate guide","complete guide","definitive guide",
    "beginner guide","for beginners","step by step","step-by-step","reasons","reasons why","ways to","ways","tips for",
    "tips on","tips","secrets of","secrets to","truth about","the truth","everything you need","need to know",
    "you need to know","explained","101","basics","fundamentals","mastery","master","masterclass","proven","simple",
    "easy way","easy","quick","quick way","fast","boost","increase","improve","transform","unlock","unleash","discover",
    "revealed","inside","behind the","behind","mistakes","common mistakes","avoid","stop","start","learn","learn how",
    "lessons","lesson","ideas","examples","templates","checklist","cheat sheet","strategies","strategy","tactics",
    "blueprint","framework","system","method","methods","formula","hack","hacks","tricks","trick","rules","rule",
    "principles","principle","laws","law","science of","art of","power of","myth","myths","facts","case study",
    "case studies","review","reviews","comparison","versus","best","top","worst","ultimate","essential","every",
    "everything","warning","breaking","updated","new","now","today","trends","future of","predict","forecast",
    "beginner","advanced","expert","pro",
];

const POWER_WORDS: &[&str] = &[
    "secret","hidden","truth","never","wrong","best","worst","ultimate","essential","proven","easy","fast","simple",
    "every","anyone","nobody","everyone","always","forever","impossible","possible","why","how","what","when","stop",
    "start","transform","unlock","master","hack","build","create","destroy","save","kill","love","hate","free","new",
    "now","instantly","money","win","warning","danger","exclusive","limited","breakthrough","guarantee","results",
    "boost","powerful","killer","brutal","shocking","stunning","incredible","amazing","effortless",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn syllables_basic() {
        assert_eq!(count_syllables(""), 0);
        assert_eq!(count_syllables("cat"), 1);
        assert_eq!(count_syllables("cake"), 1);
        assert_eq!(count_syllables("running"), 2);
    }

    #[test] fn length_sweet_spot() {
        assert_eq!(score_length(&"a".repeat(55), "google").score, 100);
        assert!(score_length(&"a".repeat(100), "google").score <= 20);
        assert_eq!(score_length(&"a".repeat(60), "youtube").score, 100);
    }

    #[test] fn keyword_front_loaded() {
        assert_eq!(score_keyword_presence("How to Bake Bread", "how to bake").score, 100);
        assert_eq!(score_keyword_presence("Bread and How to Bake It", "how to bake").score, 75);
        assert_eq!(score_keyword_presence("Completely Unrelated", "how to bake").score, 0);
    }

    #[test] fn question_detection() {
        assert_eq!(score_question("Why do birds sing?").score, 100);
        assert_eq!(score_question("Why birds sing").score, 85);
        assert_eq!(score_question("Birds sing?").score, 70);
        assert_eq!(score_question("Birds sing loudly").score, 0);
    }

    #[test] fn number_year() {
        assert_eq!(score_number_year("Best Tips for 2026", 2026).score, 100);
        assert_eq!(score_number_year("5 Tips for Writers", 2026).score, 70);
        assert_eq!(score_number_year("No numbers here", 2026).score, 0);
    }

    #[test] fn uniqueness_corpus() {
        let scorer = SeoScorer::from_curated(&["the quick brown fox jumps".to_string()]);
        assert!(scorer.score_uniqueness("completely different unusual words here").score >= 85);
        assert!(scorer.score_uniqueness("the quick brown fox jumps").score <= 25);
    }

    #[test] fn phrase_boundary() {
        assert!(contains_phrase("10 vs 20 comparison", "vs"));
        assert!(!contains_phrase("advise caution", "vs"));
        assert!(contains_phrase("How to bake", "how to"));
    }

    #[test] fn end_to_end() {
        let (score, bd) = score_seo("How to Build a Startup in 2026", "build a startup", "blog", "");
        assert!(score <= 100);
        assert_eq!(bd.platform, "google");
    }

    #[test] fn platform_derivation() {
        assert_eq!(platform_for_category("youtube"), "youtube");
        assert_eq!(platform_for_category("book"), "amazon");
        assert_eq!(platform_for_category("article"), "google");
        assert_eq!(normalize_platform("", "book"), "amazon");
        assert_eq!(normalize_platform("generic", "book"), "generic");
    }
}
