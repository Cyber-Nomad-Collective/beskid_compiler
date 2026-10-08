//! A nested match whose scrutinee is a projected path rooted in an enum pattern binding
//! (`match value.node.literal.node { ... }`, the Serialization Mod's `Policies.Text` shape).
use super::support::{SyntaxGenerationId, SyntaxModuleItem, canonical_corelib_project_fixture, find_function_definitions, lower_syntax_program};

const TEXT_SOURCE: &str = r#"
enum Literal { Str(string raw), Int(string digits), }
type SpannedLiteral { Literal node, u32 id, }
type LiteralExpression { SpannedLiteral literal, }
type SpannedLiteralExpression { LiteralExpression node, u32 id, }
enum Expression { Literal(SpannedLiteralExpression value), Other, }
type SpannedExpression { Expression node, u32 id, }
string Text(SpannedExpression expression) {
    match expression.node {
        Expression::Literal(value) => {
            match value.node.literal.node {
                Literal::Str(raw) => { return raw; },
                _ => { },
            };
        },
        _ => { },
    };
    return "";
}
"#;

#[test]
fn match_on_a_projected_path_rooted_in_a_pattern_binding_lowers() {
    let (input, isa, root) = canonical_corelib_project_fixture(&[("App/Text.bd", TEXT_SOURCE)], SyntaxGenerationId(213));
    let functions = find_function_definitions(input.database(), root);
    let text = functions[0];
    lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: text, symbol: "Text".into() }])
        .expect("a match on `value.node.literal.node` lowers through generated ISLE");
}

const NESTED_SOURCE: &str = r#"
enum Rejection { Bad(string reason), }
enum Outcome<E> { Done, Failed(E error), }
type Item {}
Outcome<Rejection> Nested<S>(S source, i64 depth) {
    if (depth > 3_i64) { return Outcome<Rejection>::Done; }
    match Nested<S>(source, depth + 1_i64) { Outcome::Failed(error) => { return Outcome<Rejection>::Failed(error); }, _ => { }, };
    return Outcome<Rejection>::Done;
}
unit Main(Item item) { Nested<Item>(item, 0_i64); return; }
"#;

/// Inside the `Nested<Item>` specialization the scrutinee `Nested<S>(source, depth + 1)` is a
/// generic call forwarding the enclosing parameter. Its declared `Outcome<Rejection>` result is
/// the match authority; the call must not be re-inferred from the untyped `S` argument.
#[test]
fn match_on_a_generic_call_forwarding_the_enclosing_parameter_lowers() {
    let (input, isa, root) = canonical_corelib_project_fixture(&[("App/Nested.bd", NESTED_SOURCE)], SyntaxGenerationId(215));
    let functions = find_function_definitions(input.database(), root);
    let nested = functions[0];
    let main = functions[1];
    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem { key: nested, symbol: "Nested".into() }, SyntaxModuleItem { key: main, symbol: "Main".into() }],
    )
    .expect("a match on `Nested<S>(...)` lowers with the Nested<Item> specialization");
}
