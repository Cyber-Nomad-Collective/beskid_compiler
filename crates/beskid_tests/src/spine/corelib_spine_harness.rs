//! Shared spine harness: matrix drivers, env filtering, and hang safeguards.

use std::collections::HashSet;
use std::env;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::projects::fixture_harness::{
    corelib_tests_project_root, typecheck_corelib_tests_entry, with_project_test_env,
};

use super::corelib_spine_catalog::{CORELIB_TYPECHECK_ENTRIES, CORELIB_TYPECHECK_SMOKE_ENTRIES};

/// Per-entry semantic gate budget (gate mode; full executable prepare can take 20+ minutes).
pub const CORELIB_SPINE_ENTRY_TIMEOUT: Duration = Duration::from_secs(600);

/// Whole-matrix budget when running every catalog entry in one process.
pub const CORELIB_SPINE_MATRIX_TIMEOUT: Duration = Duration::from_secs(3600);

/// Skip all corelib spine matrix gates (local fast path).
pub fn corelib_spine_skipped() -> bool {
    env::var("BESKID_SKIP_CORELIB_SPINE").ok().is_some_and(|value| !value.is_empty() && value != "0")
}

/// Resolve the entry list for the current run (smoke / filter / full catalog).
pub fn selected_corelib_typecheck_entries() -> Vec<&'static str> {
    if corelib_spine_skipped() {
        return Vec::new();
    }
    let base: &[&str] =
        if env::var("BESKID_CORELIB_SPINE_SMOKE").ok().is_some_and(|value| !value.is_empty() && value != "0") {
            CORELIB_TYPECHECK_SMOKE_ENTRIES
        } else {
            CORELIB_TYPECHECK_ENTRIES
        };
    filter_entries_by_env(base)
}

fn filter_entries_by_env(entries: &[&'static str]) -> Vec<&'static str> {
    let raw = env::var("BESKID_CORELIB_SPINE_ENTRIES").unwrap_or_default();
    let raw = raw.trim();
    if raw.is_empty() {
        return entries.to_vec();
    }
    let wanted: HashSet<&str> = raw.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if wanted.is_empty() {
        return entries.to_vec();
    }
    let mut selected = Vec::new();
    for entry in entries {
        if wanted.contains(entry) {
            selected.push(*entry);
        }
    }
    if selected.is_empty() {
        panic!("BESKID_CORELIB_SPINE_ENTRIES matched no catalog entries; wanted: {raw}");
    }
    for wanted_entry in &wanted {
        if !entries.contains(wanted_entry) {
            panic!("BESKID_CORELIB_SPINE_ENTRIES unknown entry: {wanted_entry}");
        }
    }
    selected
}

/// Run `f` on a worker thread and fail with a clear message if it exceeds `timeout`.
pub fn run_with_timeout(label: &str, timeout: Duration, f: impl FnOnce() + Send + 'static) {
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name(format!("corelib-spine-{label}"))
        .spawn(move || {
            f();
            let _ = done_tx.send(());
        })
        .unwrap_or_else(|err| panic!("spawn corelib spine worker for {label}: {err}"));
    match done_rx.recv_timeout(timeout) {
        Ok(()) => {}
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!(
                "{label} exceeded {timeout:?}. \
                 If this is expected on a cold cache, rerun with --test-threads=1 --nocapture. \
                 Skip locally: BESKID_SKIP_CORELIB_SPINE=1. \
                 Bisect one entry: BESKID_CORELIB_SPINE_ENTRIES=<path> cargo test --ignored --exact <test>"
            );
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("{label} worker exited without signaling completion");
        }
    }
}

/// Typecheck every selected catalog entry in one Salsa session (SOTA matrix gate).
pub fn run_corelib_typecheck_matrix() {
    let entries = selected_corelib_typecheck_entries();
    if entries.is_empty() {
        eprintln!("corelib spine matrix: skipped (BESKID_SKIP_CORELIB_SPINE)");
        return;
    }
    let root = corelib_tests_project_root();
    run_with_timeout("corelib_tests_front_end_typechecks_matrix", CORELIB_SPINE_MATRIX_TIMEOUT, move || {
        with_project_test_env(&root, || {
            let matrix_started = Instant::now();
            eprintln!("corelib spine matrix: {} entr{}", entries.len(), if entries.len() == 1 { "y" } else { "ies" });
            for entry in entries {
                if !root.join("src").join(entry).is_file() {
                    eprintln!("corelib spine matrix: skip missing {entry}");
                    continue;
                }
                run_with_timeout(&format!("corelib typecheck {entry}"), CORELIB_SPINE_ENTRY_TIMEOUT, || {
                    typecheck_corelib_tests_entry(entry)
                });
            }
            eprintln!("corelib spine matrix: finished in {:.1}s", matrix_started.elapsed().as_secs_f64());
        });
    });
}
