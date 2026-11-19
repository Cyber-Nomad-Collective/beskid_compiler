use super::support::{
    NodeFacts, NodeKind, emit_isle_item, find_function_definitions, find_nodes_of_kind, item_fixture,
    item_fixture_with_root,
};
use beskid_isle::ManagedReferenceFact;

#[test]
fn adapter_preserves_source_managedness_for_pointer_shaped_parameters() {
    let (input, _isa, item) = item_fixture("i64 Main(string text, pointer native) { return 0_i64; }");
    let parameters = find_nodes_of_kind(input.database(), item, NodeKind::Parameter);
    assert_eq!(parameters.len(), 2, "expected the two source parameters");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);

    assert_eq!(facts.managed_reference(parameters[0]), Some(ManagedReferenceFact::GcManaged));
    assert_eq!(facts.managed_reference(parameters[1]), Some(ManagedReferenceFact::NativeOrScalar));
}

#[test]
fn adapter_classifies_import_resolved_integer_constant_paths_as_scalar() {
    let (input, _isa, item) = item_fixture("const OFFSET = 8; word Main() { return OFFSET; }");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    let constant = find_nodes_of_kind(input.database(), item, NodeKind::PathExpression)
        .into_iter()
        .find(|key| facts.constant_integer(*key) == Some(8))
        .expect("resolved constant path");

    assert_eq!(facts.managed_reference(constant), Some(ManagedReferenceFact::NativeOrScalar));
}

#[test]
fn explicitly_declared_native_pointer_assignment_remains_unrooted() {
    let (input, isa, item) =
        item_fixture("pointer Main(pointer value) { mut pointer cursor = value; cursor = value; return cursor; }");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    for path in find_nodes_of_kind(input.database(), item, NodeKind::PathExpression) {
        assert_eq!(
            facts.managed_reference(path),
            Some(ManagedReferenceFact::NativeOrScalar),
            "declared native pointer path must not become a GC root"
        );
    }

    let function = emit_isle_item(&input, isa.as_ref(), item).unwrap_or_else(|error| {
        panic!("native pointer assignment lowering: {}", error.display_with_db(input.database()))
    });
    let clif = function.display().to_string();
    assert!(!clif.contains("gc_register_root"), "{clif}");
    assert!(!clif.contains("gc_unregister_root"), "{clif}");
}

#[test]
fn scalar_binary_assignment_with_constant_path_remains_unrooted() {
    let (input, isa, item) = item_fixture(
        "const LIMIT = 64; word Main(word head) { mut word cursor = head; cursor = (cursor + 1) % LIMIT; return cursor; }",
    );
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    let binary = find_nodes_of_kind(input.database(), item, NodeKind::BinaryExpression)
        .into_iter()
        .last()
        .expect("outer modulo expression");
    assert_eq!(facts.managed_reference(binary), Some(ManagedReferenceFact::NativeOrScalar));

    let function = emit_isle_item(&input, isa.as_ref(), item).unwrap_or_else(|error| {
        panic!("scalar binary assignment lowering: {}", error.display_with_db(input.database()))
    });
    let clif = function.display().to_string();
    assert!(!clif.contains("gc_register_root"), "{clif}");
}

#[test]
fn adapter_classifies_declared_arrays_and_string_call_results_as_managed() {
    let (input, _isa, root) = item_fixture_with_root(
        r#"
string Echo(string value) { return value; }
i64 Main() {
    i64[] values = [];
    string text = Echo("ok");
    return 0_i64;
}
"#,
    );
    let item = find_function_definitions(input.database(), root)[1];
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    for declaration in find_nodes_of_kind(input.database(), item, NodeKind::LetStatement) {
        assert_eq!(
            facts.managed_reference(declaration),
            Some(ManagedReferenceFact::GcManaged),
            "declared array/string local must remain managed"
        );
    }
    for call in find_nodes_of_kind(input.database(), item, NodeKind::CallExpression) {
        assert_eq!(facts.managed_reference(call), Some(ManagedReferenceFact::GcManaged));
    }
}

#[test]
fn loop_safepoint_keeps_only_the_reassigned_value() {
    use cranelift_codegen::ir::{InstructionData, ValueDef};

    let (input, isa, item) = item_fixture(
        "string Main(string first, string second, bool repeat) { mut string current = first; current = second; while repeat { break; } return current; }",
    );
    let function = emit_isle_item(&input, isa.as_ref(), item).expect("loop with reassigned root lowers");
    let entry = function.layout.entry_block().expect("entry");
    let [_, second, _] = function.dfg.block_params(entry) else { panic!("expected three parameters") };
    let roots = function
        .layout
        .blocks()
        .flat_map(|block| function.layout.block_insts(block))
        .find_map(|inst| function.dfg.user_stack_map_entries(inst))
        .expect("the polling call needs a map");
    let [root] = roots else { panic!("only the current value is live: {}", function.display()) };
    assert!(function.layout.block_insts(entry).any(|inst| {
        if let InstructionData::Store { args: [value, address], .. } = function.dfg.insts[inst]
            && let ValueDef::Result(address, _) = function.dfg.value_def(address)
            && let InstructionData::StackAddr { stack_slot, .. } = function.dfg.insts[address]
        {
            value == *second && stack_slot == root.slot
        } else {
            false
        }
    }));
}

#[test]
fn branch_polls_map_only_values_live_on_that_path() {
    use cranelift_codegen::ir::{ExternalName, InstructionData};

    let (input, isa, item) = item_fixture(
        "type Node { i64 value } i64 Main(Node live, bool branch, bool repeat) { if branch { while repeat { break; } return live.value; } while repeat { break; } return 0_i64; }",
    );
    let function = emit_isle_item(&input, isa.as_ref(), item).expect("lower branch-specific polls");
    let poll = ExternalName::testcase("gc_collect_if_needed");
    let mut counts = Vec::new();
    for inst in function.layout.blocks().flat_map(|block| function.layout.block_insts(block)) {
        if let InstructionData::Call { func_ref, .. } = function.dfg.insts[inst]
            && function.dfg.ext_funcs[func_ref].name == poll
        {
            counts.push(function.dfg.user_stack_map_entries(inst).map_or(0, |entries| entries.len()));
        }
    }
    counts.sort_unstable();
    assert_eq!(counts, [0, 1], "{}", function.display());
}
