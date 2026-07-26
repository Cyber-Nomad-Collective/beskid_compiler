use beskid_abi::runtime_source::{
    CANONICAL_BOOTSTRAP_SOURCE_PATH, CANONICAL_FIBER_SOURCE_PATH, CANONICAL_SCHEDULER_SOURCE_PATH,
    canonical_runtime_sources,
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
    let scheduler = canonical_source(CANONICAL_SCHEDULER_SOURCE_PATH);

    assert!(scheduler.contains("const SCHEDULER_STATE_OFFSET = 32;"));
    assert!(scheduler.contains("const SCHEDULER_TABLE_SIZE = 3456;"));
    assert!(scheduler.contains("return NativePointer(raw_word_load(pointer_add(state, SCHEDULER_STATE_OFFSET)));"));
    assert!(scheduler.contains("table = SystemAllocate(SCHEDULER_TABLE_SIZE, 8);"));
    assert!(scheduler.contains("memory_set(table, 0, SCHEDULER_TABLE_SIZE);"));
    assert!(scheduler.contains("raw_word_store(pointer_add(state, SCHEDULER_STATE_OFFSET), NativeWord(table));"));
    assert!(!scheduler.contains("pointer_add(state, 3072)"));
}

#[test]
fn canonical_v5_spawn_export_has_one_owner_and_enqueues_through_scheduler() {
    let bootstrap = canonical_source(CANONICAL_BOOTSTRAP_SOURCE_PATH);
    let fiber = canonical_source(CANONICAL_FIBER_SOURCE_PATH);

    assert_eq!(bootstrap.matches("Symbol:\"beskid_rt_v5_fiber_spawn_with_cancel_slot\"").count(), 1);
    assert!(bootstrap.contains("return SchedulerSpawn(entry, environment, cancelledSlot);"));
    assert_eq!(fiber.matches("Symbol:\"fiber_spawn_with_cancel_slot\"").count(), 0);
    assert!(!fiber.contains("pub i64 FiberSpawnWithCancelSlot("));
}
