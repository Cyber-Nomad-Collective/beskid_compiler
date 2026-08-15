#[test]
fn append_publicates_the_proven_owner_before_exactly_one_finish() {
    let source = include_str!("../src/context/calls.rs");
    let append = source
        .split("CollectionOperation::Append { owner: mutation_owner }")
        .nth(1)
        .expect("production append lowering");
    let append = append.split("CollectionOperation::Clear").next().expect("append lowering boundary");

    let grow = append.find("beskid_rt_v5_array_grow_rooted").expect("rooted grow");
    let element_store = append.find("store(MemFlags::new(), value, address, 0)").expect("typed element store");
    let owner_store = append.find("self.builder.def_var(variable, array)").expect("local owner-slot store");
    let field_store = append.find("store(MemFlags::new(), array, base").expect("aggregate owner-field store");
    let publication = append.find("owner_barrier").expect("owner publication barrier");
    let finish = append.find("beskid_rt_v5_array_construction_finish").expect("construction finish");

    assert!(grow < element_store);
    assert!(element_store < owner_store);
    assert!(element_store < field_store);
    assert!(owner_store < publication);
    assert!(field_store < publication);
    assert!(publication < finish);
    assert_eq!(append.matches("beskid_rt_v5_array_construction_finish").count(), 1);
    assert!(!append.contains("call(owner_barrier, &[owner, array])"));
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
