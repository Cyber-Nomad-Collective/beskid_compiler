use beskid_isle::{ISLE_INPUTS, RULE_COUNT};

#[test]
fn generated_isle_has_real_rules() {
    assert!(std::hint::black_box(RULE_COUNT) > 0);
}

#[test]
fn isle_inputs_are_in_one_stable_order() {
    assert_eq!(
        ISLE_INPUTS,
        &[
            "types.isle",
            "ast.isle",
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
        ]
    );
}
