use beskid_abi::runtime_source::{
    CANONICAL_CHANNEL_SOURCE_PATH, CANONICAL_FIBER_SOURCE_PATH, CANONICAL_HUB_SOURCE_PATH, CANONICAL_MUTEX_SOURCE_PATH,
    CANONICAL_SCHEDULER_CORE_SOURCE_PATH, CANONICAL_SCHEDULER_EXTERNAL_SOURCE_PATH, CANONICAL_SCHEDULER_LOOP_SOURCE_PATH,
    CANONICAL_SYSCALLS_SOURCE_PATH, CANONICAL_WAITGROUP_SOURCE_PATH, canonical_runtime_sources,
};

#[test]
fn scheduler_lifecycle_sources_exist() {
    let sources = canonical_runtime_sources();
    for path in [CANONICAL_SCHEDULER_CORE_SOURCE_PATH, CANONICAL_FIBER_SOURCE_PATH] {
        assert!(sources.iter().any(|unit| unit.logical_path == path), "canonical runtime source {path} must exist",);
    }
}

/// Main runs as the root fiber: every blocking primitive waits by driving the scheduler through
/// the one root-wait round instead of failing fast outside a fiber (a root `Channel.Receive`
/// once returned WouldBlock, which Corelib reported as Closed). Behaviour is proven by the
/// `Root*` tests in `runtime/beskid/tests/runtime_semantics/src/SchedulerTests.bd`.
#[test]
fn blocking_primitives_wait_through_the_root_wait_round() {
    let sources = canonical_runtime_sources();
    let text = |path: &str| {
        sources
            .iter()
            .find(|unit| unit.logical_path == path)
            .unwrap_or_else(|| panic!("canonical runtime source {path} must exist"))
            .source
            .clone()
    };
    assert!(text(CANONICAL_SCHEDULER_LOOP_SOURCE_PATH).contains("pub unit SchedulerRootWait()"));
    for path in [
        CANONICAL_CHANNEL_SOURCE_PATH,
        CANONICAL_WAITGROUP_SOURCE_PATH,
        CANONICAL_MUTEX_SOURCE_PATH,
        CANONICAL_HUB_SOURCE_PATH,
        CANONICAL_FIBER_SOURCE_PATH,
    ] {
        assert!(text(path).contains("SchedulerRootWait();"), "{path} must park the root caller through SchedulerRootWait");
    }
    let channel = text(CANONICAL_CHANNEL_SOURCE_PATH);
    assert!(
        !channel.contains("if index == FIBER_NONE || !FiberExists(index) { return CHANNEL_STATUS_WOULD_BLOCK; }"),
        "a root channel park must not report WouldBlock"
    );
}

/// External waits (timers, worker I/O, sockets, DNS) register the root caller under the root
/// waiter handle and park it by driving the scheduler, instead of failing with
/// "unsupported" (sleep returned 0, network returned NETWORK_UNSUPPORTED) or spinning on the
/// worker (syscalls) outside a fiber.
#[test]
fn external_waits_register_the_root_caller_as_a_waiter() {
    let sources = canonical_runtime_sources();
    let text = |path: &str| {
        sources
            .iter()
            .find(|unit| unit.logical_path == path)
            .unwrap_or_else(|| panic!("canonical runtime source {path} must exist"))
            .source
            .clone()
    };
    let external = text(CANONICAL_SCHEDULER_EXTERNAL_SOURCE_PATH);
    assert!(external.contains("pub word ExternalWaiterHandle()"));
    assert!(external.contains("ExternalWaitRegister(ExternalWaiterHandle(), 3, deadline)"), "sleep registers the root");
    assert!(text(CANONICAL_SYSCALLS_SOURCE_PATH).contains("ExternalWaitRegister(ExternalWaiterHandle(), operation, -1)"));
    for unit in &sources {
        assert!(
            !unit.source.contains("if current == FIBER_NONE { return NETWORK_UNSUPPORTED; }"),
            "{} must not reject a root network wait",
            unit.logical_path
        );
        assert!(
            !unit.source.contains("ExternalWaitRegister(FiberHandle(current)") || unit.logical_path.contains("Parallel"),
            "{} must register external waits through ExternalWaiterHandle",
            unit.logical_path
        );
    }
}
