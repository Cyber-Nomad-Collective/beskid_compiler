use beskid_analysis::projects::{ProjectError, ProjectLockDependencyEntry};

#[test]
fn lock_entry_roundtrips_with_optional_fields() {
    let line = "name=PkgCore;manifest=/tmp/Pkg/Project.proj;project=/tmp/Pkg;source_root=/tmp/Pkg/Src;materialized_root=/tmp/App/obj/beskid/deps/src/pkg;resolved_version=1.2.3;artifact_digest=sha256:abc;registry=default";

    let parsed = ProjectLockDependencyEntry::parse_v1_line(line).expect("parse v1 lock line");
    assert_eq!(parsed.to_v1_line(), line);
}

#[test]
fn lock_entry_roundtrips_without_optional_fields() {
    let line = "name=Core;manifest=/tmp/Core/Project.proj;project=/tmp/Core;source_root=/tmp/Core/Src;materialized_root=/tmp/App/obj/beskid/deps/src/core";

    let parsed = ProjectLockDependencyEntry::parse_v1_line(line).expect("parse v1 lock line");
    assert_eq!(parsed.to_v1_line(), line);
}

#[test]
fn lock_entry_parse_rejects_missing_required_fields() {
    let error = ProjectLockDependencyEntry::parse_v1_line("name=Core;manifest=/tmp/Core/Project.proj")
        .expect_err("missing required fields should fail");
    assert!(matches!(error, ProjectError::Validation(_)));
}
