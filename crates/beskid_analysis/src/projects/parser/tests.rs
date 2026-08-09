use super::{parse_manifest, parse_workspace_manifest};
use crate::projects::error::ProjectError;
use crate::projects::model::{DependencySource, TargetKind};

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
