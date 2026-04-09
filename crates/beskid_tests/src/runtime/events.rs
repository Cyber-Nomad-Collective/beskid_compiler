use beskid_engine::Engine;
use beskid_runtime::builtins::EventState;
use beskid_runtime::{event_get_handler, event_len, event_subscribe, event_unsubscribe_first};

#[test]
fn runtime_event_helpers_subscribe_and_iterate_handlers() {
    let mut engine = Engine::new();
    engine.with_arena(|_, _| {
        let mut slot: *mut EventState = std::ptr::null_mut();
        let handler_a = 1usize as *mut u8;
        let handler_b = 2usize as *mut u8;

        let len_after_first = event_subscribe(&mut slot, handler_a, 4);
        let len_after_second = event_subscribe(&mut slot, handler_b, 4);

        assert_eq!(len_after_first, 1);
        assert_eq!(len_after_second, 2);
        assert_eq!(event_len(slot), 2);
        assert_eq!(event_get_handler(slot, 0), handler_a);
        assert_eq!(event_get_handler(slot, 1), handler_b);

        unsafe {
            drop(Box::from_raw(slot));
        }
    });
}

#[test]
fn runtime_event_helpers_unsubscribe_first_match_only() {
    let mut engine = Engine::new();
    engine.with_arena(|_, _| {
        let mut slot: *mut EventState = std::ptr::null_mut();
        let handler_a = 3usize as *mut u8;
        let handler_b = 4usize as *mut u8;

        event_subscribe(&mut slot, handler_a, 4);
        event_subscribe(&mut slot, handler_a, 4);
        event_subscribe(&mut slot, handler_b, 4);

        let removed = event_unsubscribe_first(&mut slot, handler_a);

        assert_eq!(removed, 1);
        assert_eq!(event_len(slot), 2);
        assert_eq!(event_get_handler(slot, 0), handler_a);
        assert_eq!(event_get_handler(slot, 1), handler_b);

        unsafe {
            drop(Box::from_raw(slot));
        }
    });
}

#[test]
fn runtime_event_helpers_reject_capacity_overflow() {
    let mut engine = Engine::new();
    engine.with_arena(|_, _| {
        let mut slot: *mut EventState = std::ptr::null_mut();
        let handler_a = 5usize as *mut u8;
        let handler_b = 6usize as *mut u8;

        event_subscribe(&mut slot, handler_a, 1);
        let overflow = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = event_subscribe(&mut slot, handler_b, 1);
        }));

        assert!(
            overflow.is_err(),
            "expected overflowing event subscription to panic"
        );

        unsafe {
            drop(Box::from_raw(slot));
        }
    });
}
