//! v0.3 FFI platform-spec conformance fixtures (analysis phase).
//! Runtime link/export tests may use `#[ignore = "v0.3 FFI impl"]` until drivers land.

use beskid_analysis::analysis::{AnalysisOptions, AnalysisResult, run_rules};
use beskid_analysis::builtin_rules;

use crate::surface::ast::parse_program_ast;

fn run_on(source: &str) -> AnalysisResult {
    let program = parse_program_ast(source);
    run_rules(
        &program.node,
        "ffi_v03_spec.bd",
        source,
        &builtin_rules(),
        AnalysisOptions::default(),
    )
}

fn has_code(result: &AnalysisResult, code: &str) -> bool {
    result
        .diagnostics
        .iter()
        .any(|d| d.code.as_deref() == Some(code))
}

/// Legacy T090x codes until E1520–E1523 migration in diagnostic_kinds.
fn has_extern_abi_error(result: &AnalysisResult) -> bool {
    has_code(result, "E1520") || has_code(result, "T0901")
}

fn has_extern_library_error(result: &AnalysisResult) -> bool {
    has_code(result, "E1521") || has_code(result, "T0902")
}

fn has_extern_param_error(result: &AnalysisResult) -> bool {
    has_code(result, "E1522") || has_code(result, "T0903")
}

#[test]
fn v03_extern_contract_valid_scalars_accepted() {
    let source = r#"
[Extern(Abi: "C", Library: "libc")]
pub contract Libc {
    i64 getpid();
}
"#;
    let result = run_on(source);
    assert!(!has_extern_abi_error(&result));
    assert!(!has_extern_library_error(&result));
    assert!(!has_extern_param_error(&result));
}

#[test]
fn v03_extern_rejects_non_c_abi() {
    let source = r#"
[Extern(Abi: "Rust", Library: "libc")]
pub contract Bad {
    i64 f();
}
"#;
    let result = run_on(source);
    assert!(has_extern_abi_error(&result));
}

#[test]
fn v03_extern_requires_library() {
    let source = r#"
[Extern(Abi: "C")]
pub contract Bad {
    i64 f();
}
"#;
    let result = run_on(source);
    assert!(has_extern_library_error(&result));
}

#[test]
fn v03_extern_rejects_string_param_until_interop_views() {
    let source = r#"
[Extern(Abi: "C", Library: "libc")]
pub contract Bad {
    i64 write(i32 fd, string buf, i64 count);
}
"#;
    let result = run_on(source);
    assert!(has_extern_param_error(&result));
}

#[test]
fn v03_extern_on_mod_emits_e1510() {
    let source = r#"attribute Extern(ContractDeclaration) { Abi: string = "C" } [Extern(Abi: "C")] mod sys.io;"#;
    let result = run_on(source);
    assert!(has_code(&result, "E1510"));
}
