//! The executable front end (legacy resolver + checker) accepts the string ordering comparisons the
//! query layer lowers (`StringLt`..`StringGte`): `"a" < "b"` is a `bool`, while a string ordered
//! against a number stays rejected.

use beskid_analysis::services::{SemanticFactsError, parse_program, resolve_and_type_program};

fn type_errors(source: &str) -> Vec<String> {
    let program = parse_program(source).expect("source parses");
    match resolve_and_type_program(&program) {
        Ok(_) => Vec::new(),
        Err(SemanticFactsError::Resolve(errors)) => panic!("resolution failed: {errors:?}"),
        Err(SemanticFactsError::Type { errors, .. }) => errors.iter().map(|error| error.to_string()).collect(),
    }
}

#[test]
fn string_ordering_comparisons_type_as_bool() {
    let errors = type_errors(
        r#"
bool Main(string a, string b) {
    bool lt = a < b;
    bool lte = a <= b;
    bool gt = a > b;
    bool gte = "Z" >= "a";
    return lt && lte && gt && gte;
}
"#,
    );
    assert!(errors.is_empty(), "string ordering must type check: {errors:?}");
}

#[test]
fn string_ordered_against_a_number_stays_rejected() {
    let errors = type_errors("bool Main(string a, i64 n) { return a < n; }");
    assert!(!errors.is_empty(), "a string ordered against a number must not type check");
}
