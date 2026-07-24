use crate::gc::with_current_root;

/// Allocation call counter from `crate::gc::RuntimeState::alloc_calls`.
#[unsafe(no_mangle)]
#[cfg(feature = "metrics")]
pub extern "C-unwind" fn rt_metrics_alloc_calls() -> usize {
    let mut out = 0usize;
    with_current_root(|root| {
        out = root.runtime_state.alloc_calls;
    });
    out
}

/// Bytes requested across `alloc` calls (`crate::gc::RuntimeState::alloc_bytes`).
#[unsafe(no_mangle)]
#[cfg(feature = "metrics")]
pub extern "C-unwind" fn rt_metrics_alloc_bytes() -> usize {
    let mut out = 0usize;
    with_current_root(|root| {
        out = root.runtime_state.alloc_bytes;
    });
    out
}

/// `str_concat` invocation count.
#[unsafe(no_mangle)]
#[cfg(feature = "metrics")]
pub extern "C-unwind" fn rt_metrics_str_concat_calls() -> usize {
    let mut out = 0usize;
    with_current_root(|root| {
        out = root.runtime_state.str_concat_calls;
    });
    out
}

/// Bytes copied by `str_concat`.
#[unsafe(no_mangle)]
#[cfg(feature = "metrics")]
pub extern "C-unwind" fn rt_metrics_str_concat_bytes() -> usize {
    let mut out = 0usize;
    with_current_root(|root| {
        out = root.runtime_state.str_concat_bytes;
    });
    out
}

/// `event_subscribe` invocation count.
#[unsafe(no_mangle)]
#[cfg(feature = "metrics")]
pub extern "C-unwind" fn rt_metrics_event_subscribe_calls() -> usize {
    let mut out = 0usize;
    with_current_root(|root| {
        out = root.runtime_state.event_subscribe_calls;
    });
    out
}

/// `event_unsubscribe_first` invocation count.
#[unsafe(no_mangle)]
#[cfg(feature = "metrics")]
pub extern "C-unwind" fn rt_metrics_event_unsubscribe_calls() -> usize {
    let mut out = 0usize;
    with_current_root(|root| {
        out = root.runtime_state.event_unsubscribe_calls;
    });
    out
}

/// `event_get_handler` invocation count.
#[unsafe(no_mangle)]
#[cfg(feature = "metrics")]
pub extern "C-unwind" fn rt_metrics_event_get_handler_calls() -> usize {
    let mut out = 0usize;
    with_current_root(|root| {
        out = root.runtime_state.event_get_handler_calls;
    });
    out
}

/// Bytes reserved in the arena allocator (`heap_total_bytes`).
#[unsafe(no_mangle)]
#[cfg(feature = "metrics")]
pub extern "C-unwind" fn rt_metrics_heap_total_bytes() -> usize {
    let mut out = 0usize;
    with_current_root(|root| {
        out = root.runtime_state.heap_total_bytes;
    });
    out
}

/// Bytes considered live (`heap_live_bytes`).
#[unsafe(no_mangle)]
#[cfg(feature = "metrics")]
pub extern "C-unwind" fn rt_metrics_heap_live_bytes() -> usize {
    let mut out = 0usize;
    with_current_root(|root| {
        out = root.runtime_state.heap_live_bytes;
    });
    out
}

/// `heap_total_bytes - heap_live_bytes` (saturating).
#[unsafe(no_mangle)]
#[cfg(feature = "metrics")]
pub extern "C-unwind" fn rt_metrics_heap_fragmentation_bytes() -> usize {
    let mut out = 0usize;
    with_current_root(|root| {
        out = root.runtime_state.heap_total_bytes.saturating_sub(root.runtime_state.heap_live_bytes);
    });
    out
}
