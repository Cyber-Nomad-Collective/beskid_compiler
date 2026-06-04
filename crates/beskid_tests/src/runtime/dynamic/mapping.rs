use beskid_runtime::dynamic::{
    FieldStep, DYNAMIC_OK, map_objects_aot, register_mapping, register_shape,
    reset_tables_for_test,
};

#[repr(C)]
struct SourceShape {
    id: i64,
    flags: i32,
}

#[repr(C)]
struct TargetShape {
    id: i64,
    flags: i32,
}

const SRC_SHAPE: u32 = 1001;
const DST_SHAPE: u32 = 1002;

fn register_identity_mapping() {
    register_shape(SRC_SHAPE, std::mem::size_of::<SourceShape>());
    register_shape(DST_SHAPE, std::mem::size_of::<TargetShape>());
    register_mapping(
        SRC_SHAPE,
        DST_SHAPE,
        vec![
            FieldStep {
                src_offset: 0,
                dst_offset: 0,
                size: 8,
            },
            FieldStep {
                src_offset: 8,
                dst_offset: 8,
                size: 4,
            },
        ],
    );
}

#[test]
fn dynamic_aot_mapping_copies_fields_in_declaration_order() {
    reset_tables_for_test();
    register_identity_mapping();

    let src = SourceShape {
        id: 99,
        flags: 7,
    };
    let mut dst = TargetShape { id: 0, flags: 0 };

    let status = unsafe {
        map_objects_aot(
            SRC_SHAPE,
            DST_SHAPE,
            (&raw const src) as *const u8,
            (&raw mut dst) as *mut u8,
        )
    };
    assert_eq!(status, DYNAMIC_OK);
    assert_eq!(dst.id, 99);
    assert_eq!(dst.flags, 7);
}
