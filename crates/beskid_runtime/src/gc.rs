//! Runtime GC root state and TLS hooks over `abfall`.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use abfall::{
    GcOptions, Heap, HeapSessionGuard, enter_heap_session,
    with_current_heap as ab_with_current_heap,
};

/// Counters updated by runtime builtins (extended when `metrics` is enabled).
#[derive(Default)]
pub struct RuntimeState {
    pub allocation_counter: usize,
    pub heap_total_bytes: usize,
    pub heap_live_bytes: usize,
    #[cfg(feature = "metrics")]
    pub alloc_calls: usize,
    #[cfg(feature = "metrics")]
    pub alloc_bytes: usize,
    #[cfg(feature = "metrics")]
    pub str_concat_calls: usize,
    #[cfg(feature = "metrics")]
    pub str_concat_bytes: usize,
    #[cfg(feature = "metrics")]
    pub event_subscribe_calls: usize,
    #[cfg(feature = "metrics")]
    pub event_unsubscribe_calls: usize,
    #[cfg(feature = "metrics")]
    pub event_get_handler_calls: usize,
}

/// Runtime root state shared by builtins while an engine scope is active.
pub struct RuntimeRoot {
    pub heap: Arc<Heap>,
    pub runtime_state: RuntimeState,
}

impl RuntimeRoot {
    pub fn new(heap: Arc<Heap>) -> Self {
        Self {
            heap,
            runtime_state: RuntimeState::default(),
        }
    }
}

thread_local! {
    static CURRENT_ROOT: Cell<*mut RuntimeRoot> = const { Cell::new(std::ptr::null_mut()) };
    static CURRENT_HEAP_GUARD: RefCell<Option<HeapSessionGuard>> = const { RefCell::new(None) };
    static RUNTIME_SCOPE_DEPTH: Cell<usize> = const { Cell::new(0) };
}

pub fn enter_runtime_scope() {
    RUNTIME_SCOPE_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
}

pub fn leave_runtime_scope() {
    RUNTIME_SCOPE_DEPTH.with(|depth| {
        let current = depth.get();
        if current == 0 {
            panic!(
                "runtime scope underflow: leave_runtime_scope called without enter_runtime_scope"
            );
        }
        depth.set(current - 1);
    });
}

pub fn set_current_heap(heap: &Arc<Heap>) {
    CURRENT_HEAP_GUARD.with(|guard| {
        *guard.borrow_mut() = Some(enter_heap_session(heap));
    });
}

pub fn clear_current_heap() {
    CURRENT_HEAP_GUARD.with(|guard| {
        *guard.borrow_mut() = None;
    });
}

pub fn with_current_heap<R>(f: impl FnOnce(&Heap) -> R) -> R {
    ab_with_current_heap(f).unwrap_or_else(|| panic!("no active abfall heap session"))
}

pub fn set_current_root(root: *mut RuntimeRoot) {
    CURRENT_ROOT.with(|cell| cell.set(root));
}

pub fn clear_current_root() {
    CURRENT_ROOT.with(|cell| cell.set(std::ptr::null_mut()));
}

pub fn with_current_root<R>(f: impl FnOnce(&mut RuntimeRoot) -> R) -> R {
    CURRENT_ROOT.with(|cell| {
        let ptr = cell.get();
        if ptr.is_null() {
            panic!("no active runtime root");
        }
        // SAFETY: pointer is installed by `Engine::with_runtime` for this thread.
        let root = unsafe { &mut *ptr };
        f(root)
    })
}

pub fn with_current_root_if_active<R>(f: impl FnOnce(&mut RuntimeRoot) -> R) -> Option<R> {
    CURRENT_ROOT.with(|cell| {
        let ptr = cell.get();
        if ptr.is_null() {
            None
        } else {
            // SAFETY: pointer is installed by `Engine::with_runtime` for this thread.
            Some(f(unsafe { &mut *ptr }))
        }
    })
}

pub fn with_current_heap_and_root<R>(f: impl FnOnce(&Heap, &mut RuntimeRoot) -> R) -> R {
    with_current_root(|root| with_current_heap(|heap| f(heap, root)))
}

pub fn store_handle(root: &mut RuntimeRoot, ptr: *mut u8) -> u64 {
    root.heap.external_roots().push_handle(ptr)
}

pub fn drop_handle(root: &mut RuntimeRoot, handle: u64) {
    root.heap.external_roots().drop_handle(handle);
}

pub fn collect_if_needed(root: &mut RuntimeRoot) {
    if root.heap.should_collect() {
        let live = root.heap.force_collect();
        root.runtime_state.heap_live_bytes = live;
    }
}

pub fn beskid_heap_options_for_engine() -> GcOptions {
    GcOptions::beskid_default()
}
