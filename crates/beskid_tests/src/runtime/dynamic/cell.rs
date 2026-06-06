use crate::support::runtime::with_runtime_scope;
use beskid_runtime::{
    dynamic::DynamicCell, dynamic_cell_create, gc_object_count,
};

#[test]
fn dynamic_cell_create_allocates_through_runtime_arena() {
    with_runtime_scope(|heap, root| {
        let before = gc_object_count();
        let payload = heap.allocate_beskid(8, std::ptr::null());
        root.runtime_state.allocation_counter += 1;

        let cell_ptr = dynamic_cell_create(42, payload);
        assert!(!cell_ptr.is_null(), "expected non-null dynamic cell");

        let cell = unsafe { &*cell_ptr };
        assert_eq!(cell.shape_id, 42);
        assert_eq!(cell.payload, payload);
        assert_eq!(cell.flags, 0);
        assert_eq!(DynamicCell::SIZE, std::mem::size_of::<DynamicCell>());

        assert!(
            gc_object_count() > before,
            "dynamic cell header allocation should increase object count"
        );
    });
}
