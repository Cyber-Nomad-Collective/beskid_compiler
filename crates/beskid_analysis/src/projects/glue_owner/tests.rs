use std::{fs, path::Path};

use super::{
    GLUE_OWNER_MAX_FILE_BYTES, GLUE_OWNER_MAX_FILES, collect_glue_owner_sources, glue_owner_directory,
};
use crate::projects::{
    error::ProjectError,
    model::{ProjectGlueBackend, ProjectGlueOwner},
};

fn owner(path: &str) -> ProjectGlueOwner {
    ProjectGlueOwner { library: "glue_manual".to_string(), backend: ProjectGlueBackend::Rust, path: path.to_string() }
}

fn write(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, bytes).expect("write");
}

fn project_with_entry() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    write(temp.path(), "rust/implementation.rs", b"pub struct ManualOwned;\n");
    temp
}

#[track_caller]
fn expect_code(result: Result<impl std::fmt::Debug, ProjectError>, expected_code: &str, needle: &str) {
    match result.expect_err("collection must fail") {
        ProjectError::MetaContractViolation { code, message } => {
            assert_eq!(code, expected_code, "{message}");
            assert!(message.contains(needle), "message `{message}` does not name `{needle}`");
        }
        other => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn valid_owner_directory_collects_sorted_closed_inventory() {
    let temp = project_with_entry();
    write(temp.path(), "rust/support/codec.rs", b"pub fn codec() {}\n");
    write(temp.path(), "rust/alpha.rs", b"pub fn alpha() {}\n");

    let sources = collect_glue_owner_sources(temp.path(), &owner("rust")).expect("collect");

    let canonical_root = fs::canonicalize(temp.path()).expect("canonical root");
    assert_eq!(sources.library, "glue_manual");
    assert_eq!(sources.directory, canonical_root.join("rust"));
    let names: Vec<&str> = sources.files.iter().map(|file| file.relative_path.as_str()).collect();
    assert_eq!(names, ["alpha.rs", "implementation.rs", "support/codec.rs"]);
    assert_eq!(sources.files[1].bytes, b"pub struct ManualOwned;\n");
}

#[test]
fn current_dir_components_resolve_to_the_same_directory() {
    let temp = project_with_entry();
    let directory = glue_owner_directory(temp.path(), &owner("./rust/.")).expect("directory");
    assert_eq!(directory, fs::canonicalize(temp.path()).expect("root").join("rust"));
}

#[test]
fn missing_implementation_entry_is_rejected() {
    let temp = tempfile::tempdir().expect("tempdir");
    write(temp.path(), "rust/lib.rs", b"");
    expect_code(collect_glue_owner_sources(temp.path(), &owner("rust")), "E1865", "rust/implementation.rs");
}

#[test]
fn nested_implementation_does_not_satisfy_the_entry() {
    let temp = tempfile::tempdir().expect("tempdir");
    write(temp.path(), "rust/nested/implementation.rs", b"");
    expect_code(collect_glue_owner_sources(temp.path(), &owner("rust")), "E1865", "rust/implementation.rs");
}

#[test]
fn reserved_cargo_and_bridge_files_are_rejected() {
    for reserved in ["Cargo.toml", "Cargo.lock", "build.rs", "owner_bridge.rs", "nested/build.rs"] {
        let temp = project_with_entry();
        write(temp.path(), &format!("rust/{reserved}"), b"");
        expect_code(collect_glue_owner_sources(temp.path(), &owner("rust")), "E1865", &format!("rust/{reserved}"));
    }
}

#[test]
fn non_rust_files_are_rejected_not_ignored() {
    for other in ["README.md", ".DS_Store", "implementation.rs.bak", ".rs", "deep/notes.txt"] {
        let temp = project_with_entry();
        write(temp.path(), &format!("rust/{other}"), b"x");
        expect_code(collect_glue_owner_sources(temp.path(), &owner("rust")), "E1865", &format!("rust/{other}"));
    }
}

#[test]
fn non_utf8_source_is_rejected() {
    let temp = project_with_entry();
    write(temp.path(), "rust/binary.rs", &[0xff, 0xfe, 0x00]);
    expect_code(collect_glue_owner_sources(temp.path(), &owner("rust")), "E1865", "rust/binary.rs");
}

#[cfg(unix)]
#[test]
fn symbolic_link_inside_owner_directory_is_rejected() {
    let temp = project_with_entry();
    write(temp.path(), "elsewhere/target.rs", b"");
    std::os::unix::fs::symlink(temp.path().join("elsewhere/target.rs"), temp.path().join("rust/linked.rs"))
        .expect("symlink");
    expect_code(collect_glue_owner_sources(temp.path(), &owner("rust")), "E1865", "rust/linked.rs");
}

#[cfg(unix)]
#[test]
fn symbolic_link_directory_inside_owner_directory_is_rejected() {
    let temp = project_with_entry();
    write(temp.path(), "elsewhere/target.rs", b"");
    std::os::unix::fs::symlink(temp.path().join("elsewhere"), temp.path().join("rust/linked")).expect("symlink");
    expect_code(collect_glue_owner_sources(temp.path(), &owner("rust")), "E1865", "rust/linked");
}

#[cfg(unix)]
#[test]
fn owner_directory_reached_through_symbolic_link_is_rejected() {
    let temp = project_with_entry();
    std::os::unix::fs::symlink(temp.path().join("rust"), temp.path().join("alias")).expect("symlink");
    expect_code(collect_glue_owner_sources(temp.path(), &owner("alias")), "E1863", "symbolic link");
}

#[cfg(unix)]
#[test]
fn owner_directory_escaping_through_symbolic_link_is_rejected() {
    let outside = tempfile::tempdir().expect("outside");
    write(outside.path(), "implementation.rs", b"");
    let project = tempfile::tempdir().expect("project");
    std::os::unix::fs::symlink(outside.path(), project.path().join("rust")).expect("symlink");
    expect_code(collect_glue_owner_sources(project.path(), &owner("rust")), "E1863", "outside the project root");
}

#[test]
fn lexically_escaping_or_absolute_paths_are_rejected() {
    let temp = project_with_entry();
    for path in ["../rust", "rust/../../x", "/abs/rust", ""] {
        expect_code(collect_glue_owner_sources(temp.path(), &owner(path)), "E1863", "glue `glue_manual`");
    }
}

#[test]
fn missing_or_non_directory_path_is_rejected() {
    let temp = project_with_entry();
    expect_code(collect_glue_owner_sources(temp.path(), &owner("missing")), "E1863", "cannot be resolved");
    expect_code(
        collect_glue_owner_sources(temp.path(), &owner("rust/implementation.rs")),
        "E1863",
        "is not a directory",
    );
}

#[test]
fn dotnet_backend_is_unavailable() {
    let temp = project_with_entry();
    let dotnet = ProjectGlueOwner { backend: ProjectGlueBackend::Dotnet, ..owner("rust") };
    expect_code(collect_glue_owner_sources(temp.path(), &dotnet), "E1862", "dotnet");
}

#[test]
fn oversized_source_file_is_rejected() {
    let temp = project_with_entry();
    write(temp.path(), "rust/large.rs", &vec![b' '; GLUE_OWNER_MAX_FILE_BYTES as usize + 1]);
    expect_code(collect_glue_owner_sources(temp.path(), &owner("rust")), "E1866", "rust/large.rs");
}

#[test]
fn total_source_bound_is_enforced() {
    let temp = project_with_entry();
    let chunk = vec![b' '; 7 * 1024 * 1024];
    for index in 0..5 {
        write(temp.path(), &format!("rust/chunk_{index}.rs"), &chunk);
    }
    expect_code(collect_glue_owner_sources(temp.path(), &owner("rust")), "E1866", "total source bytes");
}

#[test]
fn source_file_count_bound_is_enforced() {
    let temp = project_with_entry();
    for index in 0..GLUE_OWNER_MAX_FILES {
        write(temp.path(), &format!("rust/m{index:04}.rs"), b"");
    }
    expect_code(collect_glue_owner_sources(temp.path(), &owner("rust")), "E1866", "source files");
}
