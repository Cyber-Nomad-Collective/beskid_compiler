use std::fs;
use std::path::PathBuf;

#[test]
fn binary_and_unary_operator_facts_have_isle_rules() {
    let isle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("isle");
    let source = ["binary.isle", "unary_casts.isle", "control_flow.isle"]
        .into_iter()
        .map(|name| fs::read_to_string(isle.join(name)).expect("read ISLE rules"))
        .collect::<String>();

    for operator in [
        "Or",
        "And",
        "IdentityEq",
        "IdentityNotEq",
        "Eq",
        "NotEq",
        "Lt",
        "Lte",
        "Gt",
        "Gte",
        "Add",
        "Sub",
        "Mul",
        "Div",
        "Mod",
        "Neg",
        "Not",
    ] {
        assert!(
            source.contains(&format!("OperatorFact.{operator}")),
            "missing ISLE rule for {operator}"
        );
    }
}

#[test]
fn every_owned_rule_group_contains_a_real_rule() {
    let isle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("isle");
    for group in [
        "expressions.isle",
        "literals.isle",
        "binary.isle",
        "unary_casts.isle",
        "calls.isle",
        "statements.isle",
        "control_flow.isle",
        "memory.isle",
        "runtime_intrinsics.isle",
        "items.isle",
    ] {
        let source = fs::read_to_string(isle.join(group)).expect("read ISLE rule group");
        assert!(
            source.contains("(rule"),
            "{group} contains no real ISLE rule"
        );
    }
}
