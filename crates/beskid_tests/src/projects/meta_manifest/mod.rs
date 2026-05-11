//! **Meta** `Project.proj` / `project.meta` contract tests (platform-spec tooling manifests +
//! workspace resolution).

use std::fs;
use std::path::{Path, PathBuf};

use beskid_analysis::projects::{
    AttachToSelector, PROJECT_FILE_NAME, ProjectError, ProjectGraphBuildOptions, ProjectGraphNode,
    ProjectKind, build_project_graph, build_project_graph_with_options, parse_manifest,
    parse_workspace_manifest,
};

use crate::test_harness::{
    temp_case_dir, write_project_manifest as write_manifest, write_workspace_manifest,
};

use super::test_cwd::with_cwd_at_workspace_root;

fn meta_manifest_fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/projects/meta_manifest")
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create fixture copy root");
    for entry in fs::read_dir(src).expect("read fixture dir") {
        let entry = entry.expect("dir entry");
        let file_type = entry.file_type().expect("file type");
        let dest_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path);
        } else {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).expect("create parent");
            }
            fs::copy(entry.path(), &dest_path).expect("copy fixture file");
        }
    }
}

#[test]
fn contract_anchor_file_exists() {
    let path = meta_manifest_fixture_root().join("CONTRACT.txt");
    assert!(
        path.is_file(),
        "expected CONTRACT.txt under {}",
        path.display()
    );
    let text = fs::read_to_string(&path).expect("read CONTRACT");
    assert!(
        text.contains("project-manifest-contract"),
        "CONTRACT should point readers at platform-spec anchors"
    );
}

#[test]
fn stub_meta_host_project_parses_with_current_manifest_parser() {
    let manifest_path = meta_manifest_fixture_root().join("stub_meta_host/Project.proj");
    let source = fs::read_to_string(&manifest_path).expect("read stub host manifest");
    let manifest = parse_manifest(&source).expect("valid Project.proj for host stub");
    assert_eq!(manifest.project.name, "StubHost");
    assert_eq!(manifest.targets.len(), 1);
}

#[test]
fn future_meta_sketch_documents_attach_to_and_entry_modules() {
    let path = meta_manifest_fixture_root().join("future_meta_fields.proj.future");
    let raw = fs::read_to_string(&path).expect("read future sketch");
    assert!(
        raw.contains("attachTo"),
        "fixture should name attachTo (platform-spec project.meta)"
    );
    assert!(
        raw.contains("entryModules"),
        "fixture should name entryModules (platform-spec project.meta)"
    );
}

#[test]
fn path_dependency_cycle_fixture_surfaces_dependency_cycle_error() {
    // Graph-phase cycle detection already exists; this anchors the on-disk layout for the
    // related meta/workspace cycle cases called out in platform-spec (separate from path deps).
    let root = temp_case_dir("meta_manifest_path_cycle_copy");
    let fixture_src = meta_manifest_fixture_root().join("path_cycle_workspace");
    copy_dir_recursive(&fixture_src, &root);

    let manifest_path = root.join("ring_a").join(PROJECT_FILE_NAME);
    let err = build_project_graph(&manifest_path).expect_err("expected dependency cycle");
    assert!(
        matches!(err, ProjectError::DependencyCycle(_)),
        "unexpected error: {err:?}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn multi_member_workspace_fixture_parses() {
    // Production Meta packs should prefer explicit member ids over `default` when multiple
    // workspace members exist (see project-manifest-contract examples + workspace resolution).
    let workspace_path = meta_manifest_fixture_root().join("multi_member_workspace/Workspace.proj");
    let source = fs::read_to_string(&workspace_path).expect("read workspace");
    let ws = parse_workspace_manifest(&source).expect("valid workspace");
    assert_eq!(ws.members.len(), 2);
    let names: Vec<_> = ws.members.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"app"));
    assert!(names.contains(&"tools"));
}

#[test]
fn parses_meta_nested_block_and_selectors() {
    let src = r#"
project {
  name = "m"
  version = "0.1.0"
  type = Meta
  meta {
    attachTo = [default, "hostA"]
    entryModules = ["Meta/Mod.bd"]
    maxMetaRounds = 6
    capabilities = [read_project_sources]
  }
}
"#;
    let m = parse_manifest(src).expect("parse");
    assert_eq!(m.project.kind, ProjectKind::Meta);
    let meta = m.project.meta.as_ref().expect("meta");
    assert_eq!(meta.attach_to.len(), 2);
    assert!(matches!(meta.attach_to[0], AttachToSelector::Default));
    assert!(matches!(
        &meta.attach_to[1],
        AttachToSelector::Member(name) if name == "hostA"
    ));
    assert_eq!(meta.entry_modules, vec!["Meta/Mod.bd".to_string()]);
    assert_eq!(meta.max_meta_rounds, Some(6));
    assert_eq!(
        meta.capabilities,
        Some(vec!["read_project_sources".to_string()])
    );
}

#[test]
fn meta_attach_to_rejects_unknown_workspace_member_id() {
    let root = temp_case_dir("meta_unknown_member");
    let app_dir = root.join("App");
    let meta_dir = root.join("MetaPack");
    fs::create_dir_all(app_dir.join("Src")).expect("mkdir");
    fs::create_dir_all(meta_dir.join("Src/Meta")).expect("mkdir");
    fs::write(app_dir.join("Src/Main.bd"), "// host\n").expect("write main");
    fs::write(meta_dir.join("Src/Meta/Mod.bd"), "// meta\n").expect("write meta mod");

    write_workspace_manifest(
        &root,
        r#"workspace {
  name = "w"
}
member "app" {
  path = "App"
}
member "meta" {
  path = "MetaPack"
}
"#,
    );

    write_manifest(
        &app_dir,
        r#"
project {
  name = "App"
  version = "0.1.0"
}

target "main" {
  kind = App
  entry = "Main.bd"
}

dependency "MetaPack" {
  source = path
  path = "../MetaPack"
}
"#,
    );

    write_manifest(
        &meta_dir,
        r#"
project {
  name = "Mp"
  version = "0.1.0"
  type = Meta
  meta {
    attachTo = ["nosuchmember"]
    entryModules = ["Meta/Mod.bd"]
  }
}
"#,
    );

    let manifest_path = app_dir.join(PROJECT_FILE_NAME);
    with_cwd_at_workspace_root(&root, || {
        let err = build_project_graph(&manifest_path).expect_err("unknown attachTo member");
        assert_eq!(err.code(), "E1813");
        let ProjectError::MetaContractViolation { message: msg, .. } = err else {
            panic!("unexpected: {err:?}");
        };
        assert!(
            msg.contains("nosuchmember"),
            "message should name unknown member: {msg}"
        );
    });

    let _ = fs::remove_dir_all(root);
}

#[test]
fn meta_default_attach_to_requires_disambiguation_when_multiple_members() {
    let root = temp_case_dir("meta_default_ambiguous");
    let app_dir = root.join("App");
    let meta_dir = root.join("MetaPack");
    fs::create_dir_all(app_dir.join("Src")).expect("mkdir");
    fs::create_dir_all(meta_dir.join("Src/Meta")).expect("mkdir");
    fs::write(app_dir.join("Src/Main.bd"), "// host\n").expect("write main");
    fs::write(meta_dir.join("Src/Meta/Mod.bd"), "// meta\n").expect("write meta mod");

    write_workspace_manifest(
        &root,
        r#"workspace {
  name = "w"
}
member "app" {
  path = "App"
}
member "meta" {
  path = "MetaPack"
}
"#,
    );

    write_manifest(
        &app_dir,
        r#"
project {
  name = "App"
  version = "0.1.0"
}

target "main" {
  kind = App
  entry = "Main.bd"
}

dependency "MetaPack" {
  source = path
  path = "../MetaPack"
}
"#,
    );

    write_manifest(
        &meta_dir,
        r#"
project {
  name = "Mp"
  version = "0.1.0"
  type = Meta
  meta {
    attachTo = default
    entryModules = ["Meta/Mod.bd"]
  }
}
"#,
    );

    let manifest_path = app_dir.join(PROJECT_FILE_NAME);
    with_cwd_at_workspace_root(&root, || {
        let err = build_project_graph(&manifest_path).expect_err("ambiguous default");
        assert_eq!(err.code(), "E1818");
        let ProjectError::MetaContractViolation { message: msg, .. } = err else {
            panic!("unexpected {err:?}");
        };
        assert!(
            msg.contains("ambiguous") || msg.contains("default"),
            "{msg}"
        );
    });

    with_cwd_at_workspace_root(&root, || {
        build_project_graph_with_options(
            &manifest_path,
            ProjectGraphBuildOptions {
                workspace_member_for_meta_default: Some("app".to_string()),
            },
        )
        .expect("explicit workspace member disambiguates default");
    });

    let _ = fs::remove_dir_all(root);
}

#[test]
fn meta_single_workspace_member_resolves_default_attach_to() {
    let root = temp_case_dir("meta_default_single_member");
    let app_dir = root.join("App");
    let meta_dir = root.join("MetaPack");
    fs::create_dir_all(app_dir.join("Src")).expect("mkdir");
    fs::create_dir_all(meta_dir.join("Src/Meta")).expect("mkdir");
    fs::write(app_dir.join("Src/Main.bd"), "// host\n").expect("write main");
    fs::write(meta_dir.join("Src/Meta/Mod.bd"), "// meta\n").expect("write meta mod");

    write_workspace_manifest(
        &root,
        r#"workspace {
  name = "w"
}
member "app" {
  path = "App"
}
"#,
    );

    write_manifest(
        &app_dir,
        r#"
project {
  name = "App"
  version = "0.1.0"
}

target "main" {
  kind = App
  entry = "Main.bd"
}

dependency "MetaPack" {
  source = path
  path = "../MetaPack"
}
"#,
    );

    write_manifest(
        &meta_dir,
        r#"
project {
  name = "Mp"
  version = "0.1.0"
  type = Meta
  meta {
    attachTo = default
    entryModules = ["Meta/Mod.bd"]
  }
}
"#,
    );

    let manifest_path = app_dir.join(PROJECT_FILE_NAME);
    with_cwd_at_workspace_root(&root, || {
        let graph = build_project_graph(&manifest_path).expect("graph");
        let mut found = false;
        for w in graph.dag.graph().node_weights() {
            if let ProjectGraphNode::ResolvedPathDependency {
                project_name,
                meta_attachments,
                ..
            } = w
            {
                if project_name == "Mp" {
                    let att = meta_attachments.as_ref().expect("resolved attachments");
                    assert_eq!(att.host_member_ids, vec!["app".to_string()]);
                    found = true;
                }
            }
        }
        assert!(found, "expected MetaPack dependency node");
    });

    let _ = fs::remove_dir_all(root);
}

#[test]
fn meta_must_not_attach_to_meta_member() {
    let root = temp_case_dir("meta_to_meta");
    let host_dir = root.join("Host");
    let inner_meta_dir = root.join("InnerMeta");
    let outer_meta_dir = root.join("OuterMeta");
    for d in [&host_dir, &inner_meta_dir, &outer_meta_dir] {
        fs::create_dir_all(d.join("Src/Meta")).expect("mkdir");
        fs::write(d.join("Src/Meta/Mod.bd"), "//\n").expect("bd");
    }
    fs::write(host_dir.join("Src/Main.bd"), "//\n").expect("main");

    write_workspace_manifest(
        &root,
        r#"workspace {
  name = "w"
}
member "host" {
  path = "Host"
}
member "inner" {
  path = "InnerMeta"
}
"#,
    );

    write_manifest(
        &host_dir,
        r#"
project {
  name = "Host"
  version = "0.1.0"
}

target "main" {
  kind = App
  entry = "Main.bd"
}
"#,
    );

    write_manifest(
        &inner_meta_dir,
        r#"
project {
  name = "Inner"
  version = "0.1.0"
  type = Meta
  meta {
    attachTo = ["host"]
    entryModules = ["Meta/Mod.bd"]
  }
}
"#,
    );

    write_manifest(
        &outer_meta_dir,
        r#"
project {
  name = "Outer"
  version = "0.1.0"
  type = Meta
  meta {
    attachTo = ["inner"]
    entryModules = ["Meta/Mod.bd"]
  }
}
"#,
    );

    let manifest_path = outer_meta_dir.join(PROJECT_FILE_NAME);
    with_cwd_at_workspace_root(&root, || {
        let err = build_project_graph(&manifest_path).expect_err("meta attaches to meta");
        assert_eq!(err.code(), "E1814");
        let ProjectError::MetaContractViolation { message: msg, .. } = err else {
            panic!("{err:?}");
        };
        assert!(
            msg.contains("must not attach") || msg.contains("Meta"),
            "{msg}"
        );
    });

    let _ = fs::remove_dir_all(root);
}
