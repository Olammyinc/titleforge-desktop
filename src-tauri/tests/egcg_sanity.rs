/// Quick test: EGCG output quality — verify no {placeholder} leaks
#[test]
fn egcg_sanity() {
    use rusqlite::Connection;
    let conn = Connection::open_in_memory().expect("mem");
    conn.execute_batch("CREATE TABLE IF NOT EXISTS curated_titles (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, category TEXT NOT NULL, genre TEXT, tone TEXT, appeal_score INTEGER, notes TEXT); CREATE TABLE IF NOT EXISTS patterns (id INTEGER PRIMARY KEY AUTOINCREMENT, category TEXT NOT NULL, template TEXT NOT NULL, slots TEXT NOT NULL, genre TEXT, tone TEXT, quality_score REAL DEFAULT 0.5, usage_count INTEGER DEFAULT 0); CREATE TABLE IF NOT EXISTS word_pools (id INTEGER PRIMARY KEY AUTOINCREMENT, pool_name TEXT NOT NULL, word TEXT NOT NULL, category TEXT, weight REAL DEFAULT 1.0); CREATE TABLE IF NOT EXISTS user_history (id INTEGER PRIMARY KEY AUTOINCREMENT, keyword TEXT NOT NULL, categories TEXT NOT NULL, genre TEXT, style TEXT, titles TEXT NOT NULL, created_at TEXT DEFAULT (datetime('now'))); CREATE TABLE IF NOT EXISTS user_favorites (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, keyword TEXT, score INTEGER, category TEXT, created_at TEXT DEFAULT (datetime('now'))); CREATE TABLE IF NOT EXISTS user_settings (key TEXT PRIMARY KEY, value TEXT); CREATE TABLE IF NOT EXISTS user_projects (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, created_at TEXT DEFAULT (datetime('now'))); CREATE TABLE IF NOT EXISTS project_titles (id INTEGER PRIMARY KEY AUTOINCREMENT, project_id INTEGER NOT NULL, title TEXT NOT NULL, keyword TEXT, score INTEGER, notes TEXT, FOREIGN KEY (project_id) REFERENCES user_projects(id) ON DELETE CASCADE);").expect("schema");
    let seed = include_str!("../../seed-data.json");
    titleforge_lib::db::import_seed_from_str(&conn, seed).expect("seed");

    let gen = titleforge_lib::title_gen::Generator::build(&conn);
    let keywords = &[
        "shirt", "laptop", "productivity", "love", "startup", "crypto",
        "coffee", "travel", "fitness", "meditation", "writing", "marketing",
        "music", "sleep", "tennis", "gaming", "vegan", "yoga", "blockchain",
        "parenting",
    ];

    let mut total = 0usize;
    let mut had_bracket = 0usize;
    let mut had_keyword = 0usize;
    let mut sample: Vec<String> = Vec::new();

    for kw in keywords {
        let cats: Vec<String> = vec!["book".to_string(), "article".to_string(), "youtube".to_string()];
        let results = gen.generate(&*kw, &cats, "normal", "any", 10);
        for r in &results {
            total += 1;
            if r.title.contains('{') || r.title.contains('}') { had_bracket += 1; }
            let lower = r.title.to_lowercase();
            let kwl = kw.to_lowercase();
            let has_kw = lower.contains(&kwl) || kwl.split_whitespace().any(|w| lower.contains(w));
            if has_kw { had_keyword += 1; }
            if sample.len() < 30 { sample.push(format!("{:>4} | {}", r.score, r.title)); }
        }
    }

    println!("\nEGCG Sanity Check: {} titles from {} keywords (book+article+youtube)", total, keywords.len());
    println!("  Keyword present: {}/{} ({:.0}%)", had_keyword, total, (had_keyword as f64 / total as f64) * 100.0);
    println!("  {{placeholder}} leak: {}/{} ({})", had_bracket, total, if had_bracket == 0 { "PASS" } else { "FAIL" });
    println!("\nSample:\n{}", sample.join("\n"));

    assert_eq!(had_bracket, 0, "No {{placeholder}} leaks allowed in output");
    assert!(had_keyword > total / 4, "At least 25% keyword presence required");
}
