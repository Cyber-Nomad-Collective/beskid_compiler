//! A `unit` local has no storage: `unit done = Effect();` and a generic `T value = ...;` at
//! `T = unit` (the `Task<unit>.Join()` shape, `T value = __isolate_join_value<T>(id)`) lower by
//! evaluating the initializer for its effects.
use super::support::{SyntaxGenerationId, SyntaxModuleItem, canonical_corelib_project_fixture, find_function_definitions, lower_syntax_program};

const SOURCE: &str = r#"
enum Outcome<T> { Done(T value), Failed, }
unit Effect() { return; }
T Pass<T>(T input) { T value = input; return value; }
Outcome<T> Wrap<T>(T input) { T value = Pass<T>(input); return Outcome<T>::Done(value); }
i64 Main() {
    unit done = Effect();
    Pass<unit>(());
    Outcome<unit> wrapped = Wrap<unit>(());
    return match wrapped { Outcome::Done(_) => 1_i64, Outcome::Failed => 0_i64, };
}
"#;

#[test]
fn unit_locals_lower_their_initializer_for_effect() {
    let (input, isa, root) = canonical_corelib_project_fixture(&[("App/UnitLet.bd", SOURCE)], SyntaxGenerationId(253));
    let functions = find_function_definitions(input.database(), root);
    let items = ["Effect", "Pass", "Wrap", "Main"]
        .iter()
        .zip(functions.iter())
        .map(|(symbol, key)| SyntaxModuleItem { key: *key, symbol: (*symbol).into() })
        .collect::<Vec<_>>();
    lower_syntax_program(&input, isa.as_ref(), &items).expect("unit locals lower through generated ISLE");
}
