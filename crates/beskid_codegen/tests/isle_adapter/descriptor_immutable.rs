//! `TYPE_DESCRIPTOR_IMMUTABLE` follows the transfer class of each instantiation: a generic
//! record or enum whose class is Shared only through its type arguments is flagged per
//! specialization, never on the declaration.
use super::support::{find_nodes_of_kind, item_fixture_with_root};
use beskid_codegen::TYPE_DESCRIPTOR_IMMUTABLE;
use beskid_queries::IndexedNodeKind;

fn literal_flags(source: &str, kind: IndexedNodeKind) -> Vec<u64> {
    let (input, _isa, root) = item_fixture_with_root(source);
    find_nodes_of_kind(input.database(), root, kind)
        .into_iter()
        .map(|literal| {
            let plan = match kind {
                IndexedNodeKind::EnumConstructorExpression => input.enum_static_plan(literal),
                _ => input.aggregate_static_plan(literal),
            };
            plan.expect("static plan").descriptor_flags
        })
        .collect()
}

#[test]
fn immutable_record_specialization_is_flagged_and_isolated_record_is_not() {
    let source = "[Immutable] type Frozen<T> { i64 index, T value } [Isolated] type Owned<T> { i64 index, T value } \
                  i64 Main() { Frozen<string> a = Frozen<string> { index: 1_i64, value: \"x\" }; \
                  Owned<string> b = Owned<string> { index: 2_i64, value: \"y\" }; return a.index + b.index; }";
    let flags = literal_flags(source, IndexedNodeKind::StructLiteralExpression);
    assert_eq!(flags.len(), 2);
    assert_eq!(flags[0] & TYPE_DESCRIPTOR_IMMUTABLE, TYPE_DESCRIPTOR_IMMUTABLE, "Shared record is immutable");
    assert_eq!(flags[1] & TYPE_DESCRIPTOR_IMMUTABLE, 0, "Moved record is not immutable");
}

#[test]
fn generic_enum_is_immutable_only_when_its_arguments_are_shared() {
    let source = "enum Sequenced<T> { Item(i64 sequence, T value), Done } \
                  i64 Main() { Sequenced<string> shared = Sequenced::Item(1_i64, \"x\"); \
                  Sequenced<string[]> moved = Sequenced::Item(2_i64, [\"y\"]); return 0_i64; }";
    let flags = literal_flags(source, IndexedNodeKind::EnumConstructorExpression);
    assert_eq!(flags.len(), 2, "two constructors: {flags:?}");
    assert_eq!(flags[0] & TYPE_DESCRIPTOR_IMMUTABLE, TYPE_DESCRIPTOR_IMMUTABLE, "Sequenced<string> is Shared");
    assert_eq!(flags[1] & TYPE_DESCRIPTOR_IMMUTABLE, 0, "Sequenced<string[]> is Moved");
}

#[test]
fn non_generic_unmarked_record_stays_unflagged() {
    let source = "type Plain { i64 a } i64 Main() { Plain p = Plain { a: 1_i64 }; return p.a; }";
    assert_eq!(literal_flags(source, IndexedNodeKind::StructLiteralExpression), vec![0]);
}
