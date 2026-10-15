use super::{parse_manifest, parse_workspace_manifest};
use crate::projects::error::ProjectError;
use crate::projects::model::{
    DependencySource, ProjectGlueBackend, ProjectGlueOwner, TargetKind, project_root_block_matches_package_name,
};
use crate::projects::validator::{GLUE_LIBRARY_LABEL_MAX_LEN, validate_manifest};

fn minimal_project(kind: &str, source_field: &str) -> String {
    format!(
        r#"p {{
  name = "p"
  version = "0.1.0"
}}
target "t" {{
  kind = {kind}
  entry = "Main.bd"
}}
dependency "d" {{
  source = {source_field}
  path = "../x"
}}
"#
    )
}

#[test]
fn parse_kind_lib_unquoted() {
    let src = minimal_project("Lib", "path");
    let m = parse_manifest(&src).expect("parse");
    assert_eq!(m.targets[0].kind, TargetKind::Lib);
    assert_eq!(m.dependencies[0].source, DependencySource::Path);
}

#[test]
fn parse_kind_and_source_quoted_legacy() {
    let src = minimal_project("\"Lib\"", "\"path\"");
    let m = parse_manifest(&src).expect("parse");
    assert_eq!(m.targets[0].kind, TargetKind::Lib);
    assert_eq!(m.dependencies[0].source, DependencySource::Path);
}

#[test]
fn name_must_stay_quoted() {
    let src = r#"MyApp {
  name = MyApp
  version = "0.1.0"
}
target "t" { kind = Lib entry = "e.bd" }
"#;
    let err = parse_manifest(src).expect_err("name unquoted");
    assert!(matches!(err, ProjectError::ParseAt { .. }));
}

#[test]
fn hyphenated_package_name_accepts_its_exact_underscore_root_projection() {
    let src = r#"beskid_runtime_native {
  name = "beskid-runtime-native"
  version = "0.1.0"
}
target "native" {
  kind = Lib
}
"#;

    let manifest = parse_manifest(src).expect("hyphenated package root projection must parse");
    assert_eq!(manifest.project.block_kind, "beskid_runtime_native");
    assert_eq!(manifest.project.name, "beskid-runtime-native");
    validate_manifest(&manifest).expect("validator must accept the canonical root projection");
}

#[test]
fn project_root_rejects_noncanonical_package_name_mismatch() {
    let src = r#"beskid_runtime {
  name = "beskid-runtime-native"
  version = "0.1.0"
}
target "native" {
  kind = Lib
}
"#;

    let err = parse_manifest(src).expect_err("noncanonical project root must fail");
    match err {
        ProjectError::MetaContractViolation { code, .. } => assert_eq!(code, "E1896"),
        other => panic!("expected MetaContractViolation E1896, got {other:?}"),
    }
}

#[test]
fn validator_rejects_noncanonical_package_name_mismatch() {
    let src = r#"beskid_runtime_native {
  name = "beskid-runtime-native"
  version = "0.1.0"
}
target "native" {
  kind = Lib
}
"#;

    let mut manifest = parse_manifest(src).expect("canonical package root must parse");
    manifest.project.block_kind = "beskid_runtime".to_string();

    let err = validate_manifest(&manifest).expect_err("validator must reject a noncanonical root");
    match err {
        ProjectError::MetaContractViolation { code, .. } => assert_eq!(code, "E1896"),
        other => panic!("expected MetaContractViolation E1896, got {other:?}"),
    }
}

#[test]
fn package_root_projection_only_maps_hyphens_to_underscores() {
    assert!(project_root_block_matches_package_name("beskid_runtime_native", "beskid-runtime-native"));
    assert!(project_root_block_matches_package_name("beskid_runtime_native", "beskid_runtime_native"));
    assert!(!project_root_block_matches_package_name("beskid-runtime-native", "beskid_runtime_native"));
    assert!(!project_root_block_matches_package_name("beskid_runtime", "beskid-runtime-native"));
}

#[test]
fn invalid_kind_reports_validation() {
    let src = minimal_project("Blob", "path");
    let err = parse_manifest(&src).expect_err("bad kind");
    assert!(matches!(err, ProjectError::ParseAt { .. }));
}

#[test]
fn parse_link_block_libraries_and_paths() {
    let src = r#"p {
  name = "p"
  version = "0.1.0"
}

target "t" {
  kind = App
  entry = "Main.bd"
}

link {
  libraries = [libc, pthread]
  searchPaths = ["/usr/lib", "/opt/local/lib"]
  extraArgs = ["-lm"]
}
"#;
    let m = parse_manifest(src).expect("parse link block");
    let link = m.link.expect("link section present");
    assert_eq!(link.libraries, vec!["libc", "pthread"]);
    assert_eq!(link.search_paths, vec!["/usr/lib", "/opt/local/lib"]);
    assert_eq!(link.extra_args, vec!["-lm"]);
}

#[test]
fn parse_link_block_unknown_key_rejected() {
    let src = r#"p {
  name = "p"
  version = "0.1.0"
}
target "t" {
  kind = App
  entry = "Main.bd"
}
link {
  bogus = [libc]
}
"#;
    let err = parse_manifest(src).expect_err("unknown link key must error");
    assert!(matches!(err, ProjectError::ParseAt { .. }));
}

#[test]
fn parse_link_block_duplicate_library_rejected() {
    let src = r#"p {
  name = "p"
  version = "0.1.0"
}
target "t" {
  kind = App
  entry = "Main.bd"
}
link {
  libraries = [libc, libc]
}
"#;
    let err = parse_manifest(src).expect_err("duplicate library must error");
    match err {
        ProjectError::MetaContractViolation { code, .. } => assert_eq!(code, "E1893"),
        other => panic!("expected MetaContractViolation E1893, got {other:?}"),
    }
}

#[test]
fn parse_link_block_absent_yields_none() {
    let src = r#"p {
  name = "p"
  version = "0.1.0"
}
target "t" {
  kind = App
  entry = "Main.bd"
}
"#;
    let m = parse_manifest(src).expect("parse without link");
    assert!(m.link.is_none());
}

#[test]
fn workspace_resolver_unquoted() {
    let src = r#"workspace {
  name = "w"
  resolver = v1
}
member "m" {
  path = "pkg"
}
"#;
    let w = parse_workspace_manifest(src).expect("parse workspace");
    assert_eq!(w.workspace.resolver, "v1");
    assert_eq!(w.workspace.name, "w");
    assert_eq!(w.members[0].path, "pkg");
}

#[test]
fn workspace_default_test_member_lands_in_extras() {
    let src = r#"workspace {
  name = "corelib"
  resolver = v1
  defaultTestMember = "corelib_tests"
}
member "corelib_tests" {
  path = "tests/corelib_tests"
}
"#;
    let w = parse_workspace_manifest(src).expect("parse workspace");
    assert_eq!(w.workspace.extras.get("defaultTestMember").map(String::as_str), Some("corelib_tests"));
}

#[test]
fn parse_grammar_block_with_outputs() {
    let src = r#"foundation {
  name = "foundation"
  version = "0.1.0"
  root = "src"
  grammar {
    roots = [grammars]
    grammarOutput {
      pest = "grammars/regex.pest"
      module = "Core.Text.Regex.Generated"
      packageId = "corelib_foundation"
    }
  }
}
target "lib" {
  kind = Lib
}
"#;
    let manifest = parse_manifest(src).expect("parse grammar manifest");
    let grammar = manifest.project.grammar_section.expect("grammar section");
    assert_eq!(grammar.roots, vec!["grammars"]);
    assert_eq!(grammar.grammar_outputs.len(), 1);
    assert_eq!(grammar.grammar_outputs[0].pest, "grammars/regex.pest");
    assert_eq!(grammar.grammar_outputs[0].module, "Core.Text.Regex.Generated");
    assert_eq!(grammar.grammar_outputs[0].package_id, "corelib_foundation");
}

#[test]
fn parse_mod_generated_output_blocks() {
    let src = r#"my_mod {
  name = "my_mod"
  version = "0.1.0"
  type = Mod
  mod {
    generatedOutput {
      layout = "generate.layout.json"
      root = "Generated"
    }
  }
}
"#;
    let manifest = parse_manifest(src).expect("parse mod manifest");
    let mod_section = manifest.project.mod_section.expect("mod section");
    let outputs = mod_section.generated_outputs.expect("generated outputs");
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].layout, "generate.layout.json");
    assert_eq!(outputs[0].resolved_root(), "Generated");
}

fn glue_project(glue: &str) -> String {
    format!(
        r#"p {{
  name = "p"
  version = "0.1.0"
}}
target "consumer" {{
  kind = Lib
}}
{glue}"#
    )
}

#[track_caller]
fn expect_meta_code(src: &str, expected: &str, needle: &str) {
    match parse_manifest(src).expect_err("manifest must be rejected") {
        ProjectError::MetaContractViolation { code, message } => {
            assert_eq!(code, expected, "{message}");
            assert!(message.contains(needle), "message `{message}` does not name `{needle}`");
        }
        other => panic!("expected MetaContractViolation {expected}, got {other:?}"),
    }
}

#[test]
fn parse_glue_owner_block_lowers_to_typed_owner() {
    let src = glue_project("glue \"glue_manual\" {\n  backend = rust\n  path = \"rust\"\n}\n");
    let manifest = parse_manifest(&src).expect("parse");
    assert_eq!(
        manifest.glue,
        vec![ProjectGlueOwner {
            library: "glue_manual".to_string(),
            backend: ProjectGlueBackend::Rust,
            path: "rust".to_string(),
        }]
    );
}

#[test]
fn parse_multiple_glue_owner_blocks_keep_manifest_order() {
    let src = glue_project(
        "glue \"zeta-owner\" { backend = rust path = \"zeta\" }\nglue \"Alpha_1\" { backend = \"rust\" path = \"owners/alpha\" }\n",
    );
    let manifest = parse_manifest(&src).expect("parse");
    let libraries: Vec<&str> = manifest.glue.iter().map(|owner| owner.library.as_str()).collect();
    assert_eq!(libraries, ["zeta-owner", "Alpha_1"]);
    assert_eq!(manifest.glue[1].path, "owners/alpha");
}

#[test]
fn manifest_without_glue_blocks_has_no_glue_owners() {
    let manifest = parse_manifest(&glue_project("")).expect("parse");
    assert!(manifest.glue.is_empty());
}

#[test]
fn glue_label_accepts_the_full_charset_and_length_bound() {
    let label = format!("a-Z_9{}", "x".repeat(GLUE_LIBRARY_LABEL_MAX_LEN - 5));
    assert_eq!(label.len(), GLUE_LIBRARY_LABEL_MAX_LEN);
    let src = glue_project(&format!("glue \"{label}\" {{ backend = rust path = \"rust\" }}\n"));
    assert_eq!(parse_manifest(&src).expect("parse").glue[0].library, label);
}

#[test]
fn glue_unknown_and_tool_path_keys_are_rejected() {
    for key in ["cargo", "rustc", "linker", "rustToolchain", "crate"] {
        let src = glue_project(&format!("glue \"g\" {{ backend = rust path = \"rust\" {key} = \"/usr/bin/x\" }}\n"));
        let err = parse_manifest(&src).expect_err("unknown key must be rejected");
        assert!(err.to_string().contains(key), "{key}: {err}");
    }
}

#[test]
fn glue_missing_label_backend_or_path_is_rejected() {
    for glue in [
        "glue { backend = rust path = \"rust\" }\n",
        "glue \"g\" { path = \"rust\" }\n",
        "glue \"g\" { backend = rust }\n",
        "glue \"g\" { backend = python path = \"rust\" }\n",
    ] {
        assert!(parse_manifest(&glue_project(glue)).is_err(), "accepted: {glue}");
    }
}

#[test]
fn glue_duplicate_label_is_rejected() {
    let src = glue_project(
        "glue \"glue_manual\" { backend = rust path = \"a\" }\nglue \"glue_manual\" { backend = rust path = \"b\" }\n",
    );
    expect_meta_code(&src, "E1861", "glue_manual");
}

#[test]
fn glue_invalid_label_characters_or_length_are_rejected() {
    let too_long = "x".repeat(GLUE_LIBRARY_LABEL_MAX_LEN + 1);
    for label in ["", "glue.manual", "glue manual", "glue/manual", "gl\u{fc}e", too_long.as_str()] {
        let src = glue_project(&format!("glue \"{label}\" {{ backend = rust path = \"rust\" }}\n"));
        expect_meta_code(&src, "E1860", "glue label");
    }
}

#[test]
fn glue_dotnet_backend_is_unavailable() {
    let src = glue_project("glue \"net_owner\" { backend = dotnet path = \"dotnet\" }\n");
    expect_meta_code(&src, "E1862", "net_owner");
}

#[test]
fn glue_escaping_or_absolute_path_is_rejected() {
    for path in ["../outside", "rust/../../outside", "/abs/rust", ""] {
        let src = glue_project(&format!("glue \"g\" {{ backend = rust path = \"{path}\" }}\n"));
        expect_meta_code(&src, "E1863", "glue `g` path");
    }
}

#[test]
fn glue_label_also_in_link_libraries_is_rejected() {
    let src = glue_project(
        "link {\n  libraries = [\"libc\", \"glue_manual\"]\n}\nglue \"glue_manual\" { backend = rust path = \"rust\" }\n",
    );
    expect_meta_code(&src, "E1864", "glue_manual");
}

#[test]
fn validator_rejects_glue_owner_built_without_parser() {
    let mut manifest = parse_manifest(&glue_project("")).expect("parse");
    manifest.glue.push(ProjectGlueOwner {
        library: "owner".to_string(),
        backend: ProjectGlueBackend::Dotnet,
        path: "rust".to_string(),
    });
    match validate_manifest(&manifest).expect_err("dotnet owner must be rejected") {
        ProjectError::MetaContractViolation { code, .. } => assert_eq!(code, "E1862"),
        other => panic!("expected E1862, got {other:?}"),
    }
}
