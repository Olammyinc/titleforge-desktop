//! Shared evidence-persistence for title-generation measurement harnesses.
//!
//! Standing rule #1 (CONTEXT.md §7): "A measurement that only prints to stdout
//! has not been taken." This module exists to make persisting the DEFAULT, so a
//! harness no longer has to remember to write a file. Every new measurement
//! should route its output through `write_evidence_csv` instead of hand-rolling
//! `fs::write` with a hardcoded path.
//!
//! It also kills the fixed-path clobber: writes default to a RUN-SUFFIXED name
//! (`<name>-run<N>.csv`) so two runs of the same harness never overwrite each
//! other's evidence — the mechanism behind the original `batch-uniqueness.csv`
//! loss. A run tag is read from the env var named by the caller (e.g. `TF_RUN`,
//! `STUDIO_RUN`) and defaults to "1".
//!
//! Usage from an integration test (tests/*.rs):
//!   #[path = "evidence.rs"]
//!   mod evidence;
//!
//!   let path = evidence::write_evidence_csv("four-x-fifty", &["col_a", "col_b"], &rows);
//!   // writes <repo>/four-x-fifty-run{run}.csv (run from env TF_RUN), returns the path
//!
//! For long unattended runs, call `write_evidence_csv` (or the flush variant)
//! after each completed batch so an interrupted run still leaves per-batch
//! evidence, rather than a single write at the very end.

use std::path::{Path, PathBuf};

/// Run tag for the evidence filename. Reads `env_name`; defaults to "1".
/// A harness passing e.g. "TF_4X50_RUN" gets run1/run2 on separate invocations
/// and the CSVs never collide.
pub fn run_tag(env_name: &str) -> String {
    std::env::var(env_name).unwrap_or_else(|_| "1".to_string())
}

/// Absolute path for a run-suffixed evidence CSV in the repo root.
/// `<base_name>-run{run}.csv` (e.g. `four-x-fifty-run1.csv`) directly under
/// the desktop repo (CARGO_MANIFEST_DIR/../). The run suffix is what prevents
/// one harness from clobbering the previous run's artifact.
pub fn evidence_path(base_name: &str, run: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(format!("{}-run{}.csv", base_name, run))
}

/// Serialise `header` + `rows` and atomically write them to a run-suffixed
/// evidence CSV. Returns the path written.
///
/// `header` is the CSV header line (no trailing newline). `rows` is the full
/// body (each row already newline-terminated). Passing `header` and `rows`
/// separately keeps row-building in the harness and lets the write be a single
/// fs::write for atomicity.
pub fn write_evidence_csv(base_name: &str, env_name: &str, header: &str, rows: &str) -> PathBuf {
    let run = run_tag(env_name);
    let path = evidence_path(base_name, &run);
    let csv = if rows.is_empty() {
        format!("{}\n", header)
    } else if rows.ends_with('\n') {
        format!("{}\n{}", header, rows)
    } else {
        format!("{}\n{}\n", header, rows)
    };
    let _ = std::fs::write(&path, csv);
    path
}

/// Write a header-only evidence file up front (before any rows exist), so that
/// interruptible runs establish the artifact immediately. Rows follow via
/// `append_evidence_csv`. Mirrors the four_x_fifty_v2 "flush after each batch"
/// discipline.
pub fn init_evidence_csv(base_name: &str, env_name: &str, header: &str) -> PathBuf {
    let run = run_tag(env_name);
    let path = evidence_path(base_name, &run);
    let _ = std::fs::write(&path, format!("{}\n", header));
    path
}

/// Append `rows` to a run-suffixed evidence CSV (created by `init_evidence_csv`).
/// Appends without re-reading the existing content — safe for unbounded,
/// interruptible runs.
pub fn append_evidence_csv(base_name: &str, env_name: &str, rows: &str) -> PathBuf {
    use std::io::Write;
    let run = run_tag(env_name);
    let path = evidence_path(base_name, &run);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(rows.as_bytes());
    }
    path
}
