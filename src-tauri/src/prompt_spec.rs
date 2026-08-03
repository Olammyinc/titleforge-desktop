//! Per-category output conventions, style descriptions, and fine-tune handling.
//!
//! Why this exists: before 2026-08-03 the offline prompt substituted a bare
//! category word (`"Generate ONE creative, clickable {category} title"`) and a
//! bare style token. Category was a LABEL, not a CONSTRAINT — measured result
//! was category collapse: cloud output varied by under one word of mean length
//! across five categories, and 100% of "product" titles were blog headlines
//! rather than product names. See CONTEXT.md §5 2026-08-03 (Task 2a entry).
//!
//! DESIGN RULE — read before adding anything here. Qwen2.5-1.5B cannot hold
//! multi-constraint prompts: the six-rule block was measured twice at 75.2 and
//! 77.6 mean against an 81.0 baseline (CONTEXT.md §5 2026-07-31, brief §3
//! "Tested and closed"). So every string below REPLACES a vague instruction
//! that was already in the prompt. It does not stack a new rule on top of one.
//! Net instruction count per generation must stay flat. If you add a second
//! rule line here, you are re-running an experiment that already failed twice.

/// What a given category's output should actually BE.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CategorySpec {
    /// Human label used in the prompt ("blog post", "song").
    pub label: &'static str,
    /// The single defining form statement. Replaces "creative, clickable".
    pub form: &'static str,
    /// Word-count guidance. Replaces the blanket "3-15 words".
    pub words: (usize, usize),
    /// One concrete exemplar. A small model imitates far better than it follows.
    pub example: &'static str,
    /// True when the output is a NAME, not a title — a different task entirely.
    pub is_name: bool,
    /// Forms where a colon is essentially never right. Measured 2026-08-03 on
    /// Qwen: after the conventions landed, `X: Y` became the dominant template
    /// (75% of book and youtube output, 50% of song). For a song, poem, album
    /// or film title a colon is a tell that the model wrote a headline.
    pub forbid_colon: bool,
    /// Forms where digits are essentially never right. Songs and poems came
    /// back with "2 Coffees and Tea" — a listicle habit bleeding across.
    pub forbid_digits: bool,
}

/// Conventions for all 16 categories. `product`, `childname`, `character` and
/// `street` are NAME categories: they must produce a name a person would
/// actually use, not a headline about the topic.
pub fn category_spec(category: &str) -> CategorySpec {
    match category.to_lowercase().as_str() {
        "book" => CategorySpec {
            label: "book title",
            form: "evocative and thematic — it names the book's world, it does not summarise it",
            words: (2, 7),
            example: "The Name of the Wind",
            is_name: false,
            // Both forms are legitimate for books: the evocative one-liner
            // ("The Name of the Wind") and the subtitle form ("Sapiens: A Brief
            // History of Humankind"). Banning the colon outright was wrong and
            // is reverted (user, 2026-08-03). The real problem was PROPORTION —
            // Qwen produced colons in 75% of book titles. That is capped per
            // batch in engine.rs instead, which is the right place for it.
            forbid_colon: false,
            forbid_digits: false,
        },
        "ebook" => CategorySpec {
            label: "eBook title",
            form: "practical and benefit-led — the reader should know what they will be able to do",
            words: (4, 10),
            example: "The 30-Minute Pantry: Cooking Without a Recipe",
            is_name: false,
            forbid_colon: false,
            forbid_digits: false,
        },
        "article" => CategorySpec {
            label: "article headline",
            form: "a clear thesis with a concrete payoff",
            words: (6, 14),
            example: "The Myth of Meritocracy: Why Talent Alone Will Never Beat Inheritance",
            is_name: false,
            forbid_colon: false,
            forbid_digits: false,
        },
        "blog" => CategorySpec {
            label: "blog post title",
            form: "reader-facing and useful — it promises something the reader gets",
            words: (5, 12),
            example: "Why Your Sourdough Keeps Coming Out Flat",
            is_name: false,
            forbid_colon: false,
            forbid_digits: false,
        },
        "movie" => CategorySpec {
            label: "film title",
            form: "short and iconic — no explanation, no how-to, no list",
            words: (1, 5),
            example: "No Country for Old Men",
            is_name: false,
            forbid_colon: true,
            forbid_digits: true,
        },
        "song" => CategorySpec {
            label: "song title",
            form: "a sensory or emotional fragment, like a lyric — never a how-to and never a list",
            words: (2, 7),
            example: "Cigarette Smoke and Honey",
            is_name: false,
            forbid_colon: true,
            forbid_digits: true,
        },
        "album" => CategorySpec {
            label: "album title",
            form: "short and atmospheric — it names a mood the whole record lives in",
            words: (1, 5),
            example: "In the Aeroplane Over the Sea",
            is_name: false,
            forbid_colon: true,
            forbid_digits: true,
        },
        "poem" => CategorySpec {
            label: "poem title",
            form: "a single compressed image — no marketing language of any kind",
            words: (1, 6),
            example: "The Fish",
            is_name: false,
            forbid_colon: true,
            forbid_digits: true,
        },
        "youtube" => CategorySpec {
            label: "YouTube video title",
            form: "spoken and first-person, built on a challenge or a result",
            words: (5, 12),
            example: "I Spent 48 Hours in a Silent Retreat",
            is_name: false,
            forbid_colon: false,
            forbid_digits: false,
        },
        "podcast" => CategorySpec {
            label: "podcast episode title",
            form: "conversational — it speaks directly to the listener",
            words: (4, 12),
            example: "You're Not Tired, You're Under-Slept",
            is_name: false,
            forbid_colon: false,
            forbid_digits: false,
        },
        "newsletter" => CategorySpec {
            label: "newsletter subject line",
            form: "insider and direct — written as if to one person who already subscribed",
            words: (4, 10),
            example: "What's the one metric most teams overlook?",
            is_name: false,
            forbid_colon: false,
            forbid_digits: false,
        },
        "speech" => CategorySpec {
            label: "speech title",
            form: "aspirational and spoken aloud — it must sound right said from a stage",
            words: (4, 10),
            example: "How to Raise a Generation of Critical Thinkers",
            is_name: false,
            forbid_colon: false,
            forbid_digits: false,
        },

        // ── NAME categories — the output is a NAME, not a title about a topic ──
        "product" => CategorySpec {
            label: "product name",
            form: "a brandable name — one or two words, no digits, no sentence, no explanation",
            words: (1, 3),
            example: "Vivid",
            is_name: true,
            forbid_colon: true,
            forbid_digits: true,
        },
        "childname" => CategorySpec {
            label: "child's given name",
            form: "a real first name a parent could put on a birth certificate",
            words: (1, 2),
            example: "Marisol",
            is_name: true,
            forbid_colon: true,
            forbid_digits: true,
        },
        "character" => CategorySpec {
            label: "character name",
            form: "a person's name for a play, film or novel",
            words: (1, 3),
            example: "Atticus Finch",
            is_name: true,
            forbid_colon: true,
            forbid_digits: true,
        },
        "street" => CategorySpec {
            label: "street or place name",
            form: "a place name you could put on a map or a signpost",
            words: (1, 4),
            example: "Kestrel Row",
            is_name: true,
            forbid_colon: true,
            forbid_digits: true,
        },

        // Unknown category: stay generic rather than assert a wrong convention.
        _ => CategorySpec {
            label: "title",
            form: "clear and specific",
            words: (3, 12),
            example: "The Quiet Thief",
            is_name: false,
            forbid_colon: false,
            forbid_digits: false,
        },
    }
}

/// Style descriptions. The offline prompt previously passed the raw style token
/// ("whisper") straight into the text, which is meaningless to the model.
/// Mirrors STYLE_DESCRIPTIONS in `netlify/functions/generate.js`.
pub fn style_description(style: &str) -> &'static str {
    match style.to_lowercase().as_str() {
        "shout" => "bold, attention-grabbing, high-impact",
        "whisper" => "subtle, understated, quietly intriguing",
        "blessing" => "wholesome, uplifting, positive — no harsh or negative words",
        "provocative" => "pointed and debate-sparking — not loud, but bold",
        "minimalist" => "ultra-clean, stripped back, Apple-esque",
        "storytelling" => "narrative and anecdotal, a story hook",
        "question" => "framed as a question the reader wants to answer",
        "playful" => "witty, pun-aware, sharp and light",
        _ => "clear and professional",
    }
}

/// Fine-tune options. Mirrors the 7 web fields; parsed from the same camelCase
/// JSON the frontend already sends to `generate_with_ai`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FineTune {
    pub audience: Option<String>,
    pub emotion: Option<String>,
    pub length: Option<String>,
    pub angle: Option<String>,
    pub must_include: Option<String>,
    pub avoid: Option<String>,
    pub beat_title: Option<String>,
}

impl FineTune {
    pub fn from_json(v: Option<&serde_json::Value>) -> Self {
        let Some(v) = v else { return Self::default() };
        let get = |k: &str| {
            v.get(k)
                .and_then(|x| x.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        Self {
            audience: get("audience"),
            emotion: get("emotion"),
            length: get("length"),
            angle: get("angle"),
            must_include: get("mustInclude"),
            avoid: get("avoid"),
            beat_title: get("beatTitle"),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The SOFT fields, collapsed into at most ONE extra prompt line.
    ///
    /// `must_include` and `avoid` are deliberately excluded: they are hard
    /// constraints, and a 1.5B asked to satisfy a word blocklist inside its
    /// 3-attempt budget burns attempts and returns empty. They are enforced
    /// deterministically in `satisfies_hard_constraints()` instead, which is
    /// both cheaper and actually reliable.
    pub fn soft_prompt_line(&self) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        if let Some(a) = &self.audience {
            parts.push(format!("for {}", a));
        }
        if let Some(e) = &self.emotion {
            parts.push(format!("evoking {}", e));
        }
        if let Some(a) = &self.angle {
            parts.push(format!("angled on {}", a));
        }
        if let Some(l) = &self.length {
            parts.push(format!("{} in length", l));
        }
        if parts.is_empty() {
            return None;
        }
        Some(format!(" Make it {}.", parts.join(", ")))
    }

    /// Hard constraints, checked AFTER generation. Returns false if the title
    /// must be rejected. Case-insensitive; `avoid` and `must_include` accept a
    /// comma-separated list.
    pub fn satisfies_hard_constraints(&self, title: &str) -> bool {
        let lower = title.to_lowercase();
        if let Some(avoid) = &self.avoid {
            for word in split_terms(avoid) {
                if lower.contains(&word) {
                    return false;
                }
            }
        }
        if let Some(must) = &self.must_include {
            for word in split_terms(must) {
                if !lower.contains(&word) {
                    return false;
                }
            }
        }
        true
    }
}

/// Shape check for NAME categories only.
///
/// Name categories were structurally impossible before this existed: the
/// generic QC in `generate_one_clean` rejected any output under two words, so
/// a correct product name ("Vivid") was thrown away, and the drift guard
/// required a >=4-char keyword word inside the title, which a brandable name
/// deliberately does not have. Title categories are NOT shape-checked here —
/// enforcing a max word count on them would cost fire rate for no measured
/// gain, and the word band stays prompt-only guidance.
pub fn passes_name_shape(title: &str, spec: &CategorySpec) -> bool {
    // Colon / digit bans apply to mood-based forms too (song, poem, album,
    // movie, book) — measured 2026-08-03, `X: Y` became the dominant template
    // across every category once the conventions landed, and songs came back
    // with digits. The prompt suggests; this enforces.
    if spec.forbid_colon && title.contains(':') {
        return false;
    }
    if spec.forbid_digits && title.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    if !spec.is_name {
        return true;
    }
    let words = title.split_whitespace().count();
    if words == 0 || words > spec.words.1 + 1 {
        return false;
    }
    // A name is not a sentence. These are the tells that the model produced a
    // headline anyway — the exact failure the user reported for `product`.
    if title.ends_with('?') || title.ends_with('.') {
        return false;
    }
    true
}

/// True if the title has visibly copied the spec's exemplar.
///
/// Measured 2026-08-03: giving Qwen a concrete example ("I Spent 48 Hours in a
/// Silent Retreat") made it return "48 Hours in the Silent Remote Office" —
/// it imitates the example's CONTENT, not just its shape. The exemplar earns
/// its place (a 1.5B imitates better than it follows), so keep it and reject
/// the echoes instead. Trips on 2+ shared distinctive words.
pub fn echoes_example(title: &str, spec: &CategorySpec) -> bool {
    const STOP: &[&str] = &[
        "the", "a", "an", "of", "in", "on", "at", "to", "for", "and", "or",
        "my", "i", "you", "your", "is", "it", "with", "how", "what", "why",
    ];
    let words = |s: &str| -> Vec<String> {
        s.split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
            .filter(|w| w.len() > 2 && !STOP.contains(&w.as_str()))
            .collect()
    };
    let ex = words(spec.example);
    if ex.is_empty() {
        return false;
    }
    let t = words(title);
    let shared = t.iter().filter(|w| ex.contains(w)).count();
    shared >= 2
}

fn split_terms(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_is_a_name_category_not_a_headline() {
        let s = category_spec("product");
        assert!(s.is_name, "product must produce a NAME, not a blog headline");
        assert_eq!(s.words, (1, 3));
        assert!(s.form.contains("no digits"));
    }

    #[test]
    fn all_four_name_categories_flagged() {
        for c in ["product", "childname", "character", "street"] {
            assert!(category_spec(c).is_name, "{c} should be a name category");
        }
        for c in ["blog", "book", "song", "youtube", "podcast", "speech"] {
            assert!(!category_spec(c).is_name, "{c} should NOT be a name category");
        }
    }

    #[test]
    fn categories_actually_differ() {
        // The bug this module fixes: every category produced the same shape.
        // Guard it — song and blog must not share a form or a length band.
        let song = category_spec("song");
        let blog = category_spec("blog");
        assert_ne!(song.form, blog.form);
        assert_ne!(song.words, blog.words);
        assert_ne!(category_spec("product").words, category_spec("article").words);
    }

    #[test]
    fn every_known_category_has_a_real_spec() {
        let all = [
            "book", "article", "blog", "movie", "song", "youtube", "podcast",
            "newsletter", "ebook", "speech", "album", "poem", "street",
            "character", "product", "childname",
        ];
        for c in all {
            let s = category_spec(c);
            assert_ne!(s.label, "title", "{c} fell through to the generic spec");
            assert!(s.words.0 >= 1 && s.words.1 >= s.words.0, "{c} bad word range");
            assert!(!s.example.is_empty(), "{c} needs an exemplar");
        }
    }

    #[test]
    fn unknown_category_falls_back_without_panicking() {
        let s = category_spec("does-not-exist");
        assert_eq!(s.label, "title");
        assert!(!s.is_name);
    }

    #[test]
    fn style_descriptions_are_resolved_not_echoed() {
        assert!(style_description("whisper").contains("understated"));
        assert!(style_description("minimalist").contains("stripped back"));
        // Unknown style must not echo the raw token into the prompt.
        assert_eq!(style_description("nonsense"), "clear and professional");
    }

    #[test]
    fn finetune_parses_camel_case_and_ignores_blanks() {
        let v = serde_json::json!({
            "audience": "beginners",
            "emotion": "  ",
            "mustInclude": "coffee",
            "beatTitle": "Some Title"
        });
        let ft = FineTune::from_json(Some(&v));
        assert_eq!(ft.audience.as_deref(), Some("beginners"));
        assert_eq!(ft.emotion, None, "whitespace-only must be treated as unset");
        assert_eq!(ft.must_include.as_deref(), Some("coffee"));
        assert_eq!(ft.beat_title.as_deref(), Some("Some Title"));
        assert!(!ft.is_empty());
        assert!(FineTune::from_json(None).is_empty());
    }

    #[test]
    fn soft_line_collapses_to_one_line_and_omits_hard_fields() {
        let ft = FineTune {
            audience: Some("developers".into()),
            emotion: Some("curiosity".into()),
            avoid: Some("ultimate".into()),
            must_include: Some("rust".into()),
            ..Default::default()
        };
        let line = ft.soft_prompt_line().unwrap();
        assert_eq!(line.matches('\n').count(), 0, "must stay a single line");
        assert!(line.contains("developers") && line.contains("curiosity"));
        assert!(!line.contains("ultimate"), "avoid is enforced in QC, not the prompt");
        assert!(!line.contains("rust"), "mustInclude is enforced in QC, not the prompt");
        assert_eq!(FineTune::default().soft_prompt_line(), None);
    }

    #[test]
    fn name_shape_accepts_real_names_and_rejects_headlines() {
        let product = category_spec("product");
        // The regression this guards: a correct one-word product name used to be
        // rejected by the generic "< 2 words" QC.
        assert!(passes_name_shape("Vivid", &product));
        assert!(passes_name_shape("Ember Roast", &product));
        // The user-reported failure: a blog headline returned for `product`.
        assert!(!passes_name_shape("The 3-Second Rule That Makes Any Shirt Fit", &product));
        assert!(!passes_name_shape("Brew: A Journey Through Coffee", &product));
        assert!(!passes_name_shape("Why Your Coffee Tastes Flat?", &product));
        assert!(!passes_name_shape("Coffee 2026", &product), "digits not allowed in a product name");
    }

    #[test]
    fn name_shape_is_a_noop_for_permissive_title_categories() {
        let blog = category_spec("blog");
        // Long, colon-bearing, digit-bearing titles are all legitimate here.
        assert!(passes_name_shape("The 5 Things Nobody Tells You: A Guide", &blog));
        assert!(passes_name_shape("Why Your Sourdough Keeps Coming Out Flat?", &blog));
    }

    #[test]
    fn mood_categories_reject_headline_punctuation() {
        // Regression guard for the 2026-08-03 measured result: `X: Y` became
        // the dominant template and songs came back carrying digits.
        let song = category_spec("song");
        assert!(!passes_name_shape("Drinking Life: A Journey Through Coffee", &song));
        assert!(!passes_name_shape("2 Coffees and Tea", &song));
        assert!(passes_name_shape("Cigarette Smoke and Honey", &song));

        let poem = category_spec("poem");
        assert!(!passes_name_shape("Rise of Sourdough: A Journey in Yeast", &poem));

        // Books keep BOTH forms — the colon subtitle is legitimate ("Sapiens:
        // A Brief History of Humankind"). Proportion is capped in engine.rs,
        // not here. (User decision 2026-08-03.)
        let book = category_spec("book");
        assert!(passes_name_shape("Remote Revolution: How Work Will Never Be the Same", &book));
        assert!(passes_name_shape("The Name of the Wind", &book));
        assert!(passes_name_shape("Catch-22", &book), "digits stay legal in book titles");

        // Categories that legitimately use colons must be unaffected.
        for c in ["ebook", "article", "blog", "podcast"] {
            let s = category_spec(c);
            assert!(passes_name_shape("Something: A Subtitle Here", &s), "{c} should allow colons");
        }
    }

    #[test]
    fn shared_opening_dedup() {
        use crate::engine::shares_opening;
        // The measured failure: a 4-title book batch that was four variations
        // on one stem (2026-08-03 run 5). The shared stem is TWO words — the
        // third word already differs ("How" vs "My"), which is why n=3 missed
        // the exact case this was written for.
        assert!(shares_opening(
            "Remote Revolution: How Work Transformed",
            "Remote Revolution: My Journey Unplugged", 2));
        assert!(shares_opening("The Art of Coffee", "the art of tea", 2),
            "case and punctuation insensitive");
        // Genuinely different titles must survive.
        assert!(!shares_opening(
            "Remote Revolution: How Work Transformed",
            "Why Nobody Misses The Commute", 2));
        // Too short to judge — must not collapse distinct short titles.
        assert!(!shares_opening("Vivid", "Vivid Brew", 2));
        // Pure function-word openings are shared by many distinct titles;
        // rejecting those would cost fire rate for no diversity gain.
        assert!(!shares_opening(
            "How To Brew Better Coffee",
            "How To Bake Sourdough", 2));
        assert!(!shares_opening("The Best Coffee", "The Best Bread", 2));
    }

    #[test]
    fn exemplar_echo_detected() {
        let yt = category_spec("youtube"); // "I Spent 48 Hours in a Silent Retreat"
        // The actual measured leak.
        assert!(echoes_example("48 Hours in the Silent Remote Office", &yt));
        assert!(echoes_example("I Spent 48 Hours Working Silent", &yt));
        // Ordinary output that merely shares stop words must NOT trip.
        assert!(!echoes_example("Why Nobody Talks About Standing Desks", &yt));
        assert!(!echoes_example("The Commute I Did Not Miss", &yt));
        // One shared distinctive word is a coincidence, not a copy.
        assert!(!echoes_example("My Retreat From Open Offices", &yt));
    }

    #[test]
    fn hard_constraints_enforced_case_insensitively() {
        let ft = FineTune {
            avoid: Some("Ultimate, Secret".into()),
            must_include: Some("coffee".into()),
            ..Default::default()
        };
        assert!(ft.satisfies_hard_constraints("Coffee at First Light"));
        assert!(!ft.satisfies_hard_constraints("The ULTIMATE Coffee Guide"));
        assert!(!ft.satisfies_hard_constraints("A Secret About Coffee"));
        assert!(!ft.satisfies_hard_constraints("Tea at First Light"), "missing mustInclude");
        // No constraints set -> everything passes.
        assert!(FineTune::default().satisfies_hard_constraints("anything at all"));
    }
}
