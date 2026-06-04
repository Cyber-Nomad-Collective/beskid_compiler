use beskid_engine::Engine;
use beskid_runtime::dynamic::{
    DynamicCell, DYNAMIC_ERR_INCOMPATIBLE, DYNAMIC_OK, FieldStep, map_dynamic_fallback,
    register_mapping, register_shape, reset_tables_for_test,
};
use beskid_runtime::dynamic_cell_create;

const SRC_SHAPE: u32 = 2001;
const DST_SHAPE: u32 = 2002;

#[repr(C)]
struct Payload {
    value: i64,
}

#[test]
fn dynamic_fallback_mapping_succeeds_for_registered_shapes() {
    reset_tables_for_test();
    register_shape(SRC_SHAPE, std::mem::size_of::<Payload>());
    register_shape(DST_SHAPE, std::mem::size_of::<Payload>());
    register_mapping(
        SRC_SHAPE,
        DST_SHAPE,
        vec![FieldStep {
            src_offset: 0,
            dst_offset: 0,
            size: 8,
        }],
    );

    let mut engine = Engine::new();
    engine.with_runtime(|heap, root| {
        let payload = heap.allocate_beskid(std::mem::size_of::<Payload>(), std::ptr::null());
        root.runtime_state.allocation_counter += 1;
        unsafe {
            *(payload as *mut Payload) = Payload { value: 42 };
        }

        let cell_ptr = dynamic_cell_create(SRC_SHAPE, payload);
        let cell = unsafe { &*cell_ptr };

        let mut out = Payload { value: 0 };
        let status =
            unsafe { map_dynamic_fallback(cell, DST_SHAPE, (&raw mut out) as *mut u8) };
        assert_eq!(status, DYNAMIC_OK);
        assert_eq!(out.value, 42);
    });
}

#[test]
fn dynamic_fallback_returns_deterministic_incompatible_error() {
    reset_tables_for_test();
    register_shape(SRC_SHAPE, std::mem::size_of::<Payload>());
    register_shape(DST_SHAPE, std::mem::size_of::<Payload>());

    let cell = DynamicCell {
        shape_id: SRC_SHAPE,
        flags: 0,
        payload: 0x1 as *mut u8,
    };
    let mut out = Payload { value: 0 };

    let status =
        unsafe { map_dynamic_fallback(&cell, DST_SHAPE, (&raw mut out) as *mut u8) };
    assert_eq!(
        status,
        DYNAMIC_ERR_INCOMPATIBLE,
        "missing mapping must surface deterministic E-dynamic-map-001 status"
    );
}
