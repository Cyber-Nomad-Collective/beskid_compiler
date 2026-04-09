use beskid_engine::Engine;
use beskid_runtime::{
    alloc, event_get_handler, event_subscribe, event_unsubscribe_first, rt_metrics_alloc_bytes,
    rt_metrics_alloc_calls, rt_metrics_event_get_handler_calls, rt_metrics_event_subscribe_calls,
    rt_metrics_event_unsubscribe_calls, rt_metrics_heap_fragmentation_bytes,
    rt_metrics_heap_live_bytes, rt_metrics_heap_total_bytes, rt_metrics_str_concat_bytes,
    rt_metrics_str_concat_calls, str_concat, str_new,
};

#[test]
fn runtime_metrics_snapshot_counters_increase_for_typical_flow() {
    let mut engine = Engine::new();
    engine.with_arena(|_, _| {
        static Z: [u8; 1] = [0];
        let _ = alloc(16, std::ptr::null());
        let _ = alloc(8, std::ptr::null());

        let left = str_new(b"hello".as_ptr(), 5);
        let right = str_new(Z.as_ptr(), 0);
        let _ = str_concat(left, right);

        let mut slot = std::ptr::null_mut();
        let handler = 7usize as *mut u8;
        let _ = event_subscribe(&mut slot, handler, 4);
        let _ = event_get_handler(slot, 0);
        let _ = event_unsubscribe_first(&mut slot, handler);

        assert!(rt_metrics_alloc_calls() >= 4);
        assert!(rt_metrics_alloc_bytes() >= 24);
        assert!(rt_metrics_str_concat_calls() >= 1);
        assert!(rt_metrics_str_concat_bytes() >= 5);
        assert!(rt_metrics_event_subscribe_calls() >= 1);
        assert!(rt_metrics_event_get_handler_calls() >= 1);
        assert!(rt_metrics_event_unsubscribe_calls() >= 1);
        assert!(rt_metrics_heap_total_bytes() >= rt_metrics_heap_live_bytes());
        assert_eq!(
            rt_metrics_heap_fragmentation_bytes(),
            rt_metrics_heap_total_bytes().saturating_sub(rt_metrics_heap_live_bytes())
        );

        unsafe {
            drop(Box::from_raw(slot));
        }
    });
}
