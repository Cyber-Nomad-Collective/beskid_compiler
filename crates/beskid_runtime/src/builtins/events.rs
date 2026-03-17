pub struct EventState {
    handlers: Vec<*mut u8>,
    capacity: usize,
}

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
        state.handlers.len()
    }
}

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
            return 1;
        }
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn event_len(state: *mut EventState) -> usize {
    if state.is_null() {
        return 0;
    }
    unsafe { (*state).handlers.len() }
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn event_get_handler(state: *mut EventState, idx: usize) -> *mut u8 {
    if state.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        (&(*state).handlers)
            .get(idx)
            .copied()
            .unwrap_or(std::ptr::null_mut())
    }
}
