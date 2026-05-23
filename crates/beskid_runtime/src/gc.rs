//! Runtime GC root state and TLS hooks over `abfall`.
//!
//! Phase B (multi-mutator) layers on top of Phase A:
//!
//! - Each OS thread that enters a runtime scope is treated as a Beskid mutator and may allocate
//!   through `alloc` even when other mutators are also active. The shared abfall heap performs
//!   concurrent marking with insertion barriers - see [`crate::builtins::gc_write_barrier`].
//! - Threads in the syscall pool are explicitly tagged via [`set_syscall_pool_worker`]. They must
//!   call [`enter_runtime_scope`] before allocating any Beskid object; otherwise allocations will
//!   panic with a guard diagnostic. This prevents accidental re-entrance of a "second mutator"
//!   from a blocking worker.
//! - [`runtime_phase`] / [`preemption_enabled`] expose optional preemption hooks that callers
//!   (codegen, future schedulers) can poll at function entry to cooperatively yield. Preemption
//!   stays off by default to keep Phase A determinism.

use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

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
    /// `true` on threads owned by the syscall worker pool.
    static IS_SYSCALL_POOL_WORKER: Cell<bool> = const { Cell::new(false) };
}

/// Concurrency phase exposed by the runtime; defaults to [`RuntimePhase::PhaseA`] (single mutator
/// at a time, channels carry `i64` payloads only). Phase B opts into multiple OS-thread mutators
/// against a shared heap and pointer-payload channels with write-barrier instrumentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RuntimePhase {
    /// Single Beskid mutator at a time; legacy behavior.
    PhaseA = 0,
    /// Multiple Beskid mutators may execute concurrently against the same heap.
    PhaseB = 1,
}

impl From<u8> for RuntimePhase {
    fn from(v: u8) -> Self {
        match v {
            1 => Self::PhaseB,
            _ => Self::PhaseA,
        }
    }
}

static RUNTIME_PHASE: AtomicU8 = AtomicU8::new(RuntimePhase::PhaseA as u8);
static RUNTIME_PREEMPT_ENABLED: AtomicBool = AtomicBool::new(false);

/// Current runtime phase. Honors `BESKID_RUNTIME_PHASE_B=1` when first read.
pub fn runtime_phase() -> RuntimePhase {
    init_phase_from_env_once();
    RuntimePhase::from(RUNTIME_PHASE.load(Ordering::Relaxed))
}

/// Override the active runtime phase. Tests and embedders use this to opt-in to Phase B without
/// process-wide environment variables.
pub fn set_runtime_phase(phase: RuntimePhase) {
    RUNTIME_PHASE.store(phase as u8, Ordering::Relaxed);
}

/// Whether the optional function-entry preemption check is enabled. Honors
/// `BESKID_RUNTIME_PREEMPT=1` on first access.
pub fn preemption_enabled() -> bool {
    init_phase_from_env_once();
    RUNTIME_PREEMPT_ENABLED.load(Ordering::Relaxed)
}

/// Toggle the preemption check. When enabled, [`runtime_preempt_check`] yields the current fiber.
pub fn set_preemption_enabled(enabled: bool) {
    RUNTIME_PREEMPT_ENABLED.store(enabled, Ordering::Relaxed);
}

fn init_phase_from_env_once() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if std::env::var("BESKID_RUNTIME_PHASE_B").ok().as_deref() == Some("1") {
            RUNTIME_PHASE.store(RuntimePhase::PhaseB as u8, Ordering::Relaxed);
        }
        if std::env::var("BESKID_RUNTIME_PREEMPT").ok().as_deref() == Some("1") {
            RUNTIME_PREEMPT_ENABLED.store(true, Ordering::Relaxed);
        }
    });
}

/// Tag the current OS thread as belonging to the syscall worker pool. Workers carrying this tag
/// must call [`enter_runtime_scope`] before allocating any Beskid object - see
/// [`assert_mutator_allowed`].
pub fn set_syscall_pool_worker() {
    IS_SYSCALL_POOL_WORKER.with(|c| c.set(true));
}

/// `true` when the calling thread was tagged by [`set_syscall_pool_worker`].
pub fn is_syscall_pool_worker() -> bool {
    IS_SYSCALL_POOL_WORKER.with(|c| c.get())
}

/// Returns `true` when the calling thread holds an active runtime scope (a non-zero
/// `enter_runtime_scope` depth).
pub fn in_runtime_scope() -> bool {
    RUNTIME_SCOPE_DEPTH.with(|c| c.get() > 0)
}

/// Panic when a syscall worker tries to allocate or otherwise act as a Beskid mutator without an
/// active runtime scope. This is the Phase B guard required by the platform-spec
/// `panic-io-and-syscalls` contract: workers parked for blocking IO must never silently re-enter
/// as a second mutator.
#[inline]
pub fn assert_mutator_allowed() {
    if is_syscall_pool_worker() && !in_runtime_scope() {
        panic!(
            "beskid runtime: syscall pool worker attempted to act as a Beskid mutator \
             without calling enter_runtime_scope (Phase B safety guard)"
        );
    }
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
    assert_mutator_allowed();
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
    if is_syscall_pool_worker() && !in_runtime_scope() {
        // Phase B guard: syscall pool workers without an active runtime scope must report "no
        // mutator" rather than dereferencing whatever stale root pointer remains on this thread.
        return None;
    }
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

/// Optional preemption point compiled into generated code (Phase B). The call is always present
/// in the runtime ABI surface; whether it actually yields is controlled by [`set_preemption_enabled`].
///
/// Function-entry sites can call this without measurable cost when preemption is disabled
/// (a single relaxed load).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn runtime_preempt_check() {
    if !preemption_enabled() {
        return;
    }
    if crate::scheduler::in_fiber_scheduler() {
        crate::scheduler::fiber_yield();
    } else {
        std::thread::yield_now();
    }
}

/// Fork the calling OS thread into a Beskid mutator that shares `heap` and uses `root` as its
/// thread-local runtime root. Returns a guard that detaches the mutator on drop. This is the
/// supported way to spawn additional Phase B mutators outside the cooperative fiber scheduler.
pub fn attach_phase_b_mutator(heap: &Arc<Heap>, root: *mut RuntimeRoot) -> MutatorAttachGuard {
    assert!(!root.is_null(), "attach_phase_b_mutator: null root");
    enter_runtime_scope();
    set_current_heap(heap);
    set_current_root(root);
    MutatorAttachGuard { _private: () }
}

/// RAII guard returned by [`attach_phase_b_mutator`]; detaches TLS heap and root state on drop.
pub struct MutatorAttachGuard {
    _private: (),
}

impl Drop for MutatorAttachGuard {
    fn drop(&mut self) {
        clear_current_heap();
        clear_current_root();
        leave_runtime_scope();
    }
}
