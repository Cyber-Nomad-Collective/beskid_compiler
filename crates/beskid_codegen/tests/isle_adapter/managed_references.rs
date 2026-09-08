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
fn managed_parameter_emits_a_root_for_its_full_function_lifetime() {
    let (input, isa, item) = item_fixture("i64 Main(string value) { return 0_i64; }");
    let function = emit_isle_item(&input, isa.as_ref(), item)
        .unwrap_or_else(|error| panic!("rooted parameter lowering: {}", error.display_with_db(input.database())));
    let clif = function.display().to_string();

    let register = clif.find("gc_register_root").expect("local root registration");
    let unregister = clif.find("gc_unregister_root").expect("local root cleanup");
    let return_instruction = clif.rfind("return").expect("function return");
    assert_eq!(clif.matches("gc_register_root").count(), 1, "{clif}");
    assert_eq!(clif.matches("gc_unregister_root").count(), 1, "{clif}");
    assert!(register < unregister && unregister < return_instruction, "{clif}");
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
fn managed_local_reassignment_updates_its_existing_root_slot() {
    let (input, isa, item) = item_fixture(
        "string Main(string first, string second) { mut string value = first; value = second; return value; }",
    );
    let function = emit_isle_item(&input, isa.as_ref(), item).unwrap_or_else(|error| {
        panic!("managed local reassignment lowering: {}", error.display_with_db(input.database()))
    });
    let clif = function.display().to_string();

    assert_eq!(clif.matches("gc_register_root").count(), 3, "{clif}");
    assert_eq!(clif.matches("gc_unregister_root").count(), 3, "{clif}");
    assert_eq!(clif.matches("stack_store").count(), 4, "local reassignment must refresh its root slot:\n{clif}");
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
fn conditional_branch_roots_end_in_the_branch_without_polluting_an_alternate_return() {
    let (input, isa, item) = item_fixture(
        "unit Main(bool condition, string value) { if condition { string branch = value; } else { return; } return; }",
    );
    let function = emit_isle_item(&input, isa.as_ref(), item)
        .unwrap_or_else(|error| panic!("branch-local root lowering: {}", error.display_with_db(input.database())));
    let clif = function.display().to_string();

    assert_eq!(clif.matches("gc_register_root").count(), 2, "parameter plus branch local:\n{clif}");
    assert_eq!(
        clif.matches("gc_unregister_root").count(),
        3,
        "branch local exits on branch fallthrough; the two returns clean only the parameter:\n{clif}"
    );
}

#[test]
fn nested_block_root_ends_before_following_statements() {
    let (input, isa, item) =
        item_fixture("unit Main(string value) { { string scoped = value; } i64 marker = 1_i64; marker; return; }");
    let function = emit_isle_item(&input, isa.as_ref(), item)
        .unwrap_or_else(|error| panic!("nested-block root lowering: {}", error.display_with_db(input.database())));
    let clif = function.display().to_string();

    let scoped_cleanup = clif.find("gc_unregister_root").expect("nested block cleanup");
    let marker_update = clif.rfind("iconst.i64 1").expect("statement following nested block");
    assert!(scoped_cleanup < marker_update, "the nested root must end before the next statement:\n{clif}");
}

#[test]
fn effect_position_block_root_ends_before_following_statements() {
    let (input, isa, item) =
        item_fixture("unit Main(string value) { { string scoped = value; }; i64 marker = 1_i64; marker; return; }");
    let function = emit_isle_item(&input, isa.as_ref(), item).unwrap_or_else(|error| {
        panic!("effect-position block root lowering: {}", error.display_with_db(input.database()))
    });
    let clif = function.display().to_string();

    let scoped_cleanup = clif.find("gc_unregister_root").expect("effect-position block cleanup");
    let marker = clif.rfind("iconst.i64 1").expect("statement following effect-position block");
    assert!(scoped_cleanup < marker, "the effect-position block root must end at its own boundary:\n{clif}");
}

#[test]
fn managed_match_binding_does_not_pollute_an_alternate_return() {
    let (input, isa, item) = item_fixture(
        "enum Choice { Text(string text), Empty } unit Main(Choice choice) { match choice { Choice::Text(text) => { text; }, Choice::Empty => { return; }, }; return; }",
    );
    let function = emit_isle_item(&input, isa.as_ref(), item)
        .unwrap_or_else(|error| panic!("match-binding root lowering: {}", error.display_with_db(input.database())));
    let clif = function.display().to_string();

    assert_eq!(clif.matches("gc_register_root").count(), 2, "parameter plus Text binding:\n{clif}");
    assert_eq!(
        clif.matches("gc_unregister_root").count(),
        3,
        "Text binding exits in its arm; the alternate and final returns clean only the parameter:\n{clif}"
    );
}

#[test]
fn array_match_binding_is_rooted_for_its_lexical_arm() {
    let (input, isa, item) = item_fixture(
        "enum Choice { Values(i64[] values), Empty } unit Main(Choice choice) { match choice { Choice::Values(values) => { values; }, Choice::Empty => {}, }; return; }",
    );
    let function = emit_isle_item(&input, isa.as_ref(), item).unwrap_or_else(|error| {
        panic!("array match-binding root lowering: {}", error.display_with_db(input.database()))
    });
    let clif = function.display().to_string();

    assert_eq!(clif.matches("gc_register_root").count(), 2, "parameter plus array binding:\n{clif}");
    assert_eq!(
        clif.matches("gc_unregister_root").count(),
        2,
        "array binding exits in its arm and the parameter exits at return:\n{clif}"
    );
}
