#[test]
fn append_initializes_the_element_before_publishing_its_owner() {
    let source = include_str!("../src/context/calls/collections.rs");
    let append = source.split_once("CollectionOperation::Append { owner: mutation_owner }").unwrap().1;
    let append = append.split_once("CollectionOperation::Clear").unwrap().0;
    let grow = append.find("self.grow_array(owner, next_length)").unwrap();
    let element = append.find("store(MemFlagsData::new(), value, address, 0)").unwrap();
    let local = append.find("self.publish_managed_local(slot, array)").unwrap();
    let field = append.find("store(MemFlagsData::new(), array, base").unwrap();
    assert!(grow < element && element < local && element < field);
    assert!(!append.contains("call(barrier, &[base, array])"));
    assert!(!append.contains("call(barrier, &[owner, array])"));
}

#[test]
fn canonical_collection_growth_uses_mutable_owner_slots_in_every_storage_adapter() {
    // Array.Append grows storage in place since 0.5.3, so persistent adapters route every growth
    // through Core.Collections.Storage.AppendAt, which appends in place only when the version owns
    // the storage tip and copies the prefix otherwise.
    for source in [
        include_str!("../../../corelib/packages/foundation/src/Core/Collections/List.bd"),
        include_str!("../../../corelib/packages/foundation/src/Core/Collections/Map.bd"),
        include_str!("../../../corelib/packages/foundation/src/Core/Collections/Set.bd"),
        include_str!("../../../corelib/packages/foundation/src/Core/Collections/Queue.bd"),
        include_str!("../../../corelib/packages/foundation/src/Core/Collections/Stack.bd"),
    ] {
        assert!(source.contains("mut "), "collection growth must establish a mutable owner slot");
        assert!(source.contains("Storage.AppendAt<"), "collection adapter must use the shared storage growth");
        assert!(!source.contains("Array.Append"), "adapters must not grow shared storage in place directly");
    }
    let storage = include_str!("../../../corelib/packages/foundation/src/Core/Collections/Storage.bd");
    assert!(storage.contains("mut T[] grown = storage;"), "in-place growth must establish a mutable owner slot");
    assert!(storage.contains("Array.Append<T>(grown, value);"), "storage growth must use canonical array growth");
    assert!(!storage.contains("= Array.Append"), "growth must not await a later assignment for rooting");
}
