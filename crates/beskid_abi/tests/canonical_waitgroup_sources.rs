use beskid_abi::runtime_source::{
    CANONICAL_SCHEDULER_CORE_SOURCE_PATH, CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH, CANONICAL_WAITGROUP_SOURCE_PATH,
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
fn canonical_waitgroup_storage_is_scheduler_owned_and_wakes_each_registered_waiter() {
    let scheduler = canonical_source(CANONICAL_SCHEDULER_CORE_SOURCE_PATH);
    let storage = canonical_source(CANONICAL_SCHEDULER_STORAGE_SOURCE_PATH);
    let wait_group = canonical_source(CANONICAL_WAITGROUP_SOURCE_PATH);

    assert!(scheduler.contains("const FIBER_TABLE_MAX = 16;"));
    assert!(scheduler.contains("const SCHEDULER_TABLE_SIZE = 3496;"));
    assert!(storage.contains("const SCHEDULER_WAITGROUP_STATE_OFFSET = 3472;"));
    assert!(storage.contains("const WG_TABLE_SIZE = 2312;"));
    assert!(storage.contains("table = SystemAllocate(WG_TABLE_SIZE, 8);"));
    assert!(storage.contains("memory_set(table, 0, WG_TABLE_SIZE);"));
    assert!(
        storage
            .contains("raw_word_store(pointer_add(scheduler, SCHEDULER_WAITGROUP_STATE_OFFSET), NativeWord(table));")
    );

    assert!(wait_group.contains("const WG_WAITER_MAX = 16;"));
    assert!(wait_group.contains("const WG_SLOT_SIZE = 144;"));
    assert!(wait_group.contains("return SchedulerWaitGroupTable();"));
    assert!(wait_group.contains("pub unit WaitGroupWakeAll(pointer slot)"));
    assert!(wait_group.contains("word waiterHandle = FiberHandle(currentFiber);"));
    assert!(wait_group.contains("if FiberHandleValid(waiterHandle)"));
    assert!(wait_group.contains("if FiberParked(waiterIndex) { WakeEnqueue(waiterIndex); }"));
    assert!(wait_group.contains("WaitGroupWaiterRemove(slot, waiterHandle);"));
    assert!(wait_group.contains("raw_word_store(pointer_add(slot, 16 + write * 8), candidate);"));
    assert!(!wait_group.contains("WakeEnqueue(waiter);"));
    assert!(wait_group.contains("WaitGroupWakeAll(slot);"));
    assert!(!wait_group.contains("RuntimeState()"));
    assert!(!wait_group.contains("pointer_add(state, 2560)"));
}
