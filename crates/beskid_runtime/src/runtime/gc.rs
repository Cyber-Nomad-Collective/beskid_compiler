use std::fmt;

use abfall::GcPhase;

use crate::gc::with_current_root_if_active;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcSnapshot {
    pub bytes_allocated: usize,
    pub object_count: usize,
    pub phase: GcPhase,
    pub collection_threshold: usize,
    pub external_root_count: usize,
    pub heap_live_bytes: usize,
    pub heap_total_bytes: usize,
}

impl fmt::Display for GcSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "gc bytes={} objects={} phase={:?} threshold={} roots={}",
            self.bytes_allocated,
            self.object_count,
            self.phase,
            self.collection_threshold,
            self.external_root_count
        )
    }
}

pub fn snapshot_gc() -> Option<GcSnapshot> {
    with_current_root_if_active(|root| {
        let stats = root.heap.stats();
        GcSnapshot {
            bytes_allocated: stats.bytes_allocated,
            object_count: stats.object_count,
            phase: stats.phase,
            collection_threshold: stats.collection_threshold,
            external_root_count: stats.external_root_count,
            heap_live_bytes: root.runtime_state.heap_live_bytes,
            heap_total_bytes: root.runtime_state.heap_total_bytes,
        }
    })
}

pub fn force_collect() -> Option<usize> {
    with_current_root_if_active(|root| {
        let live = root.heap.force_collect();
        root.runtime_state.heap_live_bytes = live;
        root.runtime_state.heap_total_bytes = root.heap.bytes_allocated();
        live
    })
}

pub fn collect_if_needed() -> Option<usize> {
    with_current_root_if_active(|root| {
        if root.heap.should_collect() {
            let live = root.heap.force_collect();
            root.runtime_state.heap_live_bytes = live;
        }
        root.runtime_state.heap_total_bytes = root.heap.bytes_allocated();
        root.runtime_state.heap_live_bytes
    })
}

pub fn write_barrier(parent: *mut u8, child: *mut u8) -> Option<()> {
    with_current_root_if_active(|root| root.heap.write_barrier(parent, child))
}
