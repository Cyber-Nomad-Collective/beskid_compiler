use beskid_analysis::hir::{AstProgram, HirProgram, lower_program, normalize_program};
use beskid_analysis::resolve::{ResolveError, ResolveWarning, Resolver};
use beskid_analysis::syntax::Spanned;

use crate::syntax::util::parse_program_ast;

fn resolve_program(
    source: &str,
) -> Result<beskid_analysis::resolve::Resolution, Vec<ResolveError>> {
    let program = parse_program_ast(source);
    let ast: Spanned<AstProgram> = program.into();
    let mut hir: Spanned<HirProgram> = lower_program(&ast);
    normalize_program(&mut hir).expect("normalization failed");
    Resolver::new().resolve_program(&hir)
}

#[test]
fn duplicate_top_level_item_is_error() {
    let result = resolve_program("unit foo() { } unit foo() { }");
    let errors = result.expect_err("expected duplicate item error");
    assert!(matches!(
        errors.first(),
        Some(ResolveError::DuplicateItem { .. })
    ));
}

#[test]
fn duplicate_local_is_error() {
    let result = resolve_program("unit foo() { let x = 1; let x = 2; }");
    let errors = result.expect_err("expected duplicate local error");
    assert!(matches!(
        errors.first(),
        Some(ResolveError::DuplicateLocal { .. })
    ));
}

#[test]
fn unknown_value_is_error() {
    let result = resolve_program("unit foo() { let x = y; }");
    let errors = result.expect_err("expected unknown value error");
    assert!(matches!(
        errors.first(),
        Some(ResolveError::UnknownValue { .. })
    ));
}

#[test]
fn unknown_type_is_error() {
    let result = resolve_program("unit foo(Missing x) { }");
    let errors = result.expect_err("expected unknown type error");
    assert!(matches!(
        errors.first(),
        Some(ResolveError::UnknownType { .. })
    ));
}

#[test]
fn shadowing_local_emits_warning() {
    let result = resolve_program("unit foo() { let x = 1; if true { let x = 2; } }")
        .expect("expected successful resolution");
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| matches!(warning, ResolveWarning::ShadowedLocal { .. }))
    );
}

#[test]
fn shadowing_item_with_local_emits_warning() {
    let result = resolve_program("unit x() { } unit foo() { let x = 1; }")
        .expect("expected successful resolution");
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| matches!(warning, ResolveWarning::ShadowedLocal { .. }))
    );
}

#[test]
fn qualified_value_path_with_missing_module_is_error() {
    let result = resolve_program("unit foo() { let x = dep.thing; }");
    let errors = result.expect_err("expected unknown module path error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ResolveError::UnknownModulePath { .. }))
    );
}

#[test]
fn qualified_value_path_with_known_module_and_missing_symbol_is_error() {
    let result = resolve_program("mod dep; unit foo() { let x = thing; }");
    let errors = result.expect_err("expected unknown value in module error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ResolveError::UnknownValue { .. }))
    );
}

#[test]
fn qualified_type_path_with_known_module_and_missing_symbol_is_error() {
    let result = resolve_program("mod dep; unit foo(Missing x) { }");
    let errors = result.expect_err("expected unknown type in module error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ResolveError::UnknownType { .. }))
    );
}

#[test]
fn qualified_module_path_to_private_item_is_error() {
    let result =
        resolve_program("mod dep; type secret { i32 value } unit foo() { let x = dep.secret; }");
    let errors = result.expect_err("expected private item in module error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ResolveError::PrivateItemInModule { .. }))
    );
}

#[test]
fn non_contract_conformance_target_is_error() {
    let result = resolve_program("type NotContract { i64 id } type User : NotContract { i64 x }");
    let errors = result.expect_err("expected invalid conformance target error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ResolveError::InvalidConformanceTarget { .. })),
        "expected InvalidConformanceTarget error, got: {errors:?}"
    );
}

#[test]
fn qualified_module_path_to_public_item_is_allowed() {
    let result = resolve_program(
        "mod dep; pub type secret { i32 value } unit foo() { let x = dep.secret; }",
    );
    assert!(
        result.is_ok(),
        "expected qualified access to public module item to resolve"
    );
}

#[test]
fn syscall_write_builtin_resolves() {
    let result = resolve_program("i64 main() { return __syscall_write(1, \"hi\"); }")
        .expect("expected __syscall_write to resolve");
    assert!(result.warnings.is_empty());
}

#[test]
fn syscall_read_builtin_resolves() {
    let result = resolve_program("string main() { return __syscall_read(0, 16); }")
        .expect("expected __syscall_read to resolve");
    assert!(result.warnings.is_empty());
}

#[test]
fn stdstring_len_resolves() {
    let result = resolve_program("i64 main() { return __str_len(\"hello\"); }")
        .expect("expected direct str_len path to resolve");
    assert!(result.warnings.is_empty());
}

#[test]
fn std_panic_resolves() {
    let result = resolve_program("unit main() { __panic_str(\"boom\"); }")
        .expect("expected __panic_str to resolve");
    assert!(result.warnings.is_empty());
}

#[test]
fn stdarray_new_resolves() {
    let result = resolve_program("i64 main() { return __array_new(8, 2); }")
        .expect("expected __array_new to resolve");
    assert!(result.warnings.is_empty());
}

#[test]
fn qualified_nested_public_module_path_is_allowed() {
    let result = resolve_program(
        "mod dep.api; pub type v1 { i32 value } unit main() { let x = dep.api.v1; }",
    );
    assert!(
        result.is_ok(),
        "expected qualified access to nested public module item to resolve"
    );
}

#[test]
fn qualified_nested_private_module_path_is_error() {
    let result = resolve_program(
        "mod dep.api; type secret { i32 value } unit main() { let x = dep.api.secret; }",
    );
    let errors = result.expect_err("expected private nested module item error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, ResolveError::PrivateItemInModule { .. }))
    );
}

#[test]
fn aliased_import_name_resolves_in_value_path() {
    let result = resolve_program(
        "mod dep; pub type Parser { i32 value } use dep.Parser as DepParser; unit main() { let x = DepParser; }",
    );
    assert!(
        result.is_ok(),
        "expected aliased import to resolve as value"
    );
}
