use crate::codegen::util::lower_resolve_type;
use beskid_codegen::{
    DYNAMIC_TYPE_NAME, dynamic_clif_type, is_dynamic_type_id, map_type_id_to_clif_with_dynamic,
    pointer_type,
};

fn find_named_type_id_for_item(
    typed: &beskid_analysis::types::TypeResult,
    item_id: beskid_analysis::resolve::ItemId,
) -> Option<beskid_analysis::types::TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = beskid_analysis::types::TypeId(index);
        let info = typed.types.get(type_id)?;
        if matches!(info, beskid_analysis::types::TypeInfo::Named(found) if *found == item_id) {
            return Some(type_id);
        }
        index += 1;
    }
}

#[test]
fn dynamic_named_alias_maps_to_cell_pointer_clif() {
    let source = format!("type {DYNAMIC_TYPE_NAME} {{ i64 payload }} i64 Main() {{ return 0; }}");
    let (_, resolution, typed) = lower_resolve_type(&source);

    let dynamic_item = resolution
        .items
        .iter()
        .find(|item| item.name == DYNAMIC_TYPE_NAME)
        .expect("expected dynamic type alias");
    let dynamic_type_id = find_named_type_id_for_item(&typed, dynamic_item.id)
        .expect("expected named type id for dynamic alias");

    assert!(is_dynamic_type_id(&resolution, &typed, dynamic_type_id));
    assert_eq!(dynamic_clif_type(), pointer_type());
    assert_eq!(
        map_type_id_to_clif_with_dynamic(&resolution, &typed, dynamic_type_id),
        Some(pointer_type())
    );
}
