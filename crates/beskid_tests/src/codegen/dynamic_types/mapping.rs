use crate::codegen::util::lower_resolve_type;
use beskid_analysis::resolve::{ItemId, ItemKind};
use beskid_codegen::require_mapping_eligible;

fn first_two_struct_items(resolution: &beskid_analysis::resolve::Resolution) -> (ItemId, ItemId) {
    let mut ids = resolution
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Type)
        .map(|item| item.id);
    (
        ids.next().expect("expected first struct"),
        ids.next().expect("expected second struct"),
    )
}

#[test]
fn dynamic_aot_mapping_eligibility_passes_for_matching_structs() {
    let source = "type Source { i64 id, i32 flags } type Target { i64 id, i32 flags } i64 Main() { return 0; }";
    let (_, resolution, typed) = lower_resolve_type(source);
    let (src_item, dst_item) = first_two_struct_items(&resolution);
    let span = beskid_analysis::syntax::SpanInfo {
        start: 0,
        end: 1,
        line_col_start: (1, 1),
        line_col_end: (1, 2),
    };
    require_mapping_eligible(span, &resolution, &typed, src_item, dst_item)
        .expect("eligible struct pair should pass mapping gate");
}
