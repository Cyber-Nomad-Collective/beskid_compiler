//! Array literals and nested call temporaries used directly as call arguments.
//!
//! The Serialization Mod's `Receiver` builds a typed path argument list in one expression:
//! `Array.Append<Item>(arguments, Named(builder, [parameters[index].node.name], []))`. Every
//! operand of that expression is a call argument: an empty literal typed by the callee
//! parameter, a one-element literal whose element is a nominal field projection, and a nested
//! direct call used as the appended value.
use super::support::{
    SyntaxGenerationId, SyntaxModuleItem, canonical_corelib_project_fixture, find_function_definitions,
    find_nodes_of_kind, lower_syntax_program,
};
use beskid_queries::{IndexedNodeKind, SemanticTypeId, empty_array_literal_element_abi_type};

const RECEIVER_SOURCE: &str = r#"
use Core.Collections.Array;
type Builder {}
type Name { string name }
type Node { Name node }
type Item {}
Item Named(Builder builder, string[] names, Item[] arguments) { return Item {}; }
Item[] Receiver(Builder builder, Node[] parameters) {
    mut Item[] arguments = [];
    mut i64 index = 0_i64;
    while (index < Array.Len(parameters)) { arguments = Array.Append<Item>(arguments, Named(builder, [parameters[index].node.name], [])); index = index + 1_i64; }
    return arguments;
}
"#;

#[test]
fn nested_call_argument_array_literals_lower_in_one_expression() {
    let (input, isa, root) =
        canonical_corelib_project_fixture(&[("App/Receiver.bd", RECEIVER_SOURCE)], SyntaxGenerationId(211));
    let functions = find_function_definitions(input.database(), root);
    let named = functions[0];
    let receiver = functions[1];

    let arrays = find_nodes_of_kind(input.database(), receiver, IndexedNodeKind::ArrayLiteralExpression);
    assert_eq!(arrays.len(), 3, "declared local, element literal, empty call argument");
    assert_eq!(
        empty_array_literal_element_abi_type(input.database(), arrays[2]),
        Ok(Some(SemanticTypeId::POINTER)),
        "the empty literal takes `Item[]` from the `Named` parameter it is passed to"
    );
    let element_plan = input.array_static_plan(arrays[1]).expect("`[parameters[index].node.name]` has a static plan");
    assert_eq!(element_plan.element_type, SemanticTypeId::STRING, "projected string field is the element ABI");
    assert!(input.array_static_plan(arrays[2]).is_some(), "empty call-argument literal has a static plan");

    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: named, symbol: "Named".into() },
            SyntaxModuleItem { key: receiver, symbol: "Receiver".into() },
        ],
    )
    .expect("array literals and call temporaries nested as call arguments lower through generated ISLE");
}

const COPY_SOURCE: &str = r#"
use Core.Collections.Array;
type Item {}
pub T[] Copy<T>(T[] values) {
    mut T[] result = Array.Empty<T>();
    mut i64 index = 0_i64;
    while (index < Array.Len<T>(values)) {
        result = Array.Append<T>(result, values[index]);
        index = index + 1_i64;
    }
    return result;
}
Item[] Main(Item[] items) { return Copy<Item>(items); }
"#;

/// `values[index]` inside `Copy<T>` has element type `T`. With `T = Item` the element ABI is
/// a pointer, so only the specialization's source binding can prove the appended value is a
/// traced reference; the pointer ABI alone must not be guessed either way.
#[test]
fn specialized_generic_function_appends_an_indexed_nominal_element() {
    let (input, isa, root) = canonical_corelib_project_fixture(&[("App/Copy.bd", COPY_SOURCE)], SyntaxGenerationId(212));
    let functions = find_function_definitions(input.database(), root);
    let copy = functions[0];
    let main = functions[1];
    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem { key: copy, symbol: "Copy".into() }, SyntaxModuleItem { key: main, symbol: "Main".into() }],
    )
    .expect("Array.Append<T>(result, values[index]) lowers with the Copy<Item> specialization");
}

const BODY_SOURCE: &str = r#"
type Builder {}
type Statement {}
Statement Let(Builder builder, string name) { return Statement {}; }
Statement[] Body(Builder builder, Statement begin, Statement finish) {
    return [Let(builder, "reader"), begin, Let(builder, "value"), finish];
}
"#;

/// `[Let(builder, "reader"), begin, ...]` mixes direct-call results (proven through
/// `node_type`) with nominal locals (proven only through their value ABI). Both shapes name the
/// same managed element representation, so the literal has one exact static plan.
#[test]
fn array_literal_mixing_call_results_and_nominal_locals_lowers() {
    let (input, isa, root) = canonical_corelib_project_fixture(&[("App/Body.bd", BODY_SOURCE)], SyntaxGenerationId(214));
    let functions = find_function_definitions(input.database(), root);
    let let_item = functions[0];
    let body = functions[1];
    let arrays = find_nodes_of_kind(input.database(), body, IndexedNodeKind::ArrayLiteralExpression);
    assert_eq!(arrays.len(), 1);
    let plan = input.array_static_plan(arrays[0]).expect("mixed call/local nominal elements have a static plan");
    assert_eq!(plan.element_type, SemanticTypeId::POINTER);
    assert!(!plan.pointer_map_offsets.is_empty(), "the managed element descriptor is traced");
    assert_eq!(plan.length, 4);
    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem { key: let_item, symbol: "Let".into() }, SyntaxModuleItem { key: body, symbol: "Body".into() }],
    )
    .expect("an array literal of call results and nominal locals lowers through generated ISLE");
}
