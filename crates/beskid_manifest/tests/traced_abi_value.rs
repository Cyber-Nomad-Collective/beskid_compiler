use beskid_manifest::{generate_v5_artifacts, load_v5_manifest_source};

#[test]
fn fiber_scheduler_layout_has_disjoint_traced_slots_and_one_runtime_authority() {
    let manifest = load_v5_manifest_source(include_str!("../../../runtime_manifest.bsol")).unwrap();
    let record = manifest.layouts.iter().find(|layout| layout.name == "BeskidFiberRecord").unwrap();
    let scheduler = manifest.layouts.iter().find(|layout| layout.name == "BeskidSchedulerState").unwrap();
    let offset = |name: &str| record.fields.iter().find(|field| field.name == name).unwrap().offset;
    assert_eq!(offset("result"), 160);
    assert_eq!(offset("capture"), offset("result") + 40);
    assert_eq!(record.size, offset("capture") + 40);
    let fibers = scheduler.fields.iter().find(|field| field.name == "fibers").unwrap();
    assert_eq!(fibers.offset, 4048);
    assert_eq!(scheduler.size, fibers.offset + 16 * record.size);
    assert_eq!(record.project_to_runtime.as_deref(), Some("constants"));
    assert_eq!(scheduler.project_to_runtime.as_deref(), Some("constants"));
    let source = include_str!("../../../runtime/beskid/src/Runtime/Fiber/Scheduler/Core.bd");
    assert!(source.contains("SystemAllocate(BESKID_SCHEDULER_STATE_SIZE, 8)"));
    assert!(source.contains("BESKID_SCHEDULER_STATE_FIBERS_OFFSET + index * BESKID_FIBER_RECORD_SIZE"));
}

#[test]
fn fiber_result_transfer_requires_a_caller_owned_traced_destination() {
    let manifest = load_v5_manifest_source(include_str!("../../../runtime_manifest.bsol")).unwrap();
    let join = manifest.corelib_services.iter().find(|service| service.name == "__fiber_join_value").unwrap();
    assert_eq!(join.params.iter().map(|parameter| parameter.ty.as_str()).collect::<Vec<_>>(), ["i64", "pointer"]);
    assert_eq!(join.result, "u8");
}

#[test]
fn traced_value_layout_and_owner_operations_cross_every_generated_boundary() {
    let manifest = load_v5_manifest_source(include_str!("../../../runtime_manifest.bsol")).unwrap();
    let layout = manifest
        .layouts
        .iter()
        .find(|layout| layout.name == "BeskidAbiValue")
        .expect("Foundation requires the canonical traced BeskidAbiValue layout");
    assert_eq!((layout.size, layout.alignment), (40, 8));
    assert_eq!(
        layout.fields.iter().map(|field| (field.name.as_str(), field.offset, field.ty.as_str())).collect::<Vec<_>>(),
        [
            ("tag", 0, "usize"),
            ("payload", 8, "pointer"),
            ("descriptor", 16, "pointer"),
            ("owner_heap", 24, "pointer"),
            ("owner_state", 32, "usize")
        ]
    );
    for operation in ["initialize", "replace_with_barrier", "move_out", "clear"] {
        let symbol = format!("beskid_rt_v5_abi_value_{operation}");
        assert!(manifest.exports.iter().any(|export| export.symbol == symbol), "missing {symbol}");
    }
    let generated = generate_v5_artifacts(&manifest).unwrap();
    assert!(generated.c_header.contains("#define BESKID_ABI_VALUE_PAYLOAD_OFFSET 8"));
    assert!(generated.rust.contains("BESKID_ABI_VALUE_PAYLOAD_OFFSET = 8"));
    assert!(generated.abi_json.contains("BeskidAbiValue"));
}
