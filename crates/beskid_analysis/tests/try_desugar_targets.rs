//! Item 4: `try_desugar_targets_for_program` must use the same typed path as
//! `invalid_try_expression_spans` (type the entry items before walking), not the body-blind
//! precheck that only seeds declaration surfaces. Without typing bodies first, a `?` operand
//! that is a `let`-bound local has no known type when `infer_expression_type` runs, so the
//! desugar target is silently dropped instead of recorded.

use beskid_analysis::services::{parse_program, resolve_and_type_program};
use beskid_analysis::types::try_desugar::try_desugar_targets_for_program;

const LOCAL_GENERIC_RESULT: &str = "enum Error { Failed() }
enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }
";

#[test]
fn try_desugar_targets_recognize_let_bound_locals() {
    let source = format!(
        "{LOCAL_GENERIC_RESULT}Result<i32, Error> Propagate(Result<i32, Error> input) {{
    Result<i32, Error> bound = input;
    i32 value = bound?;
    return Result::Ok(value);
}}
"
    );
    let program = parse_program(&source).expect("source must parse");
    let (program, resolution, _typed) =
        resolve_and_type_program(&program).expect("source must resolve and type-check");

    let targets = try_desugar_targets_for_program(&resolution, &program, &[]);

    assert_eq!(targets.len(), 1, "expected exactly one desugar target for the let-bound local, got {targets:?}");
    let target = targets.values().next().expect("one target");
    assert_eq!(target.type_name, "Result");
    assert_eq!(target.ok_variant, "Ok");
}

#[test]
fn try_desugar_targets_recognize_locals_bound_after_a_statement_match() {
    let source = format!(
        "{LOCAL_GENERIC_RESULT}Result<i32, Error> Propagate(Result<i32, Error> input) {{
    Result<i32, Error> bound = input;
    match input {{
        Result::Ok(_) => {{}},
        Result::Error(_) => {{}},
    }};
    i32 value = bound?;
    return Result::Ok(value);
}}
"
    );
    let program = parse_program(&source).expect("source must parse");
    let (program, resolution, _typed) =
        resolve_and_type_program(&program).expect("source must resolve and type-check");

    let targets = try_desugar_targets_for_program(&resolution, &program, &[]);

    assert_eq!(targets.len(), 1, "expected exactly one desugar target, got {targets:?}");
}
