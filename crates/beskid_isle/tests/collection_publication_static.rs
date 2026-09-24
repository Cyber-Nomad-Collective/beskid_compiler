#[test]
fn append_publicates_the_proven_owner_before_exactly_one_finish() {
    let source = include_str!("../src/context/calls/collections.rs");
    let append = source
        .split("CollectionOperation::Append { owner: mutation_owner }")
        .nth(1)
        .expect("production append lowering");
    let append = append.split("CollectionOperation::Clear").next().expect("append lowering boundary");

    let grow = append.find("beskid_rt_v5_array_grow_rooted").expect("rooted grow");
    let element_store = append.find("store(MemFlagsData::new(), value, address, 0)").expect("typed element store");
    let owner_store = append.find("self.publish_managed_local(slot, array)").expect("local owner-slot store");
    let field_store = append.find("store(MemFlagsData::new(), array, base").expect("aggregate owner-field store");
    let finish = append.find("beskid_rt_v5_array_construction_finish").expect("construction finish");

    assert!(grow < element_store);
    assert!(element_store < owner_store);
    assert!(element_store < field_store);
    assert!(field_store < finish);
    assert_eq!(append.matches("beskid_rt_v5_array_construction_finish").count(), 1);
    // The span-heap design (docs/superpowers/specs/2026-09-22-gc-span-heap-design.md, section
    // 6.3) removes the write-barrier call for the aggregate-field owner: collection is
    // stop-the-world, so no publication barrier is needed between the field store and finish.
    assert!(
        !append.contains("self.builder.ins().call(barrier, &[base, array])"),
        "no write-barrier call should be emitted for the aggregate-field owner"
    );
    assert!(!append.contains("call(barrier, &[owner, array])"), "publication barrier must not reuse the stale pre-grow owner pointer");
}

#[test]
fn canonical_collection_growth_uses_mutable_owner_slots_in_every_storage_adapter() {
    for source in [
        include_str!("../../../corelib/packages/foundation/src/Core/Collections/List.bd"),
        include_str!("../../../corelib/packages/foundation/src/Core/Collections/Map.bd"),
        include_str!("../../../corelib/packages/foundation/src/Core/Collections/Set.bd"),
        include_str!("../../../corelib/packages/foundation/src/Core/Collections/Queue.bd"),
        include_str!("../../../corelib/packages/foundation/src/Core/Collections/Stack.bd"),
    ] {
        assert!(source.contains("mut "), "collection growth must establish a mutable owner slot");
        assert!(source.contains("Array.Append"), "collection adapter must use canonical array growth");
        assert!(!source.contains("= Array.Append"), "growth must not await a later assignment for rooting");
    }
}
