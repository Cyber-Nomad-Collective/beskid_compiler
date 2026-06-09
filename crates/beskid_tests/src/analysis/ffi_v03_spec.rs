//! v0.3 FFI platform-spec conformance fixtures (analysis phase).
//! Runtime link/export tests may use `#[ignore = "v0.3 FFI impl"]` until drivers land.

use std::collections::HashSet;
use std::fs;

use beskid_analysis::SemanticDiagnostic;
use beskid_analysis::analysis::{AnalysisOptions, AnalysisResult, run_rules};
use beskid_analysis::builtin_rules;
use beskid_analysis::services::{
    FrontEndOptions, PrepareOptions, prepare_compilation_diagnostics,
    resolved_input_from_plan,
};

use crate::projects::with_cwd;
use crate::surface::ast::parse_program_ast;
use crate::test_harness::{temp_case_dir, write_project_manifest as write_manifest};

fn run_semantic_rules(source: &str) -> AnalysisResult {
    let program = parse_program_ast(source);
    run_rules(
        &program.node,
        "ffi_v03_spec.bd",
        source,
        &builtin_rules(),
        AnalysisOptions::default(),
    )
}

fn diagnostic_codes(diagnostics: &[SemanticDiagnostic]) -> HashSet<String> {
    diagnostics
        .iter()
        .filter_map(|diag| diag.code.clone())
        .collect()
}

fn prepare_diagnostic_codes(source: &str) -> HashSet<String> {
    let root = temp_case_dir("ffi_v03_spec");
    let src_dir = root.join("Src");
    fs::create_dir_all(&src_dir).expect("source root");
    write_manifest(
        &root,
        r#"
project {
  name = "FfiV03Spec"
  version = "0.1.0"
}

target "app" {
  kind = App
  entry = "Main.bd"
}
"#,
    );

    let entry = src_dir.join("Main.bd");
    fs::write(&entry, source).expect("write source");

    let codes = with_cwd(&root, || {
        let plan = beskid_analysis::services::compile_plan_for_input_path(&entry)
            .expect("compile plan");
        let resolved = resolved_input_from_plan(
            entry.clone(),
            source.to_string(),
            plan,
            None,
            None,
        );
        let (_, diagnostics) = prepare_compilation_diagnostics(
            &resolved,
            PrepareOptions {
                front_end: FrontEndOptions {
                    with_semantic_diagnostics: true,
                    ..Default::default()
                },
            },
            None,
        )
        .expect("prepare diagnostics");
        diagnostic_codes(&diagnostics)
    });

    let _ = fs::remove_dir_all(root);
    codes
}

fn has_code(codes: &HashSet<String>, code: &str) -> bool {
    codes.contains(code)
}

/// Legacy T090x codes until E1520–E1523 migration in diagnostic_kinds.
fn has_extern_abi_error(codes: &HashSet<String>) -> bool {
    has_code(codes, "E1520") || has_code(codes, "T0901")
}

fn has_extern_library_error(codes: &HashSet<String>) -> bool {
    has_code(codes, "E1521") || has_code(codes, "T0902")
}

fn has_extern_param_error(codes: &HashSet<String>) -> bool {
    has_code(codes, "E1522") || has_code(codes, "T0903")
}

#[test]
fn v03_extern_contract_valid_scalars_accepted() {
    let source = r#"
[Extern(Abi: "C", Library: "libc")]
pub contract Libc {
    i64 getpid();
}
pub unit Main() {}
"#;
    let codes = prepare_diagnostic_codes(source);
    assert!(!has_extern_abi_error(&codes));
    assert!(!has_extern_library_error(&codes));
    assert!(!has_extern_param_error(&codes));
}

#[test]
fn v03_extern_rejects_non_c_abi() {
    let source = r#"
[Extern(Abi: "Rust", Library: "libc")]
pub contract Bad {
    i64 f();
}
pub unit Main() {}
"#;
    let codes = prepare_diagnostic_codes(source);
    assert!(has_extern_abi_error(&codes), "got {codes:?}");
}

#[test]
fn v03_extern_requires_library() {
    let source = r#"
[Extern(Abi: "C")]
pub contract Bad {
    i64 f();
}
pub unit Main() {}
"#;
    let codes = prepare_diagnostic_codes(source);
    assert!(has_extern_library_error(&codes), "got {codes:?}");
}

#[test]
fn v03_extern_rejects_string_param_until_interop_views() {
    let source = r#"
[Extern(Abi: "C", Library: "libc")]
pub contract Bad {
    i64 write(i32 fd, string buf, i64 count);
}
pub unit Main() {}
"#;
    let codes = prepare_diagnostic_codes(source);
    assert!(has_extern_param_error(&codes), "got {codes:?}");
}

#[test]
fn v03_extern_on_mod_emits_e1510() {
    let source = r#"attribute Extern(ContractDeclaration) { Abi: string = "C" } [Extern(Abi: "C")] mod sys.io;"#;
    let result = run_semantic_rules(source);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code.as_deref() == Some("E1510"))
    );
}
