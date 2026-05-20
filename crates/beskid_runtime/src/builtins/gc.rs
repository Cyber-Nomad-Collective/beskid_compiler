use abfall::GcPhase;

use crate::gc::with_current_root;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn gc_bytes_allocated() -> usize {
    with_current_root(|root| root.heap.bytes_allocated())
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn gc_object_count() -> usize {
    with_current_root(|root| root.heap.allocation_count())
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn gc_phase() -> usize {
    with_current_root(|root| match root.heap.gc_phase() {
        GcPhase::Idle => 0,
        GcPhase::Marking => 1,
        GcPhase::Sweeping => 2,
    })
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn gc_collect() -> usize {
    with_current_root(|root| {
        let live = root.heap.force_collect();
        root.runtime_state.heap_live_bytes = live;
        root.runtime_state.heap_total_bytes = root.heap.bytes_allocated();
        live
    })
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn gc_collect_if_needed() -> usize {
    with_current_root(|root| {
        if root.heap.should_collect() {
            let live = root.heap.force_collect();
            root.runtime_state.heap_live_bytes = live;
        }
        root.runtime_state.heap_total_bytes = root.heap.bytes_allocated();
        root.heap.bytes_allocated()
    })
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn gc_external_root_count() -> usize {
    with_current_root(|root| root.heap.external_root_count())
}
