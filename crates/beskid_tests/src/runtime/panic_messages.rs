use beskid_engine::Engine;
use beskid_runtime::{event_subscribe, str_len, str_new};

fn panic_text(result: Result<(), Box<dyn std::any::Any + Send>>) -> String {
    let payload = result.expect_err("expected panic result");
    if let Some(text) = payload.downcast_ref::<&str>() {
        return (*text).to_owned();
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    "<non-string panic payload>".to_owned()
}

#[test]
fn runtime_panic_message_for_null_string_handle_is_stable() {
    let message = panic_text(std::panic::catch_unwind(|| {
        let _ = str_len(std::ptr::null());
    }));
    assert!(
        message.contains("null string handle"),
        "expected null string handle panic message, got: {message}"
    );
}

#[test]
fn runtime_panic_message_for_invalid_utf8_is_stable() {
    let mut engine = Engine::new();
    let message = engine.with_arena(|_, _| {
        panic_text(std::panic::catch_unwind(|| {
            let invalid = [0xffu8];
            let _ = str_new(invalid.as_ptr(), invalid.len());
        }))
    });

    assert!(
        message.contains("invalid utf-8 string data"),
        "expected invalid utf-8 panic message, got: {message}"
    );
}

#[test]
fn runtime_panic_message_for_event_capacity_is_stable() {
    let mut engine = Engine::new();
    let message = engine.with_arena(|_, _| {
        let mut slot = std::ptr::null_mut();
        let _ = event_subscribe(&mut slot, 1usize as *mut u8, 1);

        let text = panic_text(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || {
                let _ = event_subscribe(&mut slot, 2usize as *mut u8, 1);
            },
        )));

        unsafe {
            drop(Box::from_raw(slot));
        }

        text
    });

    assert!(
        message.contains("event capacity exceeded"),
        "expected capacity panic message, got: {message}"
    );
}
