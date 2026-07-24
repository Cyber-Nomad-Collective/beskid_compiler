use crate::gc::{collect_if_needed, with_current_heap_and_root};

/// GC-tracked zero-filled allocation; optional `type_desc_ptr` is written unaligned at the start when non-null.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn alloc(size: usize, type_desc_ptr: *const u8) -> *mut u8 {
    with_current_heap_and_root(|heap, root| {
        let ptr = heap.allocate_beskid(size, type_desc_ptr);
        root.runtime_state.allocation_counter += 1;
        let live_bytes = heap.bytes_allocated();
        root.runtime_state.heap_total_bytes = root.runtime_state.heap_total_bytes.saturating_add(size).max(live_bytes);
        root.runtime_state.heap_live_bytes = live_bytes;
        #[cfg(feature = "metrics")]
        {
            root.runtime_state.alloc_calls = root.runtime_state.alloc_calls.saturating_add(1);
            root.runtime_state.alloc_bytes = root.runtime_state.alloc_bytes.saturating_add(size);
        }
        collect_if_needed(root);
        ptr
    })
}
