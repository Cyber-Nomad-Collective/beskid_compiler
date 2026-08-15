use std::path::PathBuf;

use super::{MatrixReport, RepositorySnapshot, RevisionSnapshot, TargetReport, TargetResult};
use crate::commands::test::TestSummary;

fn repository(name: &str, clean: bool) -> RepositorySnapshot {
    RepositorySnapshot { head: name.to_string(), content_revision: name.to_string(), clean }
}

fn revisions() -> RevisionSnapshot {
    RevisionSnapshot {
        root: Some(repository("root", true)),
        compiler: Some(repository("compiler", true)),
        corelib: Some(repository("corelib", true)),
    }
}

fn passing_target(name: &str) -> TargetReport {
    TargetReport {
        target: name.to_string(),
        started_unix_ms: 1,
        ended_unix_ms: 2,
        duration_ms: 1,
        active_phase: "complete".to_string(),
        result: TargetResult::Passed,
        tests: TestSummary { passed: 1, ..TestSummary::default() },
        phases: Vec::new(),
        error: None,
    }
}

fn complete_report(revisions: RevisionSnapshot) -> MatrixReport {
    let expected_targets = vec!["Alpha".to_string(), "Beta".to_string()];
    MatrixReport {
        manifest: PathBuf::from("corelib_tests.bproj"),
        revisions,
        denominator: expected_targets.len(),
        expected_targets: expected_targets.clone(),
        selected: expected_targets.len(),
        filtered: false,
        retried: false,
        ignored: 0,
        skipped: 0,
        timed_out: false,
        cancelled: false,
        release_eligible: false,
        targets: expected_targets.iter().map(|name| passing_target(name)).collect(),
    }
}

#[test]
fn complete_clean_unfiltered_manifest_inventory_is_release_eligible() {
    let revisions = revisions();
    let mut report = complete_report(revisions.clone());
    report.finish_eligibility(&revisions);
    assert!(report.release_eligible);
}

#[test]
fn dirty_changed_or_wrong_target_inventory_is_not_release_eligible() {
    let revisions = revisions();
    let mut dirty = complete_report(revisions.clone());
    let mut current = revisions.clone();
    current.compiler.as_mut().unwrap().clean = false;
    dirty.finish_eligibility(&current);
    assert!(!dirty.release_eligible);

    let mut changed = complete_report(revisions.clone());
    let mut current = revisions.clone();
    current.corelib.as_mut().unwrap().content_revision.push_str("changed");
    changed.finish_eligibility(&current);
    assert!(!changed.release_eligible);

    let mut wrong_inventory = complete_report(revisions.clone());
    wrong_inventory.targets.swap(0, 1);
    wrong_inventory.finish_eligibility(&revisions);
    assert!(!wrong_inventory.release_eligible);
}

#[test]
fn timeout_filter_and_skip_cannot_be_masked_by_passing_targets() {
    let revisions = revisions();
    let mut report = complete_report(revisions.clone());
    report.timed_out = true;
    report.filtered = true;
    report.skipped = 1;
    report.finish_eligibility(&revisions);
    assert!(!report.release_eligible);
}

#[test]
fn matrix_uses_one_worker_and_never_spawns_per_target_children() {
    let source = include_str!("../matrix_test.rs");
    assert!(source.contains("BESKID_PREPARED_MATRIX_WORKER"));
    assert_eq!(source.matches("Command::new(executable)").count(), 1);
    assert!(!source.contains("BESKID_MATRIX_CHILD"));
    assert!(!source.contains("run_isolated_target"));
}

#[test]
fn execution_loop_consumes_prepared_targets_without_repreparing() {
    let matrix = include_str!("../matrix_test.rs");
    let test = include_str!("../test.rs");
    assert!(matrix.contains("workspace.prepare_targets(&selected_targets,"));
    assert!(matrix.contains("for target in prepared_targets"));
    assert!(!test.contains("executable_gate_prepared"));
    assert!(!test.contains("prepare_target("));
}

#[test]
fn supervisor_kills_reaps_and_cancels_remaining_targets() {
    let source = include_str!("../matrix_test.rs");
    assert!(source.contains("child.kill()"));
    assert!(source.contains("child.wait()"));
    assert!(source.contains("TargetResult::Cancelled"));
    assert!(source.contains("active_phase"));
}
