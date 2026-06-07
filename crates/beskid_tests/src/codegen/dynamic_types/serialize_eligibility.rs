use crate::codegen::util::lower_resolve_type;
use beskid_analysis::resolve::{ItemId, ItemKind};
use beskid_codegen::{mapping_pair_eligible, require_mapping_eligible, shape_id_for_item};

fn struct_item_ids(resolution: &beskid_analysis::resolve::Resolution) -> Vec<ItemId> {
    resolution
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Type)
        .map(|item| item.id)
        .collect()
}

#[test]
fn dynamic_identity_mapping_eligible_for_matching_structs() {
    let source = "type Source { i64 id, i32 flags } type Target { i64 id, i32 flags } i64 Main() { return 0; }";
    let (_, resolution, typed) = lower_resolve_type(source);
    let ids = struct_item_ids(&resolution);
    assert_eq!(ids.len(), 2, "expected two struct types");
    assert!(mapping_pair_eligible(&resolution, &typed, ids[0], ids[1]));
}

#[test]
fn dynamic_mapping_rejects_mismatched_field_types() {
    let source = "type Source { i64 id } type Target { i32 id } i64 Main() { return 0; }";
    let (_, resolution, typed) = lower_resolve_type(source);
    let ids = struct_item_ids(&resolution);
    assert!(!mapping_pair_eligible(&resolution, &typed, ids[0], ids[1]));
}

#[test]
fn dynamic_require_mapping_eligible_returns_structured_error() {
    let source = "type Source { string name } type Target { i64 id } i64 Main() { return 0; }";
    let (_, resolution, typed) = lower_resolve_type(source);
    let ids = struct_item_ids(&resolution);
    let span = beskid_analysis::syntax::SpanInfo {
        start: 0,
        end: 1,
        line_col_start: (1, 1),
        line_col_end: (1, 2),
    };
    let err = require_mapping_eligible(span, &resolution, &typed, ids[0], ids[1])
        .expect_err("expected ineligible mapping error");
    assert!(
        format!("{err}").contains("Source"),
        "expected source type name in error: {err}"
    );
}

#[test]
fn dynamic_shape_id_is_stable_for_resolved_struct_items() {
    let source = "type Source { i64 id } type Target { i64 id } i64 Main() { return 0; }";
    let (_, resolution, _) = lower_resolve_type(source);
    let ids = struct_item_ids(&resolution);
    assert_ne!(shape_id_for_item(ids[0]), shape_id_for_item(ids[1]));
    assert_eq!(shape_id_for_item(ids[0]), shape_id_for_item(ids[0]));
}
