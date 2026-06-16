//! Shared [`TypeResult`] fixtures for codegen and diagnostic tests.

use beskid_analysis::resolve::{ModuleGraph, Resolution};
use beskid_analysis::types::{LoweringPrep, TypeResult, TypeTable};

/// Minimal typed bundle for tests that only need intern table + resolution shell.
pub fn empty_type_result() -> (TypeResult, Resolution) {
    let type_result = TypeResult {
        types: TypeTable::new(),
        named_type_names: std::collections::HashMap::new(),
        node_types: std::collections::HashMap::new(),
        local_types: std::collections::HashMap::new(),
        unit_surfaces: std::collections::HashMap::new(),
        function_signatures: std::collections::HashMap::new(),
        method_function_signatures: std::collections::HashMap::new(),
        struct_fields_ordered: std::collections::HashMap::new(),
        struct_event_fields: std::collections::HashMap::new(),
        enum_variants_ordered: std::collections::HashMap::new(),
        generic_items: std::collections::HashMap::new(),
        lowering: LoweringPrep::default(),
    };
    let resolution = Resolution {
        items: Vec::new(),
        module_graph: ModuleGraph::new_root(),
        tables: Default::default(),
        span_index: Default::default(),
        warnings: Vec::new(),
        builtin_items: std::collections::HashMap::new(),
        module_imports: std::collections::HashMap::new(),
        symbols: Default::default(),
        by_symbol: std::collections::HashMap::new(),
    };
    (type_result, resolution)
}
