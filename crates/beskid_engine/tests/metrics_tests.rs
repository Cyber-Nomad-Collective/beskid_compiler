use anyhow::Result;
use beskid_engine::Engine;
use beskid_runtime::{
    event_get_handler, event_subscribe, event_unsubscribe_first, rt_metrics_event_get_handler_calls,
    rt_metrics_event_subscribe_calls, rt_metrics_event_unsubscribe_calls, rt_metrics_str_concat_bytes,
    rt_metrics_str_concat_calls, str_concat, str_len, str_new,
};

#[test]
fn engine_runtime_metrics_strings_and_events() -> Result<()> {
    let mut engine = Engine::new();
    engine.with_arena(|_, _| {
        // Strings metrics
        static Z: [u8; 1] = [0];
        let empty = str_new(Z.as_ptr(), 0);
        let hello = b"hello";
        let s1 = str_new(hello.as_ptr(), hello.len());
        let out = str_concat(empty, s1);
        assert_eq!(str_len(out), 5);
        assert_eq!(rt_metrics_str_concat_calls(), 1);
        assert_eq!(rt_metrics_str_concat_bytes(), 5);

        // Events metrics
        let mut slot: *mut core::ffi::c_void = core::ptr::null_mut();
        let event_slot = (&mut slot as *mut *mut core::ffi::c_void) as *mut *mut _;
        extern "C" fn h0() {}
        let h0p = h0 as *const () as *mut u8;
        let len1 = event_subscribe(event_slot, h0p, 4);
        assert_eq!(len1, 1);
        assert_eq!(rt_metrics_event_subscribe_calls(), 1);

        let got = event_get_handler(unsafe { *(event_slot as *mut *mut _) }, 0);
        assert!(!got.is_null());
        assert_eq!(rt_metrics_event_get_handler_calls(), 1);

        let removed = event_unsubscribe_first(event_slot, h0p);
        assert_eq!(removed, 1);
        assert_eq!(rt_metrics_event_unsubscribe_calls(), 1);
    });
    Ok(())
}

