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
    let field_store = append.find("self.emit_barriered_store(slot, array)").expect("barriered aggregate owner-field store");
    let finish = append.find("beskid_rt_v5_array_construction_finish").expect("construction finish");

    assert!(grow < element_store);
    assert!(element_store < owner_store);
    assert!(element_store < field_store);
    assert!(field_store < finish);
    assert_eq!(append.matches("beskid_rt_v5_array_construction_finish").count(), 1);
    // The owner-field store is barriered with the grown array (0.6.2 incremental marking), and
    // the element store into the slot past the old length needs none.
    assert!(!append.contains("beskid_rt_v5_array_write_barrier"), "no 0.6.1 array barrier in generated code");
    assert!(!append.contains("emit_barriered_store(slot, owner)"), "the owner store must publish the grown array, not the stale pre-grow owner");
}

#[test]
fn canonical_collection_growth_uses_mutable_owner_slots_in_every_storage_adapter() {
    // Array.Append grows storage in place since 0.5.3, so persistent adapters route every growth
    // through Core.Collections.Storage.AppendAt, which appends in place only when the version owns
    // the storage tip and copies the prefix otherwise. Building a fresh, version-private array
    // (copy, merge, filter, insert) is not growth of shared storage: such locals start from
    // `Array.Empty` or from a `CopyPrefix` copy, and only they may be grown with `Array.Append`.
    for source in [
        include_str!("../../../corelib/packages/foundation/src/Core/Collections/List.bd"),
        include_str!("../../../corelib/packages/foundation/src/Core/Collections/Map.bd"),
        include_str!("../../../corelib/packages/foundation/src/Core/Collections/Set.bd"),
        include_str!("../../../corelib/packages/foundation/src/Core/Collections/Queue.bd"),
        include_str!("../../../corelib/packages/foundation/src/Core/Collections/Stack.bd"),
    ] {
        assert!(source.contains("mut "), "collection growth must establish a mutable owner slot");
        assert!(source.contains("Storage.AppendAt<"), "collection adapter must use the shared storage growth");
        assert_private_growth_only(source);
    }
    let storage = include_str!("../../../corelib/packages/foundation/src/Core/Collections/Storage.bd");
    assert!(storage.contains("mut T[] grown = storage;"), "in-place growth must establish a mutable owner slot");
    assert!(storage.contains("Array.Append<T>(grown, value);"), "storage growth must use canonical array growth");
    assert!(!storage.contains("= Array.Append"), "growth must not await a later assignment for rooting");
}

/// Every direct `Array.Append` in a collection adapter must grow a local whose latest binding is
/// a fresh array (`Array.Empty<...>()` or a `CopyPrefix<...>(...)` copy), never shared storage.
fn assert_private_growth_only(source: &str) {
    const APPEND: &str = "Array.Append<";
    let mut offset = 0;
    while let Some(found) = source[offset..].find(APPEND) {
        let at = offset + found;
        offset = at + APPEND.len();
        let after_type = &source[offset..];
        let open = after_type.find(">(").expect("Array.Append type argument list") + 2;
        let target: String = after_type[open..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        assert!(!target.is_empty(), "Array.Append must grow a named mutable local");
        let binding = format!("[] {target} = ");
        let declared = source[..at]
            .rfind(&binding)
            .unwrap_or_else(|| panic!("Array.Append target `{target}` must be a local array binding"));
        let initializer = &source[declared + binding.len()..];
        assert!(
            initializer.starts_with("Array.Empty<") || initializer.starts_with("CopyPrefix<"),
            "adapters must not grow shared storage in place directly (`{target}`)"
        );
        let line_start = source[..declared].rfind('\n').map_or(0, |newline| newline + 1);
        assert!(
            source[line_start..declared].trim_start().starts_with("mut "),
            "growth target `{target}` must be a mutable owner slot"
        );
    }
}
