//! Phase B concurrency stress: multiple OS-thread mutators, pointer-payload channels with the
//! insertion write barrier active, and syscall-pool guard verification.
//!
//! The Phase B switch is opted-in per test via [`set_runtime_phase`]; the suite leaves Phase A
//! invariants intact for unrelated tests.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use abfall::{GcOptions, Heap};
use beskid_runtime::{
    MutatorAttachGuard, RuntimePhase, RuntimeRoot, alloc, attach_phase_b_mutator,
    channel_close, channel_create, channel_receive_ptr, channel_send_ptr, clear_current_heap,
    clear_current_root, enter_runtime_scope, gc_collect, gc_register_root, gc_unregister_root,
    in_runtime_scope, is_syscall_pool_worker, leave_runtime_scope, preemption_enabled,
    runtime_phase, runtime_preempt_check, set_current_heap, set_current_root,
    set_preemption_enabled, set_runtime_phase, set_syscall_pool_worker,
    status::STATUS_OK,
};

/// Helper: run a closure inside a freshly attached runtime scope pinned to `heap`.
fn with_scope<R>(heap: &Arc<Heap>, f: impl FnOnce(&mut RuntimeRoot) -> R) -> R {
    let mut root = RuntimeRoot::new(Arc::clone(heap));
    enter_runtime_scope();
    set_current_heap(heap);
    set_current_root(&mut root as *mut _);
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            clear_current_heap();
            clear_current_root();
            leave_runtime_scope();
        }
    }
    let _g = Guard;
    f(&mut root)
}

#[test]
fn phase_b_enables_via_setter() {
    let original = runtime_phase();
    set_runtime_phase(RuntimePhase::PhaseB);
    assert_eq!(runtime_phase(), RuntimePhase::PhaseB);
    set_runtime_phase(original);
}

#[test]
fn preemption_check_is_noop_when_disabled() {
    let was_enabled = preemption_enabled();
    set_preemption_enabled(false);
    // Should not yield, not panic, not block.
    runtime_preempt_check();
    set_preemption_enabled(was_enabled);
}

#[test]
fn preemption_check_yields_when_enabled_off_fiber() {
    let was_enabled = preemption_enabled();
    set_preemption_enabled(true);
    // Off-fiber path falls through to `thread::yield_now` and must not panic.
    runtime_preempt_check();
    set_preemption_enabled(was_enabled);
}

#[test]
fn syscall_pool_worker_without_scope_blocks_alloc() {
    // Simulate a syscall-pool thread that tries to allocate without entering the runtime scope.
    // The Phase B safety guard MUST panic with a descriptive diagnostic rather than silently
    // re-entering the GC as a second mutator. Run the suspect call on a worker thread so the
    // panic does not poison the test process TLS.
    let handle = thread::spawn(|| {
        set_syscall_pool_worker();
        assert!(is_syscall_pool_worker());
        assert!(!in_runtime_scope());
        let result = std::panic::catch_unwind(|| {
            // `alloc` is the canonical mutator path; the guard fires before any heap access.
            alloc(16, std::ptr::null());
        });
        assert!(
            result.is_err(),
            "expected syscall worker without scope to panic on alloc"
        );
        let payload = result.unwrap_err();
        let msg = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&'static str>().map(|s| s.to_string()))
            .unwrap_or_default();
        assert!(
            msg.contains("syscall pool worker") || msg.contains("Phase B safety guard"),
            "panic message should describe Phase B guard, got: {msg}"
        );
    });
    handle.join().expect("worker thread");
}

#[test]
fn syscall_pool_worker_with_runtime_scope_can_allocate() {
    // The guard only blocks workers that did NOT call `enter_runtime_scope`. A worker that
    // explicitly attaches itself as a mutator (callback trampolines, FFI bridges) must be able
    // to allocate normally.
    let heap = Heap::off();
    let arc_heap = Arc::clone(&heap);
    let handle = thread::spawn(move || {
        set_syscall_pool_worker();
        let mut root = RuntimeRoot::new(Arc::clone(&arc_heap));
        let _attach: MutatorAttachGuard = attach_phase_b_mutator(&arc_heap, &mut root as *mut _);
        let ptr = alloc(16, std::ptr::null());
        assert!(!ptr.is_null());
        gc_collect();
    });
    handle.join().expect("worker thread");
    drop(heap);
}

#[test]
fn pointer_channel_round_trip_applies_write_barrier() {
    let heap = Heap::with_options(GcOptions::beskid_test());
    with_scope(&heap, |_| {
        let parent = alloc(32, std::ptr::null());
        // Anchor the parent so concurrent collection cannot reclaim it during the round-trip.
        let mut parent_slot = parent;
        gc_register_root(&mut parent_slot as *mut *mut u8);

        let ch = channel_create(0, 0);
        let value = alloc(64, std::ptr::null());

        // Send through the pointer-payload channel - this registers a GC handle so the value
        // stays alive even if `value` falls out of stack scope on the sender side.
        assert_eq!(channel_send_ptr(ch, value), STATUS_OK);

        // Force a collection while the pointer is in flight. The pointer-payload channel's
        // external root MUST keep the object alive (otherwise the receiver would observe a
        // dangling pointer or the heap would shrink past the live set).
        let live_before = heap.bytes_allocated();
        let _ = heap.force_collect();
        let live_after = heap.bytes_allocated();
        assert!(
            live_after >= live_before / 2,
            "pointer payload should survive collection while in channel: before={live_before} after={live_after}"
        );

        let mut received: *mut u8 = std::ptr::null_mut();
        assert_eq!(channel_receive_ptr(ch, &mut received), STATUS_OK);
        assert_eq!(
            received as usize, value as usize,
            "pointer round-trip should yield the same payload"
        );

        gc_unregister_root(&mut parent_slot as *mut *mut u8);
        channel_close(ch);
    });
}

#[test]
fn pointer_channel_cross_thread_with_phase_b_mutators() {
    // Two OS threads share one heap and exchange GC-managed pointers through a channel. Each
    // thread attaches as its own Phase B mutator, exercising:
    //   - abfall concurrent allocation under contention
    //   - external-root handle table cross-thread visibility (channel queue)
    //   - write barriers fired on send and receive
    let heap = Heap::with_options(GcOptions::beskid_test());
    set_runtime_phase(RuntimePhase::PhaseB);

    let producer_heap = Arc::clone(&heap);
    let consumer_heap = Arc::clone(&heap);

    let received_count = Arc::new(AtomicUsize::new(0));
    let received_count_consumer = Arc::clone(&received_count);

    let channel_id = with_scope(&heap, |_| channel_create(8, 0));

    let producer = thread::Builder::new()
        .name("phase-b-producer".to_string())
        .spawn(move || {
            let mut root = RuntimeRoot::new(Arc::clone(&producer_heap));
            let _attach = attach_phase_b_mutator(&producer_heap, &mut root as *mut _);
            for i in 0..32 {
                let ptr = alloc(16, std::ptr::null());
                assert!(!ptr.is_null(), "producer alloc #{i}");
                let status = channel_send_ptr(channel_id, ptr);
                assert_eq!(status, STATUS_OK, "producer send #{i}");
            }
            channel_close(channel_id);
        })
        .expect("spawn producer");

    let consumer = thread::Builder::new()
        .name("phase-b-consumer".to_string())
        .spawn(move || {
            let mut root = RuntimeRoot::new(Arc::clone(&consumer_heap));
            let _attach = attach_phase_b_mutator(&consumer_heap, &mut root as *mut _);
            loop {
                let mut out: *mut u8 = std::ptr::null_mut();
                let status = channel_receive_ptr(channel_id, &mut out);
                if status != STATUS_OK {
                    break;
                }
                assert!(!out.is_null(), "received pointer should be non-null");
                received_count_consumer.fetch_add(1, Ordering::SeqCst);
                if received_count_consumer.load(Ordering::Relaxed).is_multiple_of(8) {
                    let _ = gc_collect();
                }
            }
        })
        .expect("spawn consumer");

    producer.join().expect("producer join");
    consumer.join().expect("consumer join");

    assert_eq!(received_count.load(Ordering::SeqCst), 32);

    with_scope(&heap, |_| {
        let _ = gc_collect();
    });
    set_runtime_phase(RuntimePhase::PhaseA);
    drop(heap);
}

#[test]
fn phase_b_stress_many_mutators_concurrent_allocations() {
    // Heavier stress: multiple mutator threads allocating and exchanging pointers, with periodic
    // collections. This exercises abfall concurrent marking under multi-mutator load and
    // confirms `gc_write_barrier` keeps in-flight pointers reachable.
    const PRODUCERS: usize = 2;
    const CONSUMERS: usize = 2;
    const ITEMS_PER_PRODUCER: usize = 64;

    let heap = Heap::with_options(GcOptions::beskid_test());
    set_runtime_phase(RuntimePhase::PhaseB);

    let channel_id = with_scope(&heap, |_| channel_create(64, 0));

    let alloc_count = Arc::new(AtomicUsize::new(0));
    let recv_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for idx in 0..PRODUCERS {
        let thread_heap = Arc::clone(&heap);
        let alloc_count = Arc::clone(&alloc_count);
        handles.push(
            thread::Builder::new()
                .name(format!("phase-b-producer-{idx}"))
                .spawn(move || {
                    let mut root = RuntimeRoot::new(Arc::clone(&thread_heap));
                    let _attach = attach_phase_b_mutator(&thread_heap, &mut root as *mut _);
                    for _ in 0..ITEMS_PER_PRODUCER {
                        let ptr = alloc(24, std::ptr::null());
                        alloc_count.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(channel_send_ptr(channel_id, ptr), STATUS_OK);
                    }
                })
                .expect("spawn producer"),
        );
    }
    for idx in 0..CONSUMERS {
        let thread_heap = Arc::clone(&heap);
        let recv_count = Arc::clone(&recv_count);
        handles.push(
            thread::Builder::new()
                .name(format!("phase-b-consumer-{idx}"))
                .spawn(move || {
                    let mut root = RuntimeRoot::new(Arc::clone(&thread_heap));
                    let _attach = attach_phase_b_mutator(&thread_heap, &mut root as *mut _);
                    let target = ITEMS_PER_PRODUCER * PRODUCERS / CONSUMERS;
                    let mut local = 0usize;
                    while local < target {
                        let mut out: *mut u8 = std::ptr::null_mut();
                        let status = channel_receive_ptr(channel_id, &mut out);
                        if status != STATUS_OK {
                            break;
                        }
                        assert!(!out.is_null());
                        recv_count.fetch_add(1, Ordering::SeqCst);
                        local += 1;
                    }
                })
                .expect("spawn consumer"),
        );
    }

    // Give the background GC a chance to mark/sweep while threads are running.
    thread::sleep(Duration::from_millis(20));

    for h in handles {
        h.join().expect("mutator join");
    }
    channel_close(channel_id);

    assert_eq!(
        alloc_count.load(Ordering::SeqCst),
        ITEMS_PER_PRODUCER * PRODUCERS
    );
    assert_eq!(
        recv_count.load(Ordering::SeqCst),
        ITEMS_PER_PRODUCER * PRODUCERS
    );

    with_scope(&heap, |_| {
        let _ = gc_collect();
    });
    set_runtime_phase(RuntimePhase::PhaseA);
    drop(heap);
}
