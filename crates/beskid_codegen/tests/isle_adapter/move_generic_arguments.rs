//! An array argument to a generic call inside a generic body (`Parallel.OfArray<T>` forwarding
//! `values` to `ParallelSeq.OfArray<T>`) lowers in the caller's specialization. Since 0.6.7 the
//! hand-over needs no `move`: `move` is written only at an isolate crossing.
use super::support::{
    SyntaxGenerationId, SyntaxModuleItem, canonical_corelib_project_fixture, find_function_definitions,
    lower_syntax_program,
};

const SOURCE: &str = r#"
[CompilerDerived] pub contract Transferable { }
T[] Keep<T>(T[] values) where T: Transferable { return values; }
T[] Forward<T>(T[] values) where T: Transferable { return Keep<T>(values); }
i64 Main() {
    i64[] data = [1_i64];
    i64[] kept = Forward<i64>(data);
    return kept[0];
}
"#;

#[test]
fn array_argument_to_a_generic_call_in_a_generic_body_lowers() {
    let (input, isa, root) = canonical_corelib_project_fixture(&[("App/MoveArgs.bd", SOURCE)], SyntaxGenerationId(254));
    let functions = find_function_definitions(input.database(), root);
    let items = ["Keep", "Forward", "Main"]
        .iter()
        .zip(functions.iter())
        .map(|(symbol, key)| SyntaxModuleItem { key: *key, symbol: (*symbol).into() })
        .collect::<Vec<_>>();
    lower_syntax_program(&input, isa.as_ref(), &items).expect("an array argument to a generic call lowers");
}

fn lower_variant(file: &str, source: &str, generation: u64, symbols: &[&str]) {
    let (input, isa, root) = canonical_corelib_project_fixture(&[(file, source)], SyntaxGenerationId(generation));
    let functions = find_function_definitions(input.database(), root);
    let items = symbols
        .iter()
        .zip(functions.iter())
        .map(|(symbol, key)| SyntaxModuleItem { key: *key, symbol: (*symbol).into() })
        .collect::<Vec<_>>();
    lower_syntax_program(&input, isa.as_ref(), &items).expect("variant lowers");
}

#[test]
fn array_argument_to_a_generic_call_in_a_plain_body_lowers() {
    let source = "[CompilerDerived] pub contract Transferable { }\nT[] Keep<T>(T[] values) where T: Transferable { return values; }\ni64 Main() { i64[] data = [1_i64]; i64[] kept = Keep<i64>(data); return kept[0]; }\n";
    lower_variant("App/ProbeB.bd", source, 256, &["Keep", "Main"]);
}
