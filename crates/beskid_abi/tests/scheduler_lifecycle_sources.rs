use beskid_abi::runtime_source::{
    CANONICAL_FIBER_SOURCE_PATH, CANONICAL_SCHEDULER_CORE_SOURCE_PATH, canonical_runtime_sources,
};

fn canonical_source(path: &str) -> String {
    canonical_runtime_sources()
        .into_iter()
        .find(|unit| unit.logical_path == path)
        .unwrap_or_else(|| panic!("canonical source {path}"))
        .source
}

#[test]
fn canonical_scheduler_owns_lifecycle_statuses_and_never_fakes_a_context_switch() {
    let scheduler = canonical_source(CANONICAL_SCHEDULER_CORE_SOURCE_PATH);
    let fiber = canonical_source(CANONICAL_FIBER_SOURCE_PATH);

    // The only lifecycle owner is the scheduler table.  Fiber 0 has a real
    // record/current-id slot, while no-fiber is represented by a private
    // sentinel rather than a second runtime state object.
    assert!(scheduler.contains("const SCHEDULER_CURRENT_FIBER_OFFSET = 3480;"));
    assert!(scheduler.contains("const SCHEDULER_TABLE_SIZE = 3496;"));
    assert!(scheduler.contains("const FIBER_NONE = 0xFFFF;"));
    assert!(scheduler.contains("FiberSetState(mainIdx, FiberState::Running);"));
    assert!(scheduler.contains("SchedulerSetCurrentFiber(mainIdx);"));

    // Allocation failure remains observable through the scheduler-owned
    // handle as the ABI's exact stack-overflow status (3), without publishing
    // a stack reservation or inventing a second failure registry.
    assert!(scheduler.contains("pub unit FiberAllocationFailed(word index)"));
    assert!(scheduler.contains("JoinOutcome::StackOverflow"));
    assert!(scheduler.contains("if FiberJoinStatusWord(index) != JoinOutcome::StackOverflow"));

    // All public lifecycle calls resolve through those records, including the
    // exact 0-4 join status domain and a fail-closed join-value path.
    assert!(scheduler.contains("pub word FiberJoinStatusWord(word index)"));
    assert!(scheduler.contains("return JoinOutcome::NotDone;"));
    assert!(fiber.contains("pub bool FiberCancel(i64 fiberId)"));
    assert!(fiber.contains("PendingCancelEnqueue(index);"));
    assert!(fiber.contains("pub unit FiberDetach(i64 fiberId)"));
    assert!(scheduler.contains("pub i64 FiberCurrentId()"));
    assert!(fiber.contains("pub i32 FiberJoinStatus(i64 fiberId)"));
    assert!(fiber.contains("pub i64 FiberJoinValue(i64 fiberId)"));
    assert!(fiber.contains("Trap(6, NativePointer(0), 0);"));

    // Lifecycle bookkeeping must not impersonate execution until the target
    // context task supplies the manifest-authorized transfer.
    assert!(scheduler.contains("does not claim a context transfer"));
    assert!(!scheduler.contains("result = i64(entry);"));
    assert!(!scheduler.contains("FiberDone(next, result);"));
}
