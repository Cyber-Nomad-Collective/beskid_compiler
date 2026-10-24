//! Omitted optional arguments are filled with their defaults once callees are known, so lowering sees
//! ordinary explicit arguments.
use super::support::{SyntaxModuleItem, find_function_definitions, item_fixture_with_root, lower_syntax_program};

fn lowers(source: &str, symbols: &[&str]) {
    let (input, isa, root) = item_fixture_with_root(source);
    let functions = find_function_definitions(input.database(), root);
    let items = symbols
        .iter()
        .zip(functions.iter())
        .map(|(symbol, key)| SyntaxModuleItem { key: *key, symbol: (*symbol).into() })
        .collect::<Vec<_>>();
    lower_syntax_program(&input, isa.as_ref(), &items).expect("calls with omitted optional arguments lower");
}

#[test]
fn integer_defaults_fill_omitted_arguments() {
    lowers(
        "i64 Scale(i64 value, i64 factor = 3, i64 offset = -1) { return value * factor + offset; }\n\
         i64 Main() { return Scale(2) + Scale(2, 4) + Scale(2, 4, 0); }",
        &["Scale", "Main"],
    );
}

#[test]
fn a_constant_and_a_bool_default_fill_omitted_arguments() {
    lowers(
        "const Limit = 9;\n\
         i64 Pick(bool high = true, i64 limit = Limit) { if high { return limit; } return 0; }\n\
         i64 Main() { return Pick() + Pick(false); }",
        &["Pick", "Main"],
    );
}

#[test]
fn an_integer_default_converts_to_a_float_parameter() {
    lowers("f64 Half(f64 value = 1) { return value / 2.0; }\nf64 Main() { return Half(); }", &["Half", "Main"]);
}

/// Lower every method and function of `source`.
fn lowers_with_methods(source: &str) {
    use super::support::{find_nodes_of_kind, item_name};
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let mut items = Vec::new();
    for kind in [beskid_queries::IndexedNodeKind::MethodDefinition, beskid_queries::IndexedNodeKind::FunctionDefinition] {
        for key in find_nodes_of_kind(db, root, kind) {
            let name = item_name(db, key).ok().flatten().expect("item name");
            items.push(SyntaxModuleItem { key, symbol: name.to_string() });
        }
    }
    lower_syntax_program(&input, isa.as_ref(), &items).expect("method calls with omitted optional arguments lower");
}

#[test]
fn a_method_call_gets_the_method_defaults() {
    lowers_with_methods(
        "type Counter { i64 n\n pub i64 Add(i64 base, i64 step = 1) { return base + step + n; } }\n\
         i64 Main() { Counter counter = Counter { n: 0_i64 }; return counter.Add(2) + counter.Add(2, 3); }",
    );
}

#[test]
fn a_contract_bound_call_gets_the_contract_defaults() {
    lowers_with_methods(
        "contract Stepper { i64 Step(i64 base, i64 by = 1); }\n\
         type Walker : Stepper { i64 n\n pub i64 Step(i64 base, i64 by) { return base + by + n; } }\n\
         i64 Use<T>(T stepper) where T: Stepper { return stepper.Step(5); }\n\
         i64 Main() { Walker walker = Walker { n: 0_i64 }; return Use<Walker>(walker) + walker.Step(1); }",
    );
}
