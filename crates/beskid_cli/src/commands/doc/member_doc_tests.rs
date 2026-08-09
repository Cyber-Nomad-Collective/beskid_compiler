use std::time::{SystemTime, UNIX_EPOCH};

use beskid_analysis::doc::{API_JSON_NAVIGATION_MODEL_GRAPH_V1, API_JSON_SCHEMA_VERSION, ApiDocRoot};

use super::{DocArgs, execute};

#[test]
fn api_json_contains_member_doc_markdown() {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos();
    let root = std::env::temp_dir().join(format!("beskid-doc-{nonce}"));
    std::fs::create_dir_all(&root).expect("create root");
    let source_path = root.join("Sample.bd");
    let out_path = root.join("out");

    let source = r#"
type User {
    /// Display name of user.
    string name,
}
"#;
    std::fs::write(&source_path, source).expect("write source");

    execute(DocArgs {
        input: Some(source_path.clone()),
        project: crate::project_args::ProjectResolveArgs { project: None, target: None, workspace_member: None },
        lockfile: crate::project_args::LockfilePolicyArgs { frozen: false, locked: false },
        out: out_path.clone(),
    })
    .expect("execute doc");

    let api = std::fs::read_to_string(out_path.join("api.json")).expect("read api.json");
    assert!(api.contains("\"schemaVersion\": 4"), "api.json should declare schema v4: {api}");
    assert!(api.contains("\"navigationModel\": \"graph-v1\""), "api.json should declare graph navigation model: {api}");
    assert!(api.contains("\"parentId\":"), "api.json should include parentId for member rows: {api}");
    assert!(api.contains("Display name of user."), "api.json should include member doc markdown: {api}");

    let _ = std::fs::remove_file(source_path);
    let _ = std::fs::remove_file(out_path.join("api.json"));
    let _ = std::fs::remove_file(out_path.join("index.md"));
    let _ = std::fs::remove_dir(&out_path);
    let _ = std::fs::remove_dir(&root);
}

#[test]
fn api_json_graph_links_type_field_enum_variant_and_method() {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos();
    let root = std::env::temp_dir().join(format!("beskid-doc-graph-{nonce}"));
    std::fs::create_dir_all(&root).expect("create root");
    let source_path = root.join("Graph.bd");
    let out_path = root.join("out");

    let source = r#"
type Widget {
    /// widget value
    i64 value,
}

enum Mode {
    /// on
    On,
    /// off
    Off,
}

/// Adds values.
i64 Add(
    i64 left,
    i64 right
) { return left + right; }
"#;
    std::fs::write(&source_path, source).expect("write source");

    execute(DocArgs {
        input: Some(source_path.clone()),
        project: crate::project_args::ProjectResolveArgs { project: None, target: None, workspace_member: None },
        lockfile: crate::project_args::LockfilePolicyArgs { frozen: false, locked: false },
        out: out_path.clone(),
    })
    .expect("execute doc");

    let api: ApiDocRoot = serde_json::from_str(&std::fs::read_to_string(out_path.join("api.json")).expect("read"))
        .expect("parse api.json");
    assert_eq!(api.schema_version, API_JSON_SCHEMA_VERSION);
    assert_eq!(api.navigation_model.as_deref(), Some(API_JSON_NAVIGATION_MODEL_GRAPH_V1));

    let by_id = api.items.iter().filter_map(|i| i.id.map(|id| (id, i))).collect::<std::collections::HashMap<_, _>>();

    let type_row = api.items.iter().find(|i| i.kind == "type" && i.name.contains("Widget")).expect("type Widget");
    let type_id = type_row.id.expect("type id");
    let field = api.items.iter().find(|i| i.kind == "field").expect("field");
    assert_eq!(field.parent_id, Some(type_id));
    assert!(type_row.member_ids.contains(&field.id.unwrap()));

    let enum_row = api.items.iter().find(|i| i.kind == "enum").expect("enum");
    let enum_id = enum_row.id.expect("enum id");
    let variants: Vec<_> = api.items.iter().filter(|i| i.kind == "enum_variant").collect();
    assert_eq!(variants.len(), 2);
    for v in &variants {
        assert_eq!(v.parent_id, Some(enum_id));
    }

    let func = api.items.iter().find(|i| i.kind == "function" && i.name.contains("Add")).expect("function");
    assert!(
        func.parent_id.is_some(),
        "module-level functions must be parented under a module row for library-tree navigation"
    );

    // Every id referenced as parentId must exist.
    for item in &api.items {
        if let (Some(id), Some(pid)) = (item.id, item.parent_id) {
            assert!(by_id.contains_key(&pid), "item {id} parent {pid} missing");
        }
    }

    let _ = std::fs::remove_file(source_path);
    let _ = std::fs::remove_file(out_path.join("api.json"));
    let _ = std::fs::remove_file(out_path.join("index.md"));
    let _ = std::fs::remove_dir(&out_path);
    let _ = std::fs::remove_dir(&root);
}

#[test]
fn api_json_v4_emits_field_type_ref_for_nested_types() {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos();
    let root = std::env::temp_dir().join(format!("beskid-doc-nested-{nonce}"));
    std::fs::create_dir_all(&root).expect("create root");
    let source_path = root.join("Nested.bd");
    let out_path = root.join("out");
    let source = r#"
type Inner { i64 x, }
type Outer { Inner inner, }
"#;
    std::fs::write(&source_path, source).expect("write source");
    execute(DocArgs {
        input: Some(source_path.clone()),
        project: crate::project_args::ProjectResolveArgs { project: None, target: None, workspace_member: None },
        lockfile: crate::project_args::LockfilePolicyArgs { frozen: false, locked: false },
        out: out_path.clone(),
    })
    .expect("execute doc");

    let api: ApiDocRoot = serde_json::from_str(&std::fs::read_to_string(out_path.join("api.json")).expect("read"))
        .expect("parse api.json");
    assert_eq!(api.schema_version, API_JSON_SCHEMA_VERSION);
    assert!(
        !std::path::Path::new(&api.source).is_absolute(),
        "api.json source must be package-relative: {}",
        api.source
    );
    for item in &api.items {
        assert!(
            !std::path::Path::new(&item.location.file).is_absolute(),
            "location.file must be package-relative: {}",
            item.location.file
        );
    }

    let inner_type = api.items.iter().find(|i| i.kind == "type" && i.name == "Inner").expect("Inner type");
    let field = api.items.iter().find(|i| i.kind == "field" && i.name.contains("inner")).expect("inner field");
    let field_type = field.field_type.as_ref().expect("fieldType");
    assert_eq!(field_type.display, "Inner");
    assert_eq!(field_type.ref_item_id, inner_type.id);
    assert!(field.signature.as_deref().unwrap_or("").contains("Inner"), "signature should mention Inner");

    let _ = std::fs::remove_file(source_path);
    let _ = std::fs::remove_file(out_path.join("api.json"));
    let _ = std::fs::remove_file(out_path.join("index.md"));
    let _ = std::fs::remove_dir(&out_path);
    let _ = std::fs::remove_dir(&root);
}

#[test]
fn api_json_ref_markdown_is_backtick_without_project_context() {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos();
    let root = std::env::temp_dir().join(format!("beskid-doc-ref-{nonce}"));
    std::fs::create_dir_all(&root).expect("create root");
    let source_path = root.join("Refs.bd");
    let out_path = root.join("out");
    let source = r#"
/// See @ref(helper) for details.
unit Main() { return 1; }

unit helper() { return 0; }
"#;
    std::fs::write(&source_path, source).expect("write source");

    execute(DocArgs {
        input: Some(source_path.clone()),
        project: crate::project_args::ProjectResolveArgs { project: None, target: None, workspace_member: None },
        lockfile: crate::project_args::LockfilePolicyArgs { frozen: false, locked: false },
        out: out_path.clone(),
    })
    .expect("execute doc");

    let api = std::fs::read_to_string(out_path.join("api.json")).expect("read api.json");
    assert!(api.contains("`helper`") || api.contains("helper"), "resolved @ref should appear in doc markdown: {api}");
    assert!(!api.contains("/docs/"), "single-file doc without Project.proj must not emit pckg routes: {api}");

    let _ = std::fs::remove_file(source_path);
    let _ = std::fs::remove_file(out_path.join("api.json"));
    let _ = std::fs::remove_file(out_path.join("index.md"));
    let _ = std::fs::remove_dir(&out_path);
    let _ = std::fs::remove_dir(&root);
}
