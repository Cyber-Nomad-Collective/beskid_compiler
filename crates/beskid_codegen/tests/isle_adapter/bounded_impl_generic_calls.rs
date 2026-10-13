//! Explicit generic calls (`Inner<T, B>(...)`) whose type arguments name the generics of an
//! enclosing bounded `impl<T, B> ... where B: Contract<T>` block, reached through a contract
//! witness (`D: Decoder<T>`), a concrete receiver, or an enclosing generic function. This is the
//! `Core.Bsol.Serialization` `WireDecoder<T, B>.ReadTyped` -> `ReadWithBinding<T, B>` shape: the
//! callee's where-bound witness for `B` must be resolved after the enclosing instance's concrete
//! `B` is substituted, exactly like rustc monomorphizes a nested generic call.

use super::support::{
    AstNodeKey, CodegenInput, SyntaxModuleItem, find_nodes_of_kind, item_fixture_with_root, item_name,
    lower_syntax_program,
};

/// The facts behind the nested call: the enclosing method instance (from the concrete receiver
/// call in `Main`) and the nested `ReadWith<T, B>` call specialized in that environment. Rendered
/// so a lowering failure names the declining query instead of an opaque MissingRuleOrFact.
fn describe_nested_call_facts(source: &str) -> String {
    let (input, _isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let calls = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::CallExpression);
    let mut rendered = Vec::new();
    for call in &calls {
        let site = beskid_queries::format_ast_node_site(db, *call);
        rendered.push(format!("call {site}: lowering={:?}", beskid_queries::call_lowering(db, *call)));
    }
    let enclosing = calls.iter().find_map(|call| {
        beskid_queries::generic_call_specialization(db, *call)
            .ok()
            .flatten()
            .and_then(|specialization| beskid_queries::generic_call_specialization_instance(db, specialization).ok().flatten())
    });
    rendered.push(format!("enclosing instance: {enclosing:#?}"));
    if let Some(enclosing) = enclosing {
        for call in &calls {
            let site = beskid_queries::format_ast_node_site(db, *call);
            rendered.push(format!(
                "in environment {site}: {:#?}",
                beskid_queries::generic_call_specialization_in_environment(db, *call, &enclosing)
            ));
        }
    }
    rendered.join("\n")
}

/// Every method and free function under `roots`, named by its declared name.
fn module_items(input: &CodegenInput<'_>, roots: &[AstNodeKey]) -> Vec<SyntaxModuleItem> {
    let mut items = Vec::new();
    for root in roots {
        for kind in [beskid_queries::IndexedNodeKind::MethodDefinition, beskid_queries::IndexedNodeKind::FunctionDefinition]
        {
            for key in find_nodes_of_kind(input.database(), *root, kind) {
                let name = item_name(input.database(), key).ok().flatten().expect("item name");
                items.push(SyntaxModuleItem { key, symbol: name.to_string() });
            }
        }
    }
    items
}

fn lower_single_unit(source: &str) -> Result<(), String> {
    let (input, isa, root) = item_fixture_with_root(source);
    let items = module_items(&input, &[root]);
    lower_syntax_program(&input, isa.as_ref(), &items).map(|_| ()).map_err(|error| format!("{error:?}"))
}

const SHARED: &str = r#"
contract Reader<T> {
    T Read(i64 seed);
}

contract Decoder<T> {
    T ReadTyped(i64 limit);
}

T ReadWith<T, B>(i64 seed, B binding) where B: Reader<T> {
    return binding.Read(seed);
}

type Dec<T, B> {
    i64 seed,
    B binding,
}

impl<T, B> Dec<T, B> : Decoder<T> where B: Reader<T> {
    pub T ReadTyped(i64 limit) {
        return ReadWith<T, B>(this.seed + limit, this.binding);
    }
}

type IntReader : Reader<i64> {
    pub i64 Read(i64 seed) {
        return seed * 2_i64;
    }
}
"#;

#[test]
fn bounded_impl_method_calls_a_where_bound_generic_through_a_contract_witness() {
    let source = format!(
        "{SHARED}
T Decode<T, D>(D decoder, i64 limit) where D: Decoder<T> {{
    return decoder.ReadTyped(limit);
}}

i64 Main() {{
    Dec<i64, IntReader> decoder = Dec<i64, IntReader> {{ seed: 3_i64, binding: IntReader {{ }} }};
    return Decode<i64, Dec<i64, IntReader>>(decoder, 4_i64);
}}
"
    );
    lower_single_unit(&source).expect(
        "a where-bound generic call from a bounded impl method, dispatched through a contract witness, lowers",
    );
}

#[test]
fn bounded_impl_method_calls_a_where_bound_generic_through_a_concrete_receiver() {
    let source = format!(
        "{SHARED}
i64 Main() {{
    Dec<i64, IntReader> decoder = Dec<i64, IntReader> {{ seed: 3_i64, binding: IntReader {{ }} }};
    return decoder.ReadTyped(4_i64);
}}
"
    );
    lower_single_unit(&source).unwrap_or_else(|error| {
        panic!(
            "a where-bound generic call from a bounded impl method, called on a concrete receiver, lowers: {error}\n{}",
            describe_nested_call_facts(&source)
        )
    });
}

#[test]
fn generic_function_calls_a_where_bound_generic_with_its_own_bounded_parameters() {
    let source = format!(
        "{SHARED}
T Outer<T, B>(Dec<T, B> decoder) where B: Reader<T> {{
    return ReadWith<T, B>(decoder.seed, decoder.binding);
}}

i64 Main() {{
    Dec<i64, IntReader> decoder = Dec<i64, IntReader> {{ seed: 3_i64, binding: IntReader {{ }} }};
    return Outer<i64, IntReader>(decoder);
}}
"
    );
    lower_single_unit(&source)
        .expect("a where-bound generic call from a bounded generic function with explicit enclosing parameters lowers");
}
