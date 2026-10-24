//! Omitted optional arguments are filled with their defaults before indexing, so lowering sees
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
