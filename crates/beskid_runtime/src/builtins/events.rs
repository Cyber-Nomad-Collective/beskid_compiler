/// Event state holds a bounded list of handler function pointers.
/// In v0.1, duplicates are allowed; capacity is fixed; iteration order is insertion order.
pub struct EventState {
    handlers: Vec<*mut u8>,
    capacity: usize,
}

/// Subscribe a handler to an event, allocating the state on first use.
/// Panics if capacity is 0 or exceeded; returns new length.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn event_subscribe(
    event_slot: *mut *mut EventState,
    handler: *mut u8,
    capacity: usize,
) -> usize {
    if event_slot.is_null() {
        panic!("null event slot");
    }
    if handler.is_null() {
        panic!("null event handler");
    }
    if capacity == 0 {
        panic!("event capacity must be positive");
    }

    unsafe {
        let state_ptr = *event_slot;
        let state = if state_ptr.is_null() {
            let boxed = Box::new(EventState {
                handlers: Vec::with_capacity(capacity),
                capacity,
            });
            let raw = Box::into_raw(boxed);
            *event_slot = raw;
            &mut *raw
        } else {
            &mut *state_ptr
        };

        if state.handlers.len() >= state.capacity {
            panic!("event capacity exceeded");
        }
        state.handlers.push(handler);
        #[cfg(feature = "metrics")]
        {
            use crate::gc::with_current_root;
            with_current_root(|root| {
                root.runtime_state.event_subscribe_calls = root.runtime_state.event_subscribe_calls.saturating_add(1);
            });
        }
        state.handlers.len()
    }
}

/// Unsubscribe first matching handler; returns 1 if removed, 0 otherwise.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn event_unsubscribe_first(
    event_slot: *mut *mut EventState,
    handler: *mut u8,
) -> usize {
    if event_slot.is_null() {
        return 0;
    }
    if handler.is_null() {
        return 0;
    }

    unsafe {
        let state_ptr = *event_slot;
        if state_ptr.is_null() {
            return 0;
        }
        let state = &mut *state_ptr;
        if let Some(idx) = state.handlers.iter().position(|candidate| *candidate == handler) {
            state.handlers.remove(idx);
            #[cfg(feature = "metrics")]
            {
                use crate::gc::with_current_root;
                with_current_root(|root| {
                    root.runtime_state.event_unsubscribe_calls = root.runtime_state.event_unsubscribe_calls.saturating_add(1);
                });
            }
            return 1;
        }
    }
    0
}

/// Return number of subscribed handlers (0 if state is null).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn event_len(state: *mut EventState) -> usize {
    if state.is_null() {
        return 0;
    }
    unsafe { (*state).handlers.len() }
}

/// Return handler at idx or null if out of bounds.
/// In v0.1 this is used by codegen to iterate and invoke handlers.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn event_get_handler(state: *mut EventState, idx: usize) -> *mut u8 {
    if state.is_null() {
        return std::ptr::null_mut();
    }
    let result = unsafe { (&(*state).handlers).get(idx).copied().unwrap_or(std::ptr::null_mut()) };
    #[cfg(feature = "metrics")]
    if !result.is_null() {
        use crate::gc::with_current_root;
        with_current_root(|root| {
            root.runtime_state.event_get_handler_calls = root.runtime_state.event_get_handler_calls.saturating_add(1);
        });
    }
    result
}
