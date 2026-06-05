use std::collections::HashMap;

use beskid_analysis::resolve::{ModuleGraph, Resolution};
use beskid_analysis::syntax::SpanInfo;
use beskid_analysis::types::{TypeResult, TypeTable};
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

#[test]
fn dynamic_ineligible_mapping_maps_to_e2013() {
    let span = SpanInfo {
        start: 0,
        end: 1,
        line_col_start: (1, 1),
        line_col_end: (1, 2),
    };
    let (type_result, resolution) = empty_type_context();
    let error = CodegenError::IneligibleSerializeMapping {
        span,
        src_name: "Source".to_string(),
        dst_name: "Target".to_string(),
    };
    let diagnostic =
        codegen_error_to_diagnostic("test.bd", "x", &error, &type_result, &resolution);

    assert_eq!(diagnostic.code.as_deref(), Some("E2013"));
    assert!(diagnostic.message.contains("Source"));
    assert!(diagnostic.message.contains("Target"));
    assert!(diagnostic.message.contains("Serialization Mod"));
}
