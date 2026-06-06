//! C ABI surface for the native dependency-injection [`RuntimeContainer`].
//!
//! Codegen lowers the language-meta `launch` / `with` / `inject` surface to calls into the
//! functions defined here; the spec for each function lives in
//! [`beskid_abi::builtins::BUILTIN_SPECS`] under the `composition_*` symbol names.
//!
//! All entry points are `extern "C-unwind"` so Beskid-side panics can propagate cleanly.
//! Container pointers handed back to generated code are stable for the lifetime of one
//! `launch` / `shutdown` pair.

use crate::composition::{
    ContainerError, Lifetime, RegistrationId, RegistrationRecord, RuntimeContainer, ScopeId,
};

const ABI_OK: i32 = 0;
const NULL_INSTANCE: *mut std::ffi::c_void = std::ptr::null_mut();

fn container_from_ptr<'a>(ptr: *mut RuntimeContainer) -> Option<&'a mut RuntimeContainer> {
    if ptr.is_null() {
        None
    } else {
        // SAFETY: codegen only passes pointers handed out by `composition_container_create`
        // and only on the single mutator thread (Phase A). The pointer is valid until the
        // matching `composition_container_drop`.
        Some(unsafe { &mut *ptr })
    }
}

/// Allocate a new container. Returns a heap-owned `*mut RuntimeContainer`; the caller must
/// release it with [`composition_container_drop`].
#[unsafe(no_mangle)]
pub extern "C-unwind" fn composition_container_create() -> *mut RuntimeContainer {
    Box::into_raw(Box::new(RuntimeContainer::new()))
}

/// Release a container allocated with [`composition_container_create`]. Safe to call with
/// a null pointer (no-op).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn composition_container_drop(ptr: *mut RuntimeContainer) {
    if !ptr.is_null() {
        // SAFETY: matches a `Box::into_raw` from `composition_container_create`.
        drop(unsafe { Box::from_raw(ptr) });
    }
}

/// Register a service. `lifetime` uses the [`Lifetime`] ABI constants
/// (`Scoped=0`, `Single=1`, `Transient=2`). No factory or lifecycle hooks are attached;
/// codegen-emitted closures live on the Rust side for now and are provided by host fixtures
/// using [`RuntimeContainer::register`] directly.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn composition_register(
    container: *mut RuntimeContainer,
    registration_id: u32,
    scope_id: u32,
    lifetime: i32,
) -> i32 {
    let Some(container) = container_from_ptr(container) else {
        return ContainerError::ABI_NOT_ACTIVE;
    };
    let Some(lifetime) = Lifetime::from_abi(lifetime) else {
        return ContainerError::ABI_UNKNOWN_REGISTRATION;
    };
    container.register(RegistrationRecord {
        id: RegistrationId(registration_id),
        scope: ScopeId(scope_id),
        lifetime,
        factory: None,
        init: None,
        dispose: None,
    });
    ABI_OK
}

/// Bind a plural-inject site (`[Inject] field: T[]`) to a list of target registrations.
///
/// `targets` is read as `targets_len` consecutive `u32` registration ids. Passing
/// `targets_len == 0` clears the binding for `owner`.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn composition_bind_plural(
    container: *mut RuntimeContainer,
    owner_registration_id: u32,
    targets: *const u32,
    targets_len: i64,
) -> i32 {
    let Some(container) = container_from_ptr(container) else {
        return ContainerError::ABI_NOT_ACTIVE;
    };
    let len = if targets_len < 0 {
        0
    } else {
        targets_len as usize
    };
    let slice: &[u32] = if len == 0 || targets.is_null() {
        &[]
    } else {
        // SAFETY: the codegen-emitted call site guarantees `targets` points at a contiguous
        // region of `len` u32 values, owned by the generated code for the duration of the
        // call.
        unsafe { std::slice::from_raw_parts(targets, len) }
    };
    let mapped: Vec<RegistrationId> = slice.iter().copied().map(RegistrationId).collect();
    container.bind_plural(RegistrationId(owner_registration_id), mapped);
    ABI_OK
}

/// Activate the launched host: push the global scope and initialize eager singletons.
/// Returns `0` on success or one of the `ContainerError::ABI_*` codes.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn composition_launch(container: *mut RuntimeContainer) -> i32 {
    let Some(container) = container_from_ptr(container) else {
        return ContainerError::ABI_NOT_ACTIVE;
    };
    match container.launch() {
        Ok(_) => ABI_OK,
        Err(err) => err.to_abi(),
    }
}

/// Shut the container down: dispose every active scope in LIFO order, then the global scope.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn composition_shutdown(container: *mut RuntimeContainer) -> i32 {
    let Some(container) = container_from_ptr(container) else {
        return ContainerError::ABI_NOT_ACTIVE;
    };
    match container.shutdown() {
        Ok(()) => ABI_OK,
        Err(err) => err.to_abi(),
    }
}

/// Push a new active scope on top of the container's scope stack.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn composition_scope_enter(
    container: *mut RuntimeContainer,
    scope_id: u32,
) -> i32 {
    let Some(container) = container_from_ptr(container) else {
        return ContainerError::ABI_NOT_ACTIVE;
    };
    match container.enter_scope(ScopeId(scope_id)) {
        Ok(()) => ABI_OK,
        Err(err) => err.to_abi(),
    }
}

/// Pop the active scope, running every `dispose` hook in reverse registration order.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn composition_scope_leave(
    container: *mut RuntimeContainer,
    scope_id: u32,
) -> i32 {
    let Some(container) = container_from_ptr(container) else {
        return ContainerError::ABI_NOT_ACTIVE;
    };
    match container.leave_scope(ScopeId(scope_id)) {
        Ok(()) => ABI_OK,
        Err(err) => err.to_abi(),
    }
}

/// Resolve a single registration in the active scope chain. Returns the instance pointer or
/// a null pointer if resolution fails.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn composition_resolve(
    container: *mut RuntimeContainer,
    registration_id: u32,
) -> *mut std::ffi::c_void {
    let Some(container) = container_from_ptr(container) else {
        return NULL_INSTANCE;
    };
    match container.resolve(RegistrationId(registration_id)) {
        Ok(ptr) => ptr,
        Err(_) => NULL_INSTANCE,
    }
}

/// Resolve a plural inject binding. Writes the resolved instances into `out` (capped at
/// `out_capacity`) and returns the number of bound targets (which may exceed `out_capacity`).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn composition_resolve_plural(
    container: *mut RuntimeContainer,
    owner_registration_id: u32,
    out: *mut *mut std::ffi::c_void,
    out_capacity: i64,
) -> i64 {
    let Some(container) = container_from_ptr(container) else {
        return -1;
    };
    let instances = match container.resolve_plural(RegistrationId(owner_registration_id)) {
        Ok(values) => values,
        Err(_) => return -1,
    };
    let capacity = if out_capacity < 0 {
        0
    } else {
        out_capacity as usize
    };
    if !out.is_null() && capacity > 0 {
        let to_copy = capacity.min(instances.len());
        // SAFETY: the caller guarantees `out` points to valid, aligned memory with room for
        // `out_capacity` pointers; we copy at most `to_copy ≤ capacity` entries.
        unsafe {
            std::ptr::copy_nonoverlapping(instances.as_ptr(), out, to_copy);
        }
    }
    instances.len() as i64
}

/// Diagnostic helper: returns the current depth of the active scope stack.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn composition_scope_depth(container: *mut RuntimeContainer) -> i64 {
    let Some(container) = container_from_ptr(container) else {
        return -1;
    };
    container.scope_depth() as i64
}
