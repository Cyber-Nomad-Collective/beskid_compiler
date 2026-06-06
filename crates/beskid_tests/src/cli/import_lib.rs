//! End-to-end tests for `beskid import lib <name>`.
//!
//! These exercise the same resolution + manifest merge that the CLI binary runs, so the
//! Project.proj mutation behavior is locked in even when the build infrastructure prevents
//! invoking the binary directly from CI.

use std::fs;
use std::path::PathBuf;

use beskid_analysis::external_library::{
    LibraryResolveError, default_registry, known_provider_ids,
    merge_resolution_into_manifest_source,
};
use beskid_analysis::projects::parse_manifest as parse_project_manifest;

fn write_minimal_manifest(dir: &std::path::Path) -> PathBuf {
    let manifest = dir.join("Project.proj");
    fs::write(
        &manifest,
        "project {\n  name = \"ImportTest\"\n  version = \"0.1.0\"\n}\n\n\
         target \"App\" {\n  kind = App\n  entry = \"Main.bd\"\n}\n",
    )
    .expect("write Project.proj");
    manifest
}

#[test]
fn import_lib_libc_writes_link_block_and_roundtrips_through_parser() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let manifest_path = write_minimal_manifest(tmp.path());
    let before = fs::read_to_string(&manifest_path).expect("read before");

    let registry = default_registry();
    let resolution = registry
        .resolve("c-posix", "linux", "libc")
        .expect("resolve libc");
    assert_eq!(resolution.link_args, vec!["-lc"]);

    let parsed_before = parse_project_manifest(&before).expect("parse before");
    let outcome =
        merge_resolution_into_manifest_source(&before, parsed_before.link.as_ref(), &resolution);

    fs::write(&manifest_path, &outcome.updated_source).expect("write back");

    let after = fs::read_to_string(&manifest_path).expect("read after");
    assert!(
        after.contains("link {"),
        "expected link block written, got:\n{after}",
    );
    assert!(after.contains("libraries = [libc]"));
    assert_eq!(outcome.added_libraries, vec!["libc"]);

    // After-state must round-trip through the manifest parser without diagnostics.
    let parsed_after = parse_project_manifest(&after).expect("parse after");
    let link_section = parsed_after.link.expect("link section parsed");
    assert_eq!(link_section.libraries, vec!["libc"]);
    assert!(link_section.search_paths.is_empty());
    assert!(link_section.extra_args.is_empty());
}

#[test]
fn import_lib_is_idempotent_on_repeat_invocations() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let manifest_path = write_minimal_manifest(tmp.path());

    let registry = default_registry();
    let resolution = registry
        .resolve("c-posix", "linux", "libc")
        .expect("resolve libc");

    let initial = fs::read_to_string(&manifest_path).expect("read initial");
    let outcome_one = merge_resolution_into_manifest_source(&initial, None, &resolution);
    fs::write(&manifest_path, &outcome_one.updated_source).expect("write once");

    let first = fs::read_to_string(&manifest_path).expect("read after first import");
    let parsed_first = parse_project_manifest(&first).expect("parse first");
    let outcome_two =
        merge_resolution_into_manifest_source(&first, parsed_first.link.as_ref(), &resolution);
    assert!(
        outcome_two.added_libraries.is_empty(),
        "second invocation should add nothing"
    );
    assert_eq!(outcome_two.updated_source, first);
}

#[test]
fn import_lib_merges_into_existing_link_block() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let manifest_path = tmp.path().join("Project.proj");
    fs::write(
        &manifest_path,
        "project {\n  name = \"ImportTest\"\n  version = \"0.1.0\"\n}\n\n\
         target \"App\" {\n  kind = App\n  entry = \"Main.bd\"\n}\n\n\
         link {\n  libraries = [pthread]\n}\n",
    )
    .expect("write Project.proj");

    let registry = default_registry();
    let resolution = registry
        .resolve("c-posix", "linux", "libc")
        .expect("resolve libc");

    let before = fs::read_to_string(&manifest_path).expect("read before");
    let parsed_before = parse_project_manifest(&before).expect("parse before");
    let outcome =
        merge_resolution_into_manifest_source(&before, parsed_before.link.as_ref(), &resolution);
    fs::write(&manifest_path, &outcome.updated_source).expect("write merged");

    let after = fs::read_to_string(&manifest_path).expect("read after");
    let parsed_after = parse_project_manifest(&after).expect("parse after");
    let link = parsed_after.link.expect("link section");
    assert_eq!(link.libraries, vec!["pthread", "libc"]);
    assert_eq!(outcome.added_libraries, vec!["libc"]);
}

#[test]
fn import_lib_rejects_unknown_provider() {
    let registry = default_registry();
    let err = registry
        .resolve("msvc", "linux", "libc")
        .expect_err("msvc must be rejected by closed registry");
    match err {
        LibraryResolveError::UnknownProvider { provider, known } => {
            assert_eq!(provider, "msvc");
            assert!(known.contains("c-posix"), "known list: {known}");
        }
        other => panic!("expected UnknownProvider, got {other:?}"),
    }
}

#[test]
fn import_lib_rejects_unknown_library_name() {
    let registry = default_registry();
    let err = registry
        .resolve("c-posix", "linux", "totally-not-a-real-libname")
        .expect_err("unknown library must be rejected");
    match err {
        LibraryResolveError::UnknownLogicalName { provider, .. } => {
            assert_eq!(provider, "c-posix");
        }
        other => panic!("expected UnknownLogicalName, got {other:?}"),
    }
}

#[test]
fn closed_registry_does_not_include_winapi_or_msvc() {
    let ids = known_provider_ids();
    assert!(ids.contains(&"c-posix"));
    assert!(ids.contains(&"posix"));
    assert!(!ids.contains(&"msvc"), "ids: {ids:?}");
    assert!(!ids.contains(&"winapi"), "ids: {ids:?}");
}

#[test]
fn import_lib_writes_search_path_for_path_input() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let manifest_path = write_minimal_manifest(tmp.path());
    let registry = default_registry();
    let resolution = registry
        .resolve("c-posix", "linux", "/opt/local/lib/libfoo.so")
        .expect("resolve path input");
    assert_eq!(resolution.search_paths.len(), 1);

    let before = fs::read_to_string(&manifest_path).expect("read before");
    let outcome = merge_resolution_into_manifest_source(&before, None, &resolution);
    let after = outcome.updated_source;

    let parsed_after = parse_project_manifest(&after).expect("parse after");
    let link = parsed_after.link.expect("link section");
    assert_eq!(link.libraries, vec!["/opt/local/lib/libfoo.so"]);
    assert_eq!(link.search_paths, vec!["/opt/local/lib"]);
}
