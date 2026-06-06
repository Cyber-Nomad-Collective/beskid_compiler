use std::collections::HashMap;

use beskid_analysis::hir::HirPrimitiveType;
use beskid_analysis::resolve::{ModuleGraph, Resolution};
use beskid_analysis::syntax::SpanInfo;
use beskid_analysis::types::{TypeInfo, TypeResult, TypeTable};
use beskid_codegen::diagnostics::codegen_error_to_diagnostic;
use beskid_codegen::errors::CodegenError;

fn empty_type_context() -> (TypeResult, Resolution) {
    let type_result = TypeResult {
        types: TypeTable::new(),
        named_type_names: HashMap::new(),
        expr_types: HashMap::new(),
        scoped_expr_types: HashMap::new(),
        local_types: HashMap::new(),
        function_signatures: HashMap::new(),
        method_function_signatures: HashMap::new(),
        struct_fields_ordered: HashMap::new(),
        struct_event_fields: HashMap::new(),
        enum_variants_ordered: HashMap::new(),
        generic_items: HashMap::new(),
        call_kinds: HashMap::new(),
        scoped_call_kinds: HashMap::new(),
        contract_method_order: HashMap::new(),
        contract_signatures: HashMap::new(),
        cast_intents: Vec::new(),
    };
    let resolution = Resolution {
        items: Vec::new(),
        module_graph: ModuleGraph::new_root(),
        tables: Default::default(),
        warnings: Vec::new(),
        builtin_items: HashMap::new(),
        module_imports: HashMap::new(),
        symbols: Default::default(),
        by_symbol: HashMap::new(),
    };
    (type_result, resolution)
}

fn type_result_with_i32_i64() -> (
    TypeResult,
    Resolution,
    beskid_analysis::types::TypeId,
    beskid_analysis::types::TypeId,
) {
    let (mut type_result, resolution) = empty_type_context();
    let i32 = type_result
        .types
        .intern(TypeInfo::Primitive(HirPrimitiveType::I32));
    let i64 = type_result
        .types
        .intern(TypeInfo::Primitive(HirPrimitiveType::I64));
    (type_result, resolution, i32, i64)
}

#[test]
fn maps_missing_cast_intent_to_stable_code() {
    let span = SpanInfo {
        start: 1,
        end: 2,
        line_col_start: (1, 2),
        line_col_end: (1, 3),
    };
    let (type_result, resolution, expected, actual) = type_result_with_i32_i64();
    let error = CodegenError::MissingCastIntent {
        span,
        expected,
        actual,
    };
    let diagnostic = codegen_error_to_diagnostic("test.bd", "x", &error, &type_result, &resolution);

    assert_eq!(diagnostic.code.as_deref(), Some("E2008"));
    assert!(diagnostic.message.contains("missing cast intent"));
    assert!(diagnostic.message.contains("i32"));
    assert!(diagnostic.message.contains("i64"));
}

#[test]
fn maps_type_mismatch_to_readable_type_names() {
    let span = SpanInfo {
        start: 1,
        end: 2,
        line_col_start: (1, 2),
        line_col_end: (1, 3),
    };
    let (type_result, resolution, string, i32) = {
        let (mut type_result, resolution) = empty_type_context();
        let string = type_result
            .types
            .intern(TypeInfo::Primitive(HirPrimitiveType::String));
        let i32 = type_result
            .types
            .intern(TypeInfo::Primitive(HirPrimitiveType::I32));
        (type_result, resolution, string, i32)
    };
    let error = CodegenError::TypeMismatch {
        span,
        expected: string,
        actual: i32,
    };
    let diagnostic = codegen_error_to_diagnostic("test.bd", "x", &error, &type_result, &resolution);

    assert_eq!(diagnostic.code.as_deref(), Some("E2010"));
    assert!(diagnostic.message.contains("expected string, actual i32"));
}

#[test]
fn maps_unsupported_node_to_stable_code() {
    let span = SpanInfo {
        start: 0,
        end: 1,
        line_col_start: (1, 1),
        line_col_end: (1, 2),
    };
    let (type_result, resolution) = empty_type_context();
    let error = CodegenError::UnsupportedNode {
        span,
        node: "expression kind",
    };
    let diagnostic = codegen_error_to_diagnostic("test.bd", "x", &error, &type_result, &resolution);

    assert_eq!(diagnostic.code.as_deref(), Some("E2001"));
    assert!(diagnostic.message.contains("unsupported node"));
}
