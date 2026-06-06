//! Integration tests for `beskid_template` instantiation.

use std::fs;
use std::path::PathBuf;

use beskid_template::{
    InstantiateOptions, SymbolCollectOptions, TEMPLATE_MANIFEST_REL, TEMPLATE_SCHEMA, instantiate,
    parse_manifest_bytes, substitute_text,
};

fn workspace_template_packages_root() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest_dir.join("../../beskid_templates/packages");
    if candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
}

fn write_inline_fixture(root: &std::path::Path) -> PathBuf {
    let template_dir = root.join("inline-console");
    fs::create_dir_all(template_dir.join("content")).unwrap();
    let manifest = serde_json::json!({
        "schema": TEMPLATE_SCHEMA,
        "identity": "test.inline.console::1.0.0",
        "name": "Inline Console",
        "shortName": "inline-console",
        "tags": { "type": "project" },
        "sourceName": "MyApp",
        "symbols": {
            "name": { "type": "string", "isRequired": true, "defaultValue": "MyApp" }
        },
        "sources": [{ "source": "./content/", "target": "./" }]
    });
    let manifest_path = template_dir.join(TEMPLATE_MANIFEST_REL);
    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    fs::create_dir_all(template_dir.join("content/Src")).unwrap();
    fs::write(
        template_dir.join("content/Project.proj"),
        r#"project {
  name    = "{{name}}"
  version = "0.1.0"
  root    = "Src"
}

target "app" {
  kind  = App
  entry = "Main.bd"
}
"#,
    )
    .unwrap();
    fs::write(template_dir.join("content/Src/Main.bd"), "// {{name}}\n").unwrap();
    template_dir
}

#[test]
fn parses_beskid_template_v1_manifest() {
    let json = r#"{
        "schema": "beskid.template.v1",
        "identity": "x::1",
        "name": "X",
        "shortName": "x",
        "tags": { "type": "project" }
    }"#;
    let manifest = parse_manifest_bytes(json.as_bytes()).expect("parse");
    assert_eq!(manifest.short_name, "x");
}

#[test]
fn substitutes_placeholders() {
    let mut values = std::collections::BTreeMap::new();
    values.insert("name".to_string(), "Demo".to_string());
    let out = substitute_text("Hello {{name}}", &values);
    assert_eq!(out, "Hello Demo");
}

#[test]
fn instantiates_inline_project_template() {
    let temp = tempfile::tempdir().expect("tempdir");
    let template_root = write_inline_fixture(temp.path());
    let manifest =
        beskid_template::load_manifest_from_template_root(&template_root).expect("manifest");
    let output = temp.path().join("out");
    let options = InstantiateOptions {
        template_root,
        output: output.clone(),
        host_project: None,
        force: false,
        allow_project_manifest: false,
        strict_post_actions: false,
        symbol_options: SymbolCollectOptions {
            interactive: false,
            no_interactive: true,
            primary_name: Some("MyGame".to_string()),
            bindings: Default::default(),
        },
        skip_default_lock: true,
        beskid_exe: None,
    };
    instantiate(&manifest, &options).expect("instantiate");
    let proj = fs::read_to_string(output.join("Project.proj")).expect("read");
    assert!(proj.contains("MyGame"));
    assert!(!proj.contains("{{"));
}

#[test]
fn instantiates_packaged_template_when_present() {
    let Some(packages_root) = workspace_template_packages_root() else {
        return;
    };
    for entry in fs::read_dir(packages_root).expect("read packages") {
        let entry = entry.expect("entry");
        if !entry.file_type().expect("type").is_dir() {
            continue;
        }
        let template_root = entry.path();
        if !template_root.join(TEMPLATE_MANIFEST_REL).is_file() {
            continue;
        }
        let manifest =
            beskid_template::load_manifest_from_template_root(&template_root).expect("manifest");
        let temp = tempfile::tempdir().expect("tempdir");
        let output = temp.path().join("out");
        let options = InstantiateOptions {
            template_root: template_root.clone(),
            output: output.clone(),
            host_project: None,
            force: true,
            allow_project_manifest: false,
            strict_post_actions: false,
            symbol_options: SymbolCollectOptions {
                interactive: false,
                no_interactive: true,
                primary_name: Some("PackTest".to_string()),
                bindings: Default::default(),
            },
            skip_default_lock: true,
            beskid_exe: None,
        };
        instantiate(&manifest, &options).expect("instantiate packaged template");
        assert!(
            output.join("Project.proj").is_file() || output.read_dir().unwrap().next().is_some()
        );
        return;
    }
}
