use super::support::{emit_isle_item, item_fixture};

#[test]
fn signed_negation_retains_operand_type_inside_a_short_circuit_comparison() {
    let (input, isa, item) =
        item_fixture("bool Main(i64 delta, i64 current) { return delta < 0 && current < -delta; }");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("a negated signed local retains its type while lowering a short-circuit comparison");
    let clif = function.display().to_string();

    assert_eq!(
        clif.lines().filter(|line| line.contains("icmp") && line.contains("slt")).count(),
        2,
        "both signed comparisons must lower:\n{clif}"
    );
    assert!(clif.contains("ineg"), "the signed negation must lower:\n{clif}");
}
