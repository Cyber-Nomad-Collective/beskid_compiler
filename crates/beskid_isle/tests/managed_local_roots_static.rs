#[test]
fn managed_local_rooting_is_one_atomized_lowering_seam() {
    let roots = include_str!("../src/context/roots.rs");
    assert_eq!(roots.matches("gc_register_root").count(), 1, "one registration implementation");
    assert_eq!(roots.matches("gc_unregister_root").count(), 1, "one unregistration implementation");

    let control_flow = include_str!("../src/context/control_flow.rs");
    assert!(control_flow.contains("bind_local"), "parameters/lets must use the binding seam");
    assert!(control_flow.contains("assign_local"), "assignments must update the same root slot");
    assert!(control_flow.contains("release_managed_local_roots"), "explicit returns must clean up roots");

    let emitter = include_str!("../src/emitter.rs");
    assert!(emitter.contains("release_managed_local_roots"), "implicit fallthrough must clean up roots");
}

#[test]
fn append_roots_managed_values_and_uses_owner_specific_publication() {
    let calls = include_str!("../src/context/calls/collections.rs");
    let append = calls
        .split("CollectionOperation::Append { owner: mutation_owner }")
        .nth(1)
        .expect("append lowering")
        .split("CollectionOperation::Clear")
        .next()
        .expect("append boundary");

    let payload_root = append.find("root_temporary").expect("payload lifetime root");
    let grow = append.find("beskid_rt_v5_array_grow_rooted").expect("rooted grow");
    let local_publication = append.find("publish_managed_local").expect("local root publication");
    let finish = append.find("beskid_rt_v5_array_construction_finish").expect("construction finish");

    assert!(payload_root < grow);
    assert!(grow < local_publication);
    assert!(local_publication < finish);

    // The span-heap design (docs/superpowers/specs/2026-09-22-gc-span-heap-design.md, section
    // 6.3) removes the `gc_write_barrier` call generated code used to emit for an aggregate-field
    // mutation owner: collection is stop-the-world, so a managed store can never race a
    // concurrent marker, and a barrier that never fires is pure call overhead. The runtime export
    // stays (for a future incremental collector) but no call site remains in generated code.
    let call_site = append.find("self.builder.ins().call(barrier, &[base, array])");
    assert!(call_site.is_none(), "no gc_write_barrier call should be emitted for the aggregate-field owner");
}
