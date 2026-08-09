use beskid_abi::runtime_source::{
    CANONICAL_CHANNEL_SOURCE_PATH, CANONICAL_FIBER_SOURCE_PATH, CANONICAL_MUTEX_SOURCE_PATH,
    CANONICAL_SCHEDULER_CORE_SOURCE_PATH, CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH, canonical_runtime_sources,
};

fn canonical_source(path: &str) -> String {
    canonical_runtime_sources()
        .into_iter()
        .find(|unit| unit.logical_path == path)
        .unwrap_or_else(|| panic!("missing canonical runtime source {path}"))
        .source
}

#[test]
fn canonical_scheduler_owns_native_table_through_runtime_state_scheduler_field() {
    let scheduler = canonical_source(CANONICAL_SCHEDULER_CORE_SOURCE_PATH);
    let storage = canonical_source(CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH);

    assert!(scheduler.contains("const SCHEDULER_STATE_OFFSET = 32;"));
    assert!(storage.contains("const SCHEDULER_CHANNEL_STATE_OFFSET = 3456;"));
    assert!(scheduler.contains("const SCHEDULER_TABLE_SIZE = 3496;"));
    assert!(scheduler.contains("mut pointer table = SchedTable();"));
    assert!(scheduler.contains("return NativePointer(raw_word_load(pointer_add(state, SCHEDULER_STATE_OFFSET)));"));
    assert!(scheduler.contains("table = SystemAllocate(SCHEDULER_TABLE_SIZE, 8);"));
    assert!(scheduler.contains("memory_set(table, 0, SCHEDULER_TABLE_SIZE);"));
    assert!(scheduler.contains("raw_word_store(pointer_add(state, SCHEDULER_STATE_OFFSET), NativeWord(table));"));
    assert!(!scheduler.contains("pointer_add(state, 3072)"));
}

#[test]
fn canonical_channel_storage_is_separately_allocated_and_cannot_alias_scheduler_records() {
    let storage = canonical_source(CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH);
    let channel = canonical_source(CANONICAL_CHANNEL_SOURCE_PATH);

    assert!(storage.contains("const CHANNEL_TABLE_SIZE = 12296;"));
    assert!(storage.contains(
        "mut pointer table = NativePointer(raw_word_load(pointer_add(scheduler, SCHEDULER_CHANNEL_STATE_OFFSET)));"
    ));
    assert!(storage.contains("table = SystemAllocate(CHANNEL_TABLE_SIZE, 8);"));
    assert!(storage.contains("memory_set(table, 0, CHANNEL_TABLE_SIZE);"));
    assert!(
        storage.contains("raw_word_store(pointer_add(scheduler, SCHEDULER_CHANNEL_STATE_OFFSET), NativeWord(table));")
    );
    assert!(channel.contains("return SchedulerChannelTable();"));
    assert!(!channel.contains("RuntimeState()"));
    assert!(!channel.contains("pointer_add(state, 3072)"));
    assert!(!storage.contains("pointer_add(table, 3072)"));
}

#[test]
fn canonical_mutex_storage_is_scheduler_owned_and_cannot_alias_runtime_state() {
    let storage = canonical_source(CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH);
    let mutex = canonical_source(CANONICAL_MUTEX_SOURCE_PATH);

    assert!(storage.contains("const SCHEDULER_MUTEX_STATE_OFFSET = 3464;"));
    assert!(storage.contains("const MUTEX_TABLE_SIZE = 1288;"));
    assert!(storage.contains(
        "mut pointer table = NativePointer(raw_word_load(pointer_add(scheduler, SCHEDULER_MUTEX_STATE_OFFSET)));"
    ));
    assert!(storage.contains("table = SystemAllocate(MUTEX_TABLE_SIZE, 8);"));
    assert!(storage.contains("memory_set(table, 0, MUTEX_TABLE_SIZE);"));
    assert!(
        storage.contains("raw_word_store(pointer_add(scheduler, SCHEDULER_MUTEX_STATE_OFFSET), NativeWord(table));")
    );
    assert!(mutex.contains("return SchedulerMutexTable();"));
    assert!(!mutex.contains("RuntimeState()"));
    assert!(!mutex.contains("pointer_add(state, 1280)"));
}

#[test]
fn canonical_waitgroup_storage_reassigns_only_an_explicitly_mutable_local() {
    let storage = canonical_source(CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH);

    assert!(storage.contains(
        "mut pointer table = NativePointer(raw_word_load(pointer_add(scheduler, SCHEDULER_WAITGROUP_STATE_OFFSET)));"
    ));
    assert!(storage.contains("table = SystemAllocate(WG_TABLE_SIZE, 8);"));
}

#[test]
fn canonical_v5_spawn_export_has_one_owner_and_enqueues_through_scheduler() {
    let scheduler = canonical_source(CANONICAL_SCHEDULER_CORE_SOURCE_PATH);
    let fiber = canonical_source(CANONICAL_FIBER_SOURCE_PATH);

    assert_eq!(scheduler.matches("Symbol:\"beskid_rt_v5_fiber_spawn_with_cancel_slot\"").count(), 1);
    assert_eq!(
        canonical_runtime_sources()
            .iter()
            .map(|unit| unit.source.matches("Symbol:\"beskid_rt_v5_fiber_spawn_with_cancel_slot\"").count())
            .sum::<usize>(),
        1,
    );
    assert!(scheduler.contains("return SchedulerSpawn(entry, environment, cancelledSlot);"));
    assert_eq!(fiber.matches("Symbol:\"fiber_spawn_with_cancel_slot\"").count(), 0);
    assert!(!fiber.contains("pub i64 FiberSpawnWithCancelSlot("));
}
