use std::fs;

use beskid_analysis::projects::{
    PortableLockPath, PortableLockPathBaseKind, ProjectError, ProjectLockfile,
};

const PATH_LOCK: &str = "# Project.lock v3\nroot_manifest=Project.proj\nproject_name=App\ndependencies:\n- name=Shared;source=path;project=../shared;manifest=Project.proj;source_root=Src;materialized_root=obj/beskid/deps/src/shared\n";

fn parse_rejects(content: &str) {
    assert!(ProjectLockfile::parse(content).is_err(), "malformed lock must be rejected: {content:?}");
}

#[test]
fn lock_serialization_has_exact_header_and_sorts_dependencies() {
    let unsorted = "# Project.lock v3\nroot_manifest=Project.proj\nproject_name=App\ndependencies:\n- name=Zulu;source=path;project=../zulu;manifest=Project.proj;source_root=Src;materialized_root=obj/beskid/deps/src/zulu\n- name=Alpha;source=path;project=../alpha;manifest=Project.proj;source_root=Src;materialized_root=obj/beskid/deps/src/alpha\n";
    let expected = "# Project.lock v3\nroot_manifest=Project.proj\nproject_name=App\ndependencies:\n- name=Alpha;source=path;project=../alpha;manifest=Project.proj;source_root=Src;materialized_root=obj/beskid/deps/src/alpha\n- name=Zulu;source=path;project=../zulu;manifest=Project.proj;source_root=Src;materialized_root=obj/beskid/deps/src/zulu\n";

    let parsed = ProjectLockfile::parse(unsorted).expect("parse unsorted lock");
    assert_eq!(parsed.to_content(), expected);
    assert_eq!(ProjectLockfile::parse(&parsed.to_content()).expect("parse serialized lock").to_content(), expected);
}

#[test]
fn lock_accepts_corelib_source_with_relative_installed_workspace_identity() {
    let content = "# Project.lock v3\nroot_manifest=Project.proj\nproject_name=App\ndependencies:\n- name=Core;source=corelib;project=stdlib/core;manifest=Project.proj;source_root=Src\n";
    let parsed = ProjectLockfile::parse(content).expect("parse portable Corelib identity");
    assert_eq!(parsed.to_content(), content);
}

#[test]
fn corelib_entries_name_the_shared_corelib_and_have_no_project_local_copy() {
    // A Corelib entry is read in place from the toolchain's shared Corelib, so a copy destination
    // is not part of its record; one that names a copy is a retired v2 shape and is rejected.
    parse_rejects(
        "# Project.lock v3\nroot_manifest=Project.proj\nproject_name=App\ndependencies:\n- name=Core;source=corelib;project=stdlib/core;manifest=Project.proj;source_root=Src;materialized_root=obj/beskid/deps/src/core\n",
    );
    // Path and registry entries still require their copy destination.
    parse_rejects(&PATH_LOCK.replace(";materialized_root=obj/beskid/deps/src/shared", ""));
    // Two Corelib entries never collide on a destination.
    let two = "# Project.lock v3\nroot_manifest=Project.proj\nproject_name=App\ndependencies:\n- name=Core;source=corelib;project=beskid_corelib;manifest=corelib.bproj;source_root=.\n- name=corelib_text;source=corelib;project=packages/text;manifest=corelib_text.bproj;source_root=src\n";
    assert_eq!(ProjectLockfile::parse(two).expect("two Corelib entries").to_content(), two);
}

#[test]
fn retired_lock_formats_are_not_parsed_as_current() {
    parse_rejects(&PATH_LOCK.replacen("# Project.lock v3", "# Project.lock v2", 1));
}

#[test]
fn lock_uses_dot_only_when_source_root_is_the_project_directory() {
    let aggregate = PATH_LOCK.replace(";source_root=Src", ";source_root=.");
    assert_eq!(ProjectLockfile::parse(&aggregate).expect("project-root source").to_content(), aggregate);
    parse_rejects(&PATH_LOCK.replace(";manifest=Project.proj", ";manifest=."));
    parse_rejects(&PATH_LOCK.replace(";project=../shared", ";project=."));
    parse_rejects(&PATH_LOCK.replace("root_manifest=Project.proj", "root_manifest=."));
}

#[test]
fn lock_accepts_declared_external_project_at_parent_only_path() {
    let parent = PATH_LOCK.replace(";project=../shared", ";project=../..");
    assert_eq!(ProjectLockfile::parse(&parent).expect("parent-only external project").to_content(), parent);
    parse_rejects(&parent.replace("source=path", "source=corelib"));
    parse_rejects(&parent.replace(";project=../..", ";project=../../inside/.."));
}

#[test]
fn lock_requires_exact_header_and_required_top_level_fields() {
    for content in [
        PATH_LOCK.replacen("# Project.lock v3", " # Project.lock v3", 1),
        PATH_LOCK.replacen("# Project.lock v3", "# Project.lock v3 extra", 1),
        PATH_LOCK.replacen("# Project.lock v3", "# Project.lock v1", 1),
        PATH_LOCK.replace("root_manifest=Project.proj\n", ""),
        PATH_LOCK.replace("project_name=App\n", ""),
        PATH_LOCK.replace("dependencies:\n", ""),
    ] {
        parse_rejects(&content);
    }
}

#[test]
fn lock_uses_canonical_uppercase_percent_encoding_for_utf8_values() {
    let encoded = PATH_LOCK.replace("project_name=App", "project_name=Caf%C3%A9%20App%3B1%3D2");
    let parsed = ProjectLockfile::parse(&encoded).expect("parse canonical UTF-8 encoding");
    assert_eq!(parsed.to_content(), encoded);

    for noncanonical in [
        encoded.replace("%C3%A9", "%c3%A9"),
        encoded.replace("%20", "%2o"),
        encoded.replace("%20", "%41"),
        encoded.replace("%20", " "),
        encoded.replace("%C3%A9", "%FF"),
        encoded.replace("%20", "%"),
    ] {
        parse_rejects(&noncanonical);
    }
}

#[test]
fn lock_rejects_raw_delimiters_duplicate_and_unknown_fields() {
    for malformed in [
        PATH_LOCK.replace("name=Shared", "name=Shared;name=Again"),
        PATH_LOCK.replace("name=Shared", "name=Shared;future=value"),
        PATH_LOCK.replace("project_name=App", "project_name=App\nproject_name=Again"),
        PATH_LOCK.replace("project_name=App", "project_name=App\nfuture=value"),
        PATH_LOCK.replace("name=Shared", "name=Shared;Other"),
        PATH_LOCK.replace("name=Shared", "name=Shared=Other"),
    ] {
        parse_rejects(&malformed);
    }
}

#[test]
fn lock_rejects_duplicate_dependency_names_and_materialized_destinations() {
    let duplicate_name = format!(
        "{PATH_LOCK}- name=Shared;source=path;project=../other;manifest=Project.proj;source_root=Src;materialized_root=obj/beskid/deps/src/other\n"
    );
    let duplicate_destination = format!(
        "{PATH_LOCK}- name=Other;source=path;project=../other;manifest=Project.proj;source_root=Src;materialized_root=obj/beskid/deps/src/shared\n"
    );
    parse_rejects(&duplicate_name);
    parse_rejects(&duplicate_destination);
}

#[test]
fn lock_registry_entries_require_a_lowercase_sha256_digest() {
    let digest = "0123456789abcdef".repeat(4);
    let valid = format!(
        "# Project.lock v3\nroot_manifest=Project.proj\nproject_name=App\ndependencies:\n- name=Widget;source=registry;project=deps/widget;manifest=Project.proj;source_root=Src;materialized_root=obj/beskid/deps/src/widget;registry=main;resolved_version=1.2.3;artifact_digest=sha256:{digest}\n"
    );
    assert_eq!(ProjectLockfile::parse(&valid).expect("parse pinned registry entry").to_content(), valid);

    for malformed in [
        valid.replace(&digest, &digest[..63]),
        valid.replace(&digest, &digest.to_uppercase()),
        valid.replace(&digest, &format!("{}g", &digest[..63])),
        valid.replace("sha256:", "sha512:"),
        valid.replace(";artifact_digest=sha256:", ";artifact_digest="),
        valid.replace(";registry=main", ""),
        valid.replace(";resolved_version=1.2.3", ""),
    ] {
        parse_rejects(&malformed);
    }
}

#[test]
fn lock_rejects_absolute_drive_unc_and_unsafe_relative_paths() {
    for (field, original, replacement) in [
        ("root_manifest", "\nroot_manifest=Project.proj", "\nroot_manifest=/tmp/Project.proj"),
        ("root_manifest", "\nroot_manifest=Project.proj", "\nroot_manifest=../Project.proj"),
        ("project", ";project=../shared", ";project=C:/shared"),
        ("project", ";project=../shared", ";project=C:%5Cshared"),
        ("project", ";project=../shared", ";project=//server/share"),
        ("project", ";project=../shared", ";project=/tmp/shared"),
        ("project", ";project=../shared", ";project=../shared/../../escape"),
        ("project", ";project=../shared", ";project=shared//nested"),
        ("manifest", ";manifest=Project.proj", ";manifest=../Project.proj"),
        ("source_root", ";source_root=Src", ";source_root=../Src"),
        (
            "materialized_root",
            ";materialized_root=obj/beskid/deps/src/shared",
            ";materialized_root=obj/beskid/deps/other",
        ),
        (
            "materialized_root",
            ";materialized_root=obj/beskid/deps/src/shared",
            ";materialized_root=obj/beskid/deps/src/../outside",
        ),
    ] {
        let malformed = PATH_LOCK.replacen(original, replacement, 1);
        assert_ne!(malformed, PATH_LOCK, "fixture must replace {field}");
        parse_rejects(&malformed);
    }
}

#[test]
fn portable_external_project_path_resolves_existing_sibling() {
    let temp = tempfile::tempdir().expect("temporary checkout");
    let checkout = temp.path().join("checkout");
    let sibling = temp.path().join("shared");
    fs::create_dir(&checkout).expect("checkout directory");
    fs::create_dir(&sibling).expect("declared sibling directory");

    let path = PortableLockPath::parse("project", "../shared", PortableLockPathBaseKind::ExternalProject)
        .expect("normalized external path");
    assert_eq!(path.as_str(), "../shared");
    assert_eq!(path.resolve(&checkout).expect("resolve declared sibling"), sibling.canonicalize().unwrap());

    let missing = PortableLockPath::parse("project", "../missing", PortableLockPathBaseKind::ExternalProject)
        .expect("well-formed external path");
    assert!(missing.resolve(&checkout).is_err());
}

#[cfg(unix)]
#[test]
fn portable_materialized_path_rejects_symlink_escape() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temporary checkout");
    let checkout = temp.path().join("checkout");
    let outside = temp.path().join("outside");
    let deps = checkout.join("obj/beskid/deps/src");
    fs::create_dir_all(&deps).expect("materialization directory");
    fs::create_dir(&outside).expect("outside directory");
    symlink(&outside, deps.join("escape")).expect("link outside the compiler-owned directory");

    let path = PortableLockPath::parse(
        "materialized_root",
        "obj/beskid/deps/src/escape",
        PortableLockPathBaseKind::MaterializedRoot,
    )
    .expect("lexically valid materialized path");
    assert!(path.resolve(&checkout).is_err());
}

#[cfg(unix)]
#[test]
fn portable_lock_directory_path_rejects_symlink_escape() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temporary checkout");
    let checkout = temp.path().join("checkout");
    let outside = temp.path().join("outside.bproj");
    fs::create_dir(&checkout).expect("checkout directory");
    fs::write(&outside, "").expect("outside manifest");
    symlink(&outside, checkout.join("Project.bproj")).expect("link outside the lock directory");

    let path = PortableLockPath::parse("root_manifest", "Project.bproj", PortableLockPathBaseKind::LockDirectory)
        .expect("lexically valid manifest path");
    assert!(path.resolve(&checkout).is_err());
}

#[cfg(unix)]
#[test]
fn portable_materialized_path_rejects_dangling_symlink() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temporary checkout");
    let checkout = temp.path().join("checkout");
    let deps = checkout.join("obj/beskid/deps/src");
    fs::create_dir_all(&deps).expect("materialization directory");
    symlink(temp.path().join("outside-not-yet-created"), deps.join("escape"))
        .expect("dangling link outside the compiler-owned directory");

    let path = PortableLockPath::parse(
        "materialized_root",
        "obj/beskid/deps/src/escape",
        PortableLockPathBaseKind::MaterializedRoot,
    )
    .expect("lexically valid materialized path");
    assert!(path.resolve(&checkout).is_err());
}

#[cfg(unix)]
#[test]
fn portable_materialized_path_rejects_in_project_owned_root_symlink() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temporary checkout");
    let checkout = temp.path().join("checkout");
    let deps = checkout.join("obj/beskid/deps");
    let unrelated = checkout.join("unrelated");
    fs::create_dir_all(&deps).expect("dependency parent");
    fs::create_dir(&unrelated).expect("unrelated in-project directory");
    symlink(&unrelated, deps.join("src")).expect("redirect compiler-owned source root");

    let path = PortableLockPath::parse(
        "materialized_root",
        "obj/beskid/deps/src/pkg",
        PortableLockPathBaseKind::MaterializedRoot,
    )
    .expect("lexically valid materialized path");
    assert!(path.resolve(&checkout).is_err());
}

#[test]
fn portable_path_error_does_not_emit_decoded_control_characters() {
    let temp = tempfile::tempdir().expect("temporary checkout");
    for decoded in ["missing\nINJECT", "missing\u{1b}[31m"] {
        let path = PortableLockPath::parse("manifest", decoded, PortableLockPathBaseKind::ProjectDirectory)
            .expect("encoded control byte is a valid path value");
        let diagnostic = path.resolve(temp.path()).expect_err("missing path").to_string();
        assert!(!diagnostic.contains('\n'), "newline must not enter diagnostic");
        assert!(!diagnostic.contains('\u{1b}'), "escape byte must not enter diagnostic");
    }
}

#[cfg(unix)]
#[test]
fn portable_materialized_ancestor_error_does_not_emit_decoded_escape() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("temporary checkout");
    let deps = temp.path().join("obj/beskid/deps/src");
    fs::create_dir_all(&deps).expect("materialization directory");
    symlink(temp.path().join("missing"), deps.join("escape\u{1b}[31m")).expect("dangling untrusted link");

    let path = PortableLockPath::parse(
        "materialized_root",
        "obj/beskid/deps/src/escape\u{1b}[31m",
        PortableLockPathBaseKind::MaterializedRoot,
    )
    .expect("encoded escape byte is a valid path value");
    let diagnostic = path.resolve(temp.path()).expect_err("dangling path").to_string();
    assert!(!diagnostic.contains('\u{1b}'), "escape byte must not enter diagnostic");
}
