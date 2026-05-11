//! GC arena root, TLS helpers, and ephemeral handle storage used by allocation builtins.

use std::cell::Cell;

use gc_arena::{Collect, DynamicRootSet, Gc, Mutation};

/// Counters and root lists updated by runtime builtins (extended when `metrics` is enabled).
#[derive(Default)]
pub struct RuntimeState {
    pub allocation_counter: usize,
    pub handles: Vec<*mut u8>,
    pub registered_roots: Vec<*mut *mut u8>,
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

unsafe impl<'gc> Collect<'gc> for RuntimeState {
    fn trace<T: gc_arena::collect::Trace<'gc>>(&self, _: &mut T) {}
}

/// Opaque byte backing for a [`Gc`] allocation (not traced; treated as raw bytes).
pub struct RawAllocation {
    pub data: Box<[u8]>,
}

unsafe impl<'gc> Collect<'gc> for RawAllocation {
    fn trace<T: gc_arena::collect::Trace<'gc>>(&self, _: &mut T) {}
}

/// Per-arena root: tracked allocations, dynamic roots, and [`RuntimeState`].
#[derive(Collect)]
#[collect(no_drop)]
pub struct RuntimeRoot<'gc> {
    pub globals: Vec<Gc<'gc, RawAllocation>>,
    pub dynamic_roots: DynamicRootSet<'gc>,
    pub runtime_state: RuntimeState,
}

thread_local! {
    static CURRENT_MUTATION: Cell<*mut Mutation<'static>> = Cell::new(std::ptr::null_mut());
    static CURRENT_ROOT: Cell<*mut RuntimeRoot<'static>> = Cell::new(std::ptr::null_mut());
    static RUNTIME_SCOPE_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Increment runtime TLS nesting depth; panics on re-entrant nested scopes.
pub fn enter_runtime_scope() {
    RUNTIME_SCOPE_DEPTH.with(|depth| {
        let current = depth.get();
        if current > 0 {
            panic!("runtime reentrancy violation: nested arena scope is not supported");
        }
        depth.set(current + 1);
    });
}

/// Pair with [`enter_runtime_scope`]; panics if depth was zero.
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

/// Install the active arena mutation pointer used by [`with_current_mutation`] and allocation builtins.
pub fn set_current_mutation(mc: *mut Mutation<'_>) {
    let ptr = mc as *mut Mutation<'static>;
    CURRENT_MUTATION.with(|cell| cell.set(ptr));
}

/// Clear the TLS mutation pointer (typically when leaving an engine scope).
pub fn clear_current_mutation() {
    CURRENT_MUTATION.with(|cell| cell.set(std::ptr::null_mut()));
}

/// Run `f` with the current arena [`Mutation`]; panics if none is installed.
pub fn with_current_mutation<R>(f: impl FnOnce(&Mutation<'_>) -> R) -> R {
    CURRENT_MUTATION.with(|cell| {
        let ptr = cell.get();
        if ptr.is_null() {
            panic!("no active gc-arena mutation");
        }
        let mutation = unsafe { &*ptr };
        f(mutation)
    })
}

/// Install the active [`RuntimeRoot`] pointer for builtins that touch runtime state.
pub fn set_current_root(root: *mut RuntimeRoot<'_>) {
    let ptr = root as *mut RuntimeRoot<'static>;
    CURRENT_ROOT.with(|cell| cell.set(ptr));
}

/// Clear the TLS root pointer.
pub fn clear_current_root() {
    CURRENT_ROOT.with(|cell| cell.set(std::ptr::null_mut()));
}

/// Run `f` with the current [`RuntimeRoot`]; panics if none is installed.
pub fn with_current_root<R>(f: impl FnOnce(&mut RuntimeRoot<'_>) -> R) -> R {
    CURRENT_ROOT.with(|cell| {
        let ptr = cell.get();
        if ptr.is_null() {
            panic!("no active gc-arena root");
        }
        let root = unsafe { &mut *ptr };
        f(root)
    })
}

/// Run `f` with both current mutation and root; panics if either TLS slot is unset.
pub fn with_current_mutation_and_root<R>(
    f: impl for<'gc> FnOnce(&'gc Mutation<'gc>, &'gc mut RuntimeRoot<'gc>) -> R,
) -> R {
    let mutation_ptr = CURRENT_MUTATION.with(|cell| cell.get());
    if mutation_ptr.is_null() {
        panic!("no active gc-arena mutation");
    }
    let root_ptr = CURRENT_ROOT.with(|cell| cell.get());
    if root_ptr.is_null() {
        panic!("no active gc-arena root");
    }
    unsafe {
        f(
            &*(mutation_ptr as *const Mutation<'_>),
            &mut *(root_ptr as *mut RuntimeRoot<'_>),
        )
    }
}

/// Append `ptr` to `root.runtime_state.handles` and return its index as a handle id.
pub fn store_handle(root: &mut RuntimeRoot<'_>, ptr: *mut u8) -> u64 {
    let index = root.runtime_state.handles.len();
    root.runtime_state.handles.push(ptr);
    index as u64
}

/// Null out the handle slot at `handle` if in range (used by [`crate::builtins::gc_unroot_handle`]).
pub fn drop_handle(root: &mut RuntimeRoot<'_>, handle: u64) {
    if let Some(slot) = root.runtime_state.handles.get_mut(handle as usize) {
        *slot = std::ptr::null_mut();
    }
}
