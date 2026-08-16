use std::collections::HashMap;

use crate::resolve::{ModuleGraph, Resolution};
use crate::syntax::{AstNodeId, PrimitiveType, SpanInfo};
use crate::types::{TypeInfo, TypeTable};

use super::super::model::LoweringPrepSurfaces;
use super::PrepWalker;

fn span(start: usize, end: usize) -> SpanInfo {
    SpanInfo { start, end, ..SpanInfo::default() }
}

fn table() -> TypeTable {
    let mut t = TypeTable::new();
    for p in [PrimitiveType::I32, PrimitiveType::I64, PrimitiveType::Unit, PrimitiveType::Never] {
        t.intern(TypeInfo::Primitive(p));
    }
    t
}

#[test]
fn records_numeric_cast() {
    let types = table();
    let i32 = types.find_primitive(PrimitiveType::I32).unwrap();
    let i64 = types.find_primitive(PrimitiveType::I64).unwrap();
    let surfaces = LoweringPrepSurfaces {
        types: &types,
        local_types: &HashMap::new(),
        function_signatures: &HashMap::new(),
        method_function_signatures: &HashMap::new(),
        struct_fields_ordered: &HashMap::new(),
        struct_event_fields: &HashMap::new(),
        enum_variants_ordered: &HashMap::new(),
        generic_items: &HashMap::new(),
        methods_by_receiver: &HashMap::new(),
        contract_signatures: &HashMap::new(),
        named_types: &HashMap::new(),
    };
    let resolution = Resolution {
        items: Vec::new(),
        module_graph: ModuleGraph::default(),
        tables: crate::resolve::ResolutionTables::new(),
        span_index: Default::default(),
        warnings: Vec::new(),
        builtin_items: HashMap::new(),
        module_imports: HashMap::new(),
        symbols: crate::resolve::SymbolRegistry::default(),
        by_symbol: HashMap::new(),
    };
    let node_types = HashMap::new();
    let mut w = PrepWalker::new(&resolution, &node_types, &surfaces);
    w.record_numeric_cast(AstNodeId(1), span(0, 1), i64, i32);
    assert_eq!(w.prep.cast_intents.len(), 1);
}
