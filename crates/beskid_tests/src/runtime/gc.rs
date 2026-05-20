use beskid_engine::Engine;
use beskid_runtime::{
    alloc, gc_bytes_allocated, gc_collect, gc_external_root_count, gc_object_count, gc_phase,
    snapshot_gc,
};

#[test]
fn engine_runtime_scope_exposes_gc_snapshot() {
    let mut engine = Engine::new();
    engine.with_runtime(|heap, _| {
        let _ptr = alloc(40, std::ptr::null());
        let snapshot = snapshot_gc().expect("engine runtime scope should expose GC stats");

        assert_eq!(snapshot.bytes_allocated, heap.bytes_allocated());
        assert_eq!(snapshot.object_count, heap.allocation_count());
        assert_eq!(snapshot.phase as usize, gc_phase());
        assert_eq!(snapshot.external_root_count, gc_external_root_count());
    });
}

#[test]
fn gc_builtins_are_available_through_engine_runtime_scope() {
    let mut engine = Engine::new();
    engine.with_runtime(|heap, _| {
        let _ptr = alloc(24, std::ptr::null());
        assert_eq!(gc_bytes_allocated(), heap.bytes_allocated());
        assert_eq!(gc_object_count(), heap.allocation_count());
        assert_eq!(gc_collect(), heap.bytes_allocated());
    });

    let snapshot = engine
        .gc_snapshot()
        .expect("engine helper should create a runtime scope for snapshots");
    assert_eq!(snapshot.phase as usize, 0);
}
