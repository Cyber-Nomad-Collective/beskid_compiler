//! Compile-fail corpus for the reachability-scoped semantic legality gate (design
//! `docs/superpowers/specs/2026-09-23-production-semantic-diagnostics-design.md`, section 4
//! slice 9).
//!
//! `corelib/beskid_corelib/tests/corelib_tests/fixtures/compile-fail/legality/` holds one
//! `beskid test` target per legality code. Each target's test calls a helper in a dependency
//! unit that carries exactly one legality violation, the shape World A never judges. Its entry
//! declares the expected code on a `// EXPECT: <code>` line. `beskid test` rejects every target
//! (a compile-fail target that compiled cleanly would itself be a regression,
//! `beskid_cli/src/commands/test.rs`); this harness drives the same executable gate those
//! targets hit (`prepare_compilation_diagnostics` with semantic diagnostics and the full
//! dependency closure) and asserts each target reports its expected code as an error, in the
//! dependency unit's own source, and no internal compiler error.

use std::path::PathBuf;

use beskid_analysis::Severity;
use beskid_analysis::services::{DependencyTypingPolicy, FrontEndOptions, PrepareOptions};

use crate::projects::fixture_harness::{resolve_fixture, with_project_test_env};

fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/beskid_corelib/tests/corelib_tests/fixtures/compile-fail/legality")
}

/// `(target, entry file, expected code)` for every compile-fail entry of the corpus.
fn corpus_targets() -> Vec<(String, String, String)> {
    let root = corpus_root();
    let mut targets = std::fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("read compile-fail corpus {}: {error}", root.display()))
        .map(|entry| entry.expect("corpus entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "bd"))
        .map(|path| {
            let source = std::fs::read_to_string(&path).expect("read corpus entry");
            let code = source
                .lines()
                .find_map(|line| line.strip_prefix("// EXPECT: "))
                .unwrap_or_else(|| panic!("{} declares no `// EXPECT: <code>` line", path.display()))
                .trim()
                .to_owned();
            let file = path.file_name().expect("file name").to_string_lossy().into_owned();
            let target = path.file_stem().expect("file stem").to_string_lossy().into_owned();
            (target, file, code)
        })
        .collect::<Vec<_>>();
    targets.sort();
    targets
}

/// Copy only checked-in project inputs, excluding lockfiles and generated `obj/` content.
fn copy_fixture_tree(source: &std::path::Path, destination: &std::path::Path) {
    std::fs::create_dir_all(destination).expect("create temporary fixture directory");
    for entry in std::fs::read_dir(source).expect("read fixture directory") {
        let source_path = entry.expect("fixture entry").path();
        let destination_path = destination.join(source_path.file_name().expect("fixture entry name"));
        let file_type = std::fs::symlink_metadata(&source_path).expect("fixture metadata").file_type();
        if file_type.is_dir() {
            if source_path.file_name().is_some_and(|name| name == "Helpers") {
                copy_fixture_tree(&source_path, &destination_path);
            }
        } else if file_type.is_file()
            && source_path.extension().is_some_and(|extension| extension == "bd" || extension == "bproj")
        {
            std::fs::copy(&source_path, &destination_path).expect("copy fixture file");
        }
    }
}

#[test]
fn every_legality_compile_fail_target_is_rejected_with_its_code_in_the_dependency_unit() {
    let targets = corpus_targets();
    let codes = targets.iter().map(|(_, _, code)| code.as_str()).collect::<std::collections::BTreeSet<_>>();
    for expected in [
        "E1101", "E1105", "E1108", "E1201", "E1203", "E1204", "E1211", "E1214", "E1229", "E1230", "E1231", "E1301",
        "E1302", "E1304", "E1307",
    ] {
        assert!(codes.contains(expected), "the compile-fail corpus has no target for {expected}");
    }
    let source_root = corpus_root();
    let compiler_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("compiler workspace root");
    let temporary_parent = compiler_root.join("target");
    match std::fs::symlink_metadata(&temporary_parent) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(&temporary_parent).expect("create Cargo target directory");
        }
        Err(error) => panic!("inspect Cargo target directory: {error}"),
    }
    let target_metadata = std::fs::symlink_metadata(&temporary_parent).expect("Cargo target directory");
    assert!(
        target_metadata.is_dir() && !target_metadata.file_type().is_symlink(),
        "Cargo target directory must be a real directory outside source inventory: {}",
        temporary_parent.display()
    );
    let temporary_root = tempfile::Builder::new()
        .prefix("legality-compile-fail-")
        .tempdir_in(&temporary_parent)
        .expect("temporary legality fixture directory");
    let root = temporary_root.path();
    copy_fixture_tree(&source_root, root);
    let copied_manifest = root.join("legality.bproj");
    let manifest_text = std::fs::read_to_string(&copied_manifest).expect("read copied fixture manifest");
    let original_corelib_path = "path = \"../../../../..\"";
    assert_eq!(manifest_text.matches(original_corelib_path).count(), 1, "fixture Corelib path changed unexpectedly");
    std::fs::write(
        &copied_manifest,
        manifest_text.replacen(original_corelib_path, "path = \"../../corelib/beskid_corelib\"", 1),
    )
    .expect("rewrite copied fixture Corelib path");
    let mut failures = Vec::new();
    with_project_test_env(root, || {
        for (target, entry, code) in &targets {
            let resolved = resolve_fixture(root, entry, target);
            let result = beskid_queries::prepare_compilation_diagnostics(
                &resolved,
                PrepareOptions { mod_invoker: None,
                    front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
                    dependency_typing: DependencyTypingPolicy::FullClosure,
                    native_mod_adapter_sources: false,
                },
                None,
            );
            let diagnostics = match result {
                Ok((_, diagnostics, _)) => diagnostics,
                Err(error) => {
                    failures.push(format!("{target}: prepare failed instead of reporting {code}: {error:#}"));
                    continue;
                }
            };
            let errors = diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity == Severity::Error)
                .map(|diagnostic| {
                    (diagnostic.code.clone().unwrap_or_default(), diagnostic.src.name().to_owned(), diagnostic.message.clone())
                })
                .collect::<Vec<_>>();
            let helper = format!("Helpers/{target}.bd");
            let reported = errors
                .iter()
                .any(|(found, source, _)| found == code && std::path::Path::new(source).ends_with(&helper));
            let internal = errors.iter().any(|(found, _, _)| found.starts_with("E21"));
            if !reported || internal {
                failures.push(format!("{target}: expected {code} in {helper}, got {errors:?}"));
            }
        }
    });
    assert!(
        !source_root.join("Project.lock").exists(),
        "legality compile-fail test must not write a lockfile into the checked-in fixture"
    );
    assert!(failures.is_empty(), "compile-fail corpus regressions:\n{}", failures.join("\n"));
}
