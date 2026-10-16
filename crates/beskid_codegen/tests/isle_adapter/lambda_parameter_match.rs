//! A match on a typed lambda parameter (`(i64 index, Color color) => match color { ... }`, the Glue
//! extensions callback shape) lowers: the scrutinee is typed by the binding-identity authority, in
//! a non-generic body and inside a generic specialization.
use super::support::{SyntaxGenerationId, SyntaxModuleItem, canonical_corelib_project_fixture, find_function_definitions, lower_syntax_program};

const PLAIN_SOURCE: &str = r#"
enum Color { Red, Green, }
i64 Visit((i64, Color) => i64 f) { return f(1_i64, Color::Red); }
i64 Main() {
    return Visit((i64 index, Color color) => match color { Color::Red => index, Color::Green => 0_i64, });
}
"#;

#[test]
fn match_on_a_typed_lambda_parameter_lowers() {
    let (input, isa, root) = canonical_corelib_project_fixture(&[("App/Visit.bd", PLAIN_SOURCE)], SyntaxGenerationId(251));
    let functions = find_function_definitions(input.database(), root);
    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: functions[0], symbol: "Visit".into() },
            SyntaxModuleItem { key: functions[1], symbol: "Main".into() },
        ],
    )
    .expect("a match on the lambda parameter `color` lowers through generated ISLE");
}

const GENERIC_SOURCE: &str = r#"
enum Slot<T> { Full(T value), Empty, }
i64 Each<T>((i64, Slot<T>) => i64 f, Slot<T> slot) { return f(0_i64, slot); }
i64 Count<T>(Slot<T> slot) {
    return Each<T>((i64 index, Slot<T> item) => match item { Slot::Full(_) => index + 1_i64, Slot::Empty => index, }, slot);
}
i64 Main() { return Count<i64>(Slot<i64>::Full(3_i64)); }
"#;

#[test]
fn match_on_a_typed_lambda_parameter_in_a_generic_body_lowers() {
    let (input, isa, root) = canonical_corelib_project_fixture(&[("App/Count.bd", GENERIC_SOURCE)], SyntaxGenerationId(252));
    let functions = find_function_definitions(input.database(), root);
    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: functions[0], symbol: "Each".into() },
            SyntaxModuleItem { key: functions[1], symbol: "Count".into() },
            SyntaxModuleItem { key: functions[2], symbol: "Main".into() },
        ],
    )
    .expect("a match on the lambda parameter `item` lowers in the Count<i64> specialization");
}
