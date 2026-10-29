//! Compile-fail corpora for `Core.Parallel` rejections and for names that must not resolve.
//!
//! `corelib/beskid_corelib/tests/corelib_tests/fixtures/compile-fail/{parallel,resolution}/` each
//! hold one project with one target per entry file. Each entry declares the code it must produce on
//! a `// EXPECT: <code>` line. This gate drives the executable prepare gate (`beskid check` and
//! `beskid test` share it: semantic diagnostics with the full dependency closure) for every target
//! and asserts that the entry itself reports its expected code as an error, with no internal
//! compiler error. The corpus fails closed: every `.bd` entry must be a target of the manifest, and
//! every target must have an entry with an `EXPECT` line.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use beskid_analysis::Severity;
use beskid_analysis::services::{FrontEndOptions, PrepareOptions};

use crate::projects::fixture_harness::{enter_mod_host, mod_host_for, resolve_fixture, with_project_test_env};

fn corpus_root(corpus: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/beskid_corelib/tests/corelib_tests/fixtures/compile-fail")
        .join(corpus)
}

/// `(target, entry file, expected code)` for every entry of `corpus`, checked against the targets
/// its manifest declares.
fn corpus_targets(corpus: &str) -> Vec<(String, String, String)> {
    let root = corpus_root(corpus);
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
            assert!(
                code.len() == 5 && code.starts_with('E') && code[1..].bytes().all(|byte| byte.is_ascii_digit()),
                "{} must expect one diagnostic code, found `{code}`",
                path.display()
            );
            let file = path.file_name().expect("file name").to_string_lossy().into_owned();
            let target = path.file_stem().expect("file stem").to_string_lossy().into_owned();
            (target, file, code)
        })
        .collect::<Vec<_>>();
    targets.sort();
    let manifest = std::fs::read_to_string(root.join(format!("{corpus}.bproj"))).expect("read corpus manifest");
    let declared = manifest
        .lines()
        .filter_map(|line| line.trim().strip_prefix("target \"")?.split('"').next().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    let entries = targets.iter().map(|(target, _, _)| target.clone()).collect::<BTreeSet<_>>();
    assert_eq!(declared, entries, "{corpus}: manifest targets and entry files differ");
    assert!(!entries.is_empty(), "{corpus}: the corpus has no entries");
    targets
}

/// Copy only checked-in project inputs, excluding lockfiles and generated `obj/` content.
fn copy_fixture_tree(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).expect("create temporary fixture directory");
    for entry in std::fs::read_dir(source).expect("read fixture directory") {
        let source_path = entry.expect("fixture entry").path();
        let file_type = std::fs::symlink_metadata(&source_path).expect("fixture metadata").file_type();
        if file_type.is_file()
            && source_path.extension().is_some_and(|extension| extension == "bd" || extension == "bproj")
        {
            let destination_path = destination.join(source_path.file_name().expect("fixture entry name"));
            std::fs::copy(&source_path, &destination_path).expect("copy fixture file");
        }
    }
}

/// A temporary directory under the Cargo target directory, outside the source inventory.
fn temporary_root(prefix: &str) -> tempfile::TempDir {
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
    tempfile::Builder::new().prefix(prefix).tempdir_in(&temporary_parent).expect("temporary fixture directory")
}

/// Every target of `corpus` is rejected with its expected code, reported in its own entry file.
fn assert_corpus_rejected(corpus: &str, required_codes: &[&str]) {
    let targets = corpus_targets(corpus);
    let codes = targets.iter().map(|(_, _, code)| code.as_str()).collect::<BTreeSet<_>>();
    for expected in required_codes {
        assert!(codes.contains(expected), "the {corpus} compile-fail corpus has no target for {expected}");
    }
    let source_root = corpus_root(corpus);
    let temporary = temporary_root(&format!("{corpus}-compile-fail-"));
    let root = temporary.path();
    copy_fixture_tree(&source_root, root);
    let copied_manifest = root.join(format!("{corpus}.bproj"));
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
        let mod_host = enter_mod_host();
        for (target, entry, code) in &targets {
            let resolved = resolve_fixture(root, entry, target);
            let result = beskid_queries::prepare_compilation_diagnostics(
                &resolved,
                PrepareOptions {
                    front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
                    native_mod_adapter_sources: false,
                    mod_host: mod_host_for(&mod_host, &resolved),
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
                    (
                        diagnostic.code.clone().unwrap_or_default(),
                        diagnostic.src.name().to_owned(),
                        diagnostic.message.clone(),
                    )
                })
                .collect::<Vec<_>>();
            let reported = errors.iter().any(|(found, source, _)| found == code && Path::new(source).ends_with(entry));
            let internal = errors.iter().any(|(found, _, _)| found.starts_with("E21"));
            if !reported || internal {
                failures.push(format!("{target}: expected {code} in {entry}, got {errors:?}"));
            }
        }
    });
    assert!(
        !source_root.join("Project.lock").exists(),
        "{corpus} compile-fail test must not write a lockfile into the checked-in fixture"
    );
    assert!(failures.is_empty(), "{corpus} compile-fail corpus regressions:\n{}", failures.join("\n"));
}

#[test]
fn every_parallel_compile_fail_target_is_rejected_with_its_code() {
    assert_corpus_rejected("parallel", &["E1206", "E1234", "E1235", "E1236", "E1610"]);
}

#[test]
fn every_resolution_compile_fail_target_is_rejected_with_its_code() {
    assert_corpus_rejected("resolution", &["E1101", "E1105"]);
}
