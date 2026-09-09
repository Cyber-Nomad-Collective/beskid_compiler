use super::support::{emit_isle_item, item_fixture};

#[test]
fn declared_integer_constant_contextualizes_to_a_signed_comparison_operand() {
    let (input, isa, item) = item_fixture(
        "const CHANNEL_BUFFER_SIZE = 16; bool Main(i64 capacity) { return capacity < 0 || capacity > CHANNEL_BUFFER_SIZE; }",
    );
    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("a declared integer constant contextualizes to its signed comparison sibling");
    let clif = function.display().to_string();

    assert_eq!(
        clif.lines().filter(|line| line.contains("icmp") && line.contains("slt")).count(),
        1,
        "the lower-bound comparison must remain signed:\n{clif}"
    );
    assert_eq!(
        clif.lines().filter(|line| line.contains("icmp") && line.contains("sgt")).count(),
        1,
        "the upper-bound comparison must remain signed:\n{clif}"
    );
    assert!(clif.contains("iconst.i64 16"), "the constant must adopt the sibling i64 representation:\n{clif}");
}
