use beskid_abi::runtime_source::{canonical_runtime_sources, CANONICAL_COLLECTIONS_SOURCE_PATH};

fn canonical_collections_source() -> String {
    canonical_runtime_sources()
        .into_iter()
        .find(|unit| unit.logical_path == CANONICAL_COLLECTIONS_SOURCE_PATH)
        .unwrap_or_else(|| panic!("missing canonical runtime source {CANONICAL_COLLECTIONS_SOURCE_PATH}"))
        .source
}

#[test]
fn canonical_array_growth_preserves_descriptor_roots_results_and_copies_only_initialized_values() {
    let collections = canonical_collections_source();

    assert!(collections.contains("Symbol:\"beskid_rt_v5_array_grow_rooted\""));
    assert!(
        collections.contains("pub pointer GrowArrayRooted(pointer array, word minimumCapacity, pointer rootHandleOut)")
    );
    assert!(collections.contains("raw_word_store(rootHandleOut, 0);"));
    assert!(collections.contains("pointer object = NativePointer(NativeWord(array) - 16);"));
    assert!(collections.contains("pointer descriptor = NativePointer(raw_word_load(object));"));
    assert!(collections.contains("raw_word_store(replacementObject, NativeWord(descriptor));"));
    assert!(collections.contains("if minimumCapacity <= capacity"));
    assert!(collections.contains("word existingHandle = GcRootHandle(object);"));
    assert!(collections.contains("word copyBytes = stride * length;"));
    assert!(collections.contains("word backingSize = stride * minimumCapacity;"));
    assert!(collections.contains("if minimumCapacity != 0 && backingSize / stride != minimumCapacity"));
    assert!(collections.contains("memory_copy(replacementData, sourceData, copyBytes);"));
    assert!(collections.contains("word handle = GcRootHandle(replacementObject);"));
    assert!(collections.contains("raw_word_store(rootHandleOut, handle);"));
    assert!(collections.contains("GcUnrootHandle(handle);"));
}
