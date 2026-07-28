use beskid_abi::runtime_source::{
    CANONICAL_HUB_SOURCE_PATH, CANONICAL_SCHEDULER_SOURCE_PATH, canonical_runtime_sources,
};

fn canonical_source(path: &str) -> String {
    canonical_runtime_sources()
        .into_iter()
        .find(|unit| unit.logical_path == path)
        .unwrap_or_else(|| panic!("missing canonical runtime source {path}"))
        .source
}

#[test]
fn canonical_hub_storage_is_scheduler_owned_and_supports_stable_256_entry_registration() {
    let scheduler = canonical_source(CANONICAL_SCHEDULER_SOURCE_PATH);
    let hub = canonical_source(CANONICAL_HUB_SOURCE_PATH);

    assert!(scheduler.contains("const SCHEDULER_HUB_STATE_OFFSET = 3480;"));
    assert!(scheduler.contains("const SCHEDULER_TABLE_SIZE = 3488;"));
    assert!(scheduler.contains(
        "mut pointer table = NativePointer(raw_word_load(pointer_add(scheduler, SCHEDULER_HUB_STATE_OFFSET)));"
    ));
    assert!(scheduler.contains("table = SystemAllocate(HUB_TABLE_SIZE, 8);"));
    assert!(scheduler.contains("memory_set(table, 0, HUB_TABLE_SIZE);"));
    assert!(scheduler.contains(
        "raw_word_store(pointer_add(scheduler, SCHEDULER_HUB_STATE_OFFSET), NativeWord(table));"
    ));

    assert!(hub.contains("const HUB_ENTRY_MAX = 256;"));
    assert!(hub.contains("const HUB_SLOT_SIZE = 4112;"));
    assert!(hub.contains("const HUB_TABLE_SIZE = 65800;"));
    assert!(hub.contains("return SchedulerHubTable();"));
    assert!(!hub.contains("RuntimeState()"));
    assert!(!hub.contains("pointer_add(state, 19072)"));
    assert!(hub.contains("if i64(raw_word_load(existingEntry)) == index"));
    assert!(hub.contains("raw_word_store(pointer_add(existingEntry, 8), word(channelId));"));
    assert!(hub.contains("while move + 1 < entryCount"));
    assert!(hub.contains("raw_word_store(pointer_add(hub, 8), cursor % remaining);"));
}
