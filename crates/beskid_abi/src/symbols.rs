//! Stable linker symbol names for runtime exports (must match `#[unsafe(no_mangle)]` functions).

pub const SYM_ABI_VERSION: &str = "beskid_runtime_abi_version";
pub const SYM_ALLOC: &str = "alloc";
pub const SYM_STR_NEW: &str = "str_new";
pub const SYM_STR_CONCAT: &str = "str_concat";
pub const SYM_ARRAY_NEW: &str = "array_new";
pub const SYM_ARRAY_LEN: &str = "array_len";
pub const SYM_PANIC: &str = "panic";
pub const SYM_PANIC_STR: &str = "panic_str";
pub const SYM_SYSCALL_WRITE: &str = "syscall_write";
pub const SYM_SYSCALL_READ: &str = "syscall_read";
pub const SYM_STR_LEN: &str = "str_len";
pub const SYM_GC_BYTES_ALLOCATED: &str = "gc_bytes_allocated";
pub const SYM_GC_OBJECT_COUNT: &str = "gc_object_count";
pub const SYM_GC_PHASE: &str = "gc_phase";
pub const SYM_GC_COLLECT: &str = "gc_collect";
pub const SYM_GC_COLLECT_IF_NEEDED: &str = "gc_collect_if_needed";
pub const SYM_GC_EXTERNAL_ROOT_COUNT: &str = "gc_external_root_count";
pub const SYM_GC_WRITE_BARRIER: &str = "gc_write_barrier";
pub const SYM_GC_ROOT_HANDLE: &str = "gc_root_handle";
pub const SYM_GC_UNROOT_HANDLE: &str = "gc_unroot_handle";
pub const SYM_GC_REGISTER_ROOT: &str = "gc_register_root";
pub const SYM_GC_UNREGISTER_ROOT: &str = "gc_unregister_root";
pub const SYM_EVENT_SUBSCRIBE: &str = "event_subscribe";
pub const SYM_EVENT_UNSUBSCRIBE_FIRST: &str = "event_unsubscribe_first";
pub const SYM_EVENT_LEN: &str = "event_len";
pub const SYM_EVENT_GET_HANDLER: &str = "event_get_handler";
pub const SYM_INTEROP_DISPATCH_UNIT: &str = "interop_dispatch_unit";
pub const SYM_INTEROP_DISPATCH_PTR: &str = "interop_dispatch_ptr";
pub const SYM_INTEROP_DISPATCH_USIZE: &str = "interop_dispatch_usize";
pub const SYM_TEST_BYTES_PTR: &str = "test_bytes_ptr";
pub const SYM_TEST_BYTES_LEN: &str = "test_bytes_len";

pub const SYM_FIBER_SPAWN: &str = "fiber_spawn";
pub const SYM_FIBER_SPAWN_WITH_CANCEL_SLOT: &str = "fiber_spawn_with_cancel_slot";
/// Blocks until the target fiber completes; returns join status only (see `fiber_join_value`).
pub const SYM_FIBER_JOIN: &str = "fiber_join_status";
pub const SYM_FIBER_JOIN_VALUE: &str = "fiber_join_value";
pub const SYM_FIBER_DETACH: &str = "fiber_detach";
pub const SYM_FIBER_CANCEL: &str = "fiber_cancel";
pub const SYM_FIBER_YIELD: &str = "fiber_yield";
pub const SYM_FIBER_NOW_MILLIS: &str = "fiber_now_millis";
pub const SYM_FIBER_CURRENT_ID: &str = "fiber_current_id";
pub const SYM_FIBER_PROCESSOR_COUNT: &str = "fiber_processor_count";

pub const SYM_CHANNEL_CREATE: &str = "channel_create";
pub const SYM_CHANNEL_SEND: &str = "channel_send";
/// Parks until a message is available; returns status without dequeuing (see `channel_receive_value`).
pub const SYM_CHANNEL_RECEIVE: &str = "channel_receive_status";
pub const SYM_CHANNEL_RECEIVE_VALUE: &str = "channel_receive_value";
pub const SYM_CHANNEL_TRY_SEND: &str = "channel_try_send";
pub const SYM_CHANNEL_TRY_RECEIVE: &str = "channel_try_receive";
pub const SYM_CHANNEL_CLOSE: &str = "channel_close";
/// Phase B pointer-payload channel send; applies the insertion write barrier and registers the
/// payload as an external GC root for in-flight tracing.
pub const SYM_CHANNEL_SEND_PTR: &str = "channel_send_ptr";
/// Phase B pointer-payload `try_send`; non-blocking variant of [`SYM_CHANNEL_SEND_PTR`].
pub const SYM_CHANNEL_TRY_SEND_PTR: &str = "channel_try_send_ptr";
/// Phase B pointer-payload channel receive; drops the external root and applies a receiver-side
/// write barrier before storing the pointer through `out_ptr`.
pub const SYM_CHANNEL_RECEIVE_PTR: &str = "channel_receive_ptr";
/// Phase B pointer-payload `try_receive`; non-blocking variant of [`SYM_CHANNEL_RECEIVE_PTR`].
pub const SYM_CHANNEL_TRY_RECEIVE_PTR: &str = "channel_try_receive_ptr";
/// Phase B optional preemption check called at function entry by codegen when preemption is on.
pub const SYM_RUNTIME_PREEMPT_CHECK: &str = "runtime_preempt_check";

pub const SYM_HUB_CREATE: &str = "hub_create";
pub const SYM_HUB_REGISTER: &str = "hub_register";
pub const SYM_HUB_UNREGISTER: &str = "hub_unregister";
/// Parks until a member channel can receive; stores index/value for follow-up builtins.
pub const SYM_HUB_WAIT_RECEIVE: &str = "hub_wait_receive_status";
pub const SYM_HUB_WAIT_RECEIVE_INDEX: &str = "hub_wait_receive_index";
pub const SYM_HUB_WAIT_RECEIVE_VALUE: &str = "hub_wait_receive_value";

pub const SYM_MUTEX_CREATE: &str = "mutex_create";
pub const SYM_MUTEX_LOCK: &str = "mutex_lock";
pub const SYM_MUTEX_TRY_LOCK: &str = "mutex_try_lock";
pub const SYM_MUTEX_UNLOCK: &str = "mutex_unlock";

pub const SYM_WAIT_GROUP_CREATE: &str = "wait_group_create";
pub const SYM_WAIT_GROUP_ADD: &str = "wait_group_add";
pub const SYM_WAIT_GROUP_DONE: &str = "wait_group_done";
pub const SYM_WAIT_GROUP_WAIT: &str = "wait_group_wait";

pub const SYM_COMPOSITION_CONTAINER_CREATE: &str = "composition_container_create";
pub const SYM_COMPOSITION_CONTAINER_DROP: &str = "composition_container_drop";
pub const SYM_COMPOSITION_REGISTER: &str = "composition_register";
pub const SYM_COMPOSITION_BIND_PLURAL: &str = "composition_bind_plural";
pub const SYM_COMPOSITION_LAUNCH: &str = "composition_launch";
pub const SYM_COMPOSITION_SHUTDOWN: &str = "composition_shutdown";
pub const SYM_COMPOSITION_SCOPE_ENTER: &str = "composition_scope_enter";
pub const SYM_COMPOSITION_SCOPE_LEAVE: &str = "composition_scope_leave";
pub const SYM_COMPOSITION_RESOLVE: &str = "composition_resolve";
pub const SYM_COMPOSITION_RESOLVE_PLURAL: &str = "composition_resolve_plural";
pub const SYM_COMPOSITION_SCOPE_DEPTH: &str = "composition_scope_depth";

pub const SYM_BESKID_REGISTER_CALLBACKS: &str = "beskid_register_callbacks";

pub const SYM_DYNAMIC_CELL_CREATE: &str = "dynamic_cell_create";
pub const SYM_DYNAMIC_CELL_WRAP: &str = "dynamic_cell_wrap";
pub const SYM_DYNAMIC_CAST_CHECKED: &str = "dynamic_cast_checked";
pub const SYM_DYNAMIC_MAP_AOT: &str = "dynamic_map_aot";
pub const SYM_DYNAMIC_MAP_FALLBACK: &str = "dynamic_map_fallback";
pub const SYM_DYNAMIC_OBJECT_ALLOC: &str = "dynamic_object_alloc";

/// User-facing FFI layout band for callback registration tables (independent of
/// [`crate::BESKID_RUNTIME_ABI_VERSION`]).
pub const BESKID_USER_FFI_LAYOUT_BAND: u32 = 1;

/// All symbols the JIT builder registers when wiring the Beskid runtime.
pub const RUNTIME_EXPORT_SYMBOLS: &[&str] = &[
    SYM_ABI_VERSION,
    SYM_ALLOC,
    SYM_STR_NEW,
    SYM_STR_CONCAT,
    SYM_STR_LEN,
    SYM_ARRAY_NEW,
    SYM_ARRAY_LEN,
    SYM_PANIC,
    SYM_PANIC_STR,
    SYM_SYSCALL_WRITE,
    SYM_SYSCALL_READ,
    SYM_GC_BYTES_ALLOCATED,
    SYM_GC_OBJECT_COUNT,
    SYM_GC_PHASE,
    SYM_GC_COLLECT,
    SYM_GC_COLLECT_IF_NEEDED,
    SYM_GC_EXTERNAL_ROOT_COUNT,
    SYM_GC_WRITE_BARRIER,
    SYM_GC_ROOT_HANDLE,
    SYM_GC_UNROOT_HANDLE,
    SYM_GC_REGISTER_ROOT,
    SYM_GC_UNREGISTER_ROOT,
    SYM_EVENT_SUBSCRIBE,
    SYM_EVENT_UNSUBSCRIBE_FIRST,
    SYM_EVENT_LEN,
    SYM_EVENT_GET_HANDLER,
    SYM_INTEROP_DISPATCH_UNIT,
    SYM_INTEROP_DISPATCH_PTR,
    SYM_INTEROP_DISPATCH_USIZE,
    SYM_TEST_BYTES_PTR,
    SYM_TEST_BYTES_LEN,
    SYM_FIBER_SPAWN,
    SYM_FIBER_SPAWN_WITH_CANCEL_SLOT,
    SYM_FIBER_JOIN,
    SYM_FIBER_JOIN_VALUE,
    SYM_FIBER_DETACH,
    SYM_FIBER_CANCEL,
    SYM_FIBER_YIELD,
    SYM_FIBER_NOW_MILLIS,
    SYM_FIBER_CURRENT_ID,
    SYM_FIBER_PROCESSOR_COUNT,
    SYM_CHANNEL_CREATE,
    SYM_CHANNEL_SEND,
    SYM_CHANNEL_RECEIVE,
    SYM_CHANNEL_RECEIVE_VALUE,
    SYM_CHANNEL_TRY_SEND,
    SYM_CHANNEL_TRY_RECEIVE,
    SYM_CHANNEL_CLOSE,
    SYM_CHANNEL_SEND_PTR,
    SYM_CHANNEL_TRY_SEND_PTR,
    SYM_CHANNEL_RECEIVE_PTR,
    SYM_CHANNEL_TRY_RECEIVE_PTR,
    SYM_RUNTIME_PREEMPT_CHECK,
    SYM_HUB_CREATE,
    SYM_HUB_REGISTER,
    SYM_HUB_UNREGISTER,
    SYM_HUB_WAIT_RECEIVE,
    SYM_HUB_WAIT_RECEIVE_INDEX,
    SYM_HUB_WAIT_RECEIVE_VALUE,
    SYM_MUTEX_CREATE,
    SYM_MUTEX_LOCK,
    SYM_MUTEX_TRY_LOCK,
    SYM_MUTEX_UNLOCK,
    SYM_WAIT_GROUP_CREATE,
    SYM_WAIT_GROUP_ADD,
    SYM_WAIT_GROUP_DONE,
    SYM_WAIT_GROUP_WAIT,
    SYM_COMPOSITION_CONTAINER_CREATE,
    SYM_COMPOSITION_CONTAINER_DROP,
    SYM_COMPOSITION_REGISTER,
    SYM_COMPOSITION_BIND_PLURAL,
    SYM_COMPOSITION_LAUNCH,
    SYM_COMPOSITION_SHUTDOWN,
    SYM_COMPOSITION_SCOPE_ENTER,
    SYM_COMPOSITION_SCOPE_LEAVE,
    SYM_COMPOSITION_RESOLVE,
    SYM_COMPOSITION_RESOLVE_PLURAL,
    SYM_COMPOSITION_SCOPE_DEPTH,
    SYM_BESKID_REGISTER_CALLBACKS,
    SYM_DYNAMIC_CAST_CHECKED,
    SYM_DYNAMIC_CELL_CREATE,
    SYM_DYNAMIC_CELL_WRAP,
    SYM_DYNAMIC_MAP_AOT,
    SYM_DYNAMIC_MAP_FALLBACK,
    SYM_DYNAMIC_OBJECT_ALLOC,
];
