//! Parameter/return classification for builtins shared by codegen (`BUILTIN_SPECS`) and JIT hosts.

use crate::symbols::{
    SYM_ALLOC, SYM_ARRAY_LEN, SYM_ARRAY_NEW, SYM_CHANNEL_CLOSE, SYM_CHANNEL_CREATE,
    SYM_CHANNEL_RECEIVE, SYM_CHANNEL_RECEIVE_PTR, SYM_CHANNEL_RECEIVE_VALUE, SYM_CHANNEL_SEND,
    SYM_CHANNEL_SEND_PTR, SYM_CHANNEL_TRY_RECEIVE, SYM_CHANNEL_TRY_RECEIVE_PTR,
    SYM_CHANNEL_TRY_SEND, SYM_CHANNEL_TRY_SEND_PTR, SYM_FIBER_CANCEL, SYM_FIBER_CURRENT_ID,
    SYM_FIBER_DETACH, SYM_FIBER_JOIN, SYM_FIBER_JOIN_VALUE, SYM_FIBER_NOW_MILLIS,
    SYM_FIBER_PROCESSOR_COUNT, SYM_FIBER_SPAWN, SYM_FIBER_SPAWN_WITH_CANCEL_SLOT, SYM_FIBER_YIELD,
    SYM_GC_BYTES_ALLOCATED, SYM_GC_COLLECT, SYM_GC_COLLECT_IF_NEEDED, SYM_GC_EXTERNAL_ROOT_COUNT,
    SYM_GC_OBJECT_COUNT, SYM_GC_PHASE, SYM_GC_REGISTER_ROOT, SYM_GC_ROOT_HANDLE,
    SYM_GC_UNREGISTER_ROOT, SYM_GC_UNROOT_HANDLE, SYM_GC_WRITE_BARRIER, SYM_HUB_CREATE,
    SYM_HUB_REGISTER, SYM_HUB_UNREGISTER, SYM_HUB_WAIT_RECEIVE, SYM_HUB_WAIT_RECEIVE_INDEX,
    SYM_HUB_WAIT_RECEIVE_VALUE, SYM_INTEROP_DISPATCH_PTR, SYM_INTEROP_DISPATCH_UNIT,
    SYM_INTEROP_DISPATCH_USIZE, SYM_MUTEX_CREATE, SYM_MUTEX_LOCK, SYM_MUTEX_TRY_LOCK,
    SYM_MUTEX_UNLOCK, SYM_PANIC, SYM_PANIC_STR, SYM_RUNTIME_PREEMPT_CHECK, SYM_STR_CONCAT,
    SYM_STR_LEN, SYM_STR_NEW, SYM_SYSCALL_READ, SYM_SYSCALL_WRITE, SYM_TEST_BYTES_LEN,
    SYM_TEST_BYTES_PTR, SYM_WAIT_GROUP_ADD, SYM_WAIT_GROUP_CREATE, SYM_WAIT_GROUP_DONE,
    SYM_WAIT_GROUP_WAIT,
};

/// Scalar kinds used when building Cranelift signatures for builtins (`Ptr` vs fixed `I64`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiParamKind {
    Ptr,
    I64,
}

/// Builtin return slot shape (including [`AbiReturnKind::Never`] for diverging calls).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiReturnKind {
    Void,
    Ptr,
    I64,
    I32,
    Never,
}

/// One importable runtime function: exported `symbol` string and Cranelift ABI shape.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinFnSpec {
    pub symbol: &'static str,
    pub params: &'static [AbiParamKind],
    pub returns: AbiReturnKind,
}

const PTR_PTR: [AbiParamKind; 2] = [AbiParamKind::Ptr, AbiParamKind::Ptr];
const PTR_ONLY: [AbiParamKind; 1] = [AbiParamKind::Ptr];
const I64_ONLY: [AbiParamKind; 1] = [AbiParamKind::I64];
const I64_PTR: [AbiParamKind; 2] = [AbiParamKind::I64, AbiParamKind::Ptr];
const I64_I64: [AbiParamKind; 2] = [AbiParamKind::I64, AbiParamKind::I64];
const I64_I64_I64: [AbiParamKind; 3] = [AbiParamKind::I64, AbiParamKind::I64, AbiParamKind::I64];
const PTR_PTR_PTR: [AbiParamKind; 3] = [AbiParamKind::Ptr, AbiParamKind::Ptr, AbiParamKind::Ptr];

/// Canonical list of builtin imports (alloc, strings, GC hooks, syscalls, test helpers, …).
pub const BUILTIN_SPECS: &[BuiltinFnSpec] = &[
    BuiltinFnSpec {
        symbol: SYM_ALLOC,
        params: &PTR_PTR,
        returns: AbiReturnKind::Ptr,
    },
    BuiltinFnSpec {
        symbol: SYM_STR_NEW,
        params: &PTR_PTR,
        returns: AbiReturnKind::Ptr,
    },
    BuiltinFnSpec {
        symbol: SYM_STR_CONCAT,
        params: &PTR_PTR,
        returns: AbiReturnKind::Ptr,
    },
    BuiltinFnSpec {
        symbol: SYM_ARRAY_NEW,
        params: &PTR_PTR,
        returns: AbiReturnKind::Ptr,
    },
    BuiltinFnSpec {
        symbol: SYM_ARRAY_LEN,
        params: &PTR_ONLY,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_PANIC,
        params: &PTR_PTR,
        returns: AbiReturnKind::Never,
    },
    BuiltinFnSpec {
        symbol: SYM_PANIC_STR,
        params: &PTR_ONLY,
        returns: AbiReturnKind::Never,
    },
    BuiltinFnSpec {
        symbol: SYM_SYSCALL_WRITE,
        params: &I64_PTR,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_SYSCALL_READ,
        params: &I64_I64,
        returns: AbiReturnKind::Ptr,
    },
    BuiltinFnSpec {
        symbol: SYM_STR_LEN,
        params: &PTR_ONLY,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_GC_BYTES_ALLOCATED,
        params: &[],
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_GC_OBJECT_COUNT,
        params: &[],
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_GC_PHASE,
        params: &[],
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_GC_COLLECT,
        params: &[],
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_GC_COLLECT_IF_NEEDED,
        params: &[],
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_GC_EXTERNAL_ROOT_COUNT,
        params: &[],
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_GC_WRITE_BARRIER,
        params: &PTR_PTR,
        returns: AbiReturnKind::Void,
    },
    BuiltinFnSpec {
        symbol: SYM_GC_ROOT_HANDLE,
        params: &PTR_ONLY,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_GC_UNROOT_HANDLE,
        params: &I64_ONLY,
        returns: AbiReturnKind::Void,
    },
    BuiltinFnSpec {
        symbol: SYM_GC_REGISTER_ROOT,
        params: &PTR_ONLY,
        returns: AbiReturnKind::Void,
    },
    BuiltinFnSpec {
        symbol: SYM_GC_UNREGISTER_ROOT,
        params: &PTR_ONLY,
        returns: AbiReturnKind::Void,
    },
    BuiltinFnSpec {
        symbol: SYM_INTEROP_DISPATCH_UNIT,
        params: &PTR_ONLY,
        returns: AbiReturnKind::Void,
    },
    BuiltinFnSpec {
        symbol: SYM_INTEROP_DISPATCH_PTR,
        params: &PTR_ONLY,
        returns: AbiReturnKind::Ptr,
    },
    BuiltinFnSpec {
        symbol: SYM_INTEROP_DISPATCH_USIZE,
        params: &PTR_ONLY,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_TEST_BYTES_PTR,
        params: &[],
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_TEST_BYTES_LEN,
        params: &[],
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_FIBER_SPAWN,
        params: &PTR_PTR,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_FIBER_SPAWN_WITH_CANCEL_SLOT,
        params: &PTR_PTR_PTR,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_FIBER_JOIN,
        params: &I64_ONLY,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_FIBER_JOIN_VALUE,
        params: &I64_ONLY,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_FIBER_DETACH,
        params: &I64_ONLY,
        returns: AbiReturnKind::Void,
    },
    BuiltinFnSpec {
        symbol: SYM_FIBER_CANCEL,
        params: &I64_ONLY,
        returns: AbiReturnKind::Void,
    },
    BuiltinFnSpec {
        symbol: SYM_FIBER_YIELD,
        params: &[],
        returns: AbiReturnKind::Void,
    },
    BuiltinFnSpec {
        symbol: SYM_FIBER_NOW_MILLIS,
        params: &[],
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_FIBER_CURRENT_ID,
        params: &[],
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_FIBER_PROCESSOR_COUNT,
        params: &[],
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_CHANNEL_CREATE,
        params: &I64_I64,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_CHANNEL_SEND,
        params: &I64_I64,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_CHANNEL_RECEIVE,
        params: &I64_ONLY,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_CHANNEL_RECEIVE_VALUE,
        params: &I64_ONLY,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_CHANNEL_TRY_SEND,
        params: &I64_I64,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_CHANNEL_TRY_RECEIVE,
        params: &I64_PTR,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_CHANNEL_CLOSE,
        params: &I64_ONLY,
        returns: AbiReturnKind::Void,
    },
    BuiltinFnSpec {
        symbol: SYM_CHANNEL_SEND_PTR,
        params: &I64_PTR,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_CHANNEL_TRY_SEND_PTR,
        params: &I64_PTR,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_CHANNEL_RECEIVE_PTR,
        params: &I64_PTR,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_CHANNEL_TRY_RECEIVE_PTR,
        params: &I64_PTR,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_RUNTIME_PREEMPT_CHECK,
        params: &[],
        returns: AbiReturnKind::Void,
    },
    BuiltinFnSpec {
        symbol: SYM_HUB_CREATE,
        params: &[],
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_HUB_REGISTER,
        params: &I64_I64_I64,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_HUB_UNREGISTER,
        params: &I64_I64,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_HUB_WAIT_RECEIVE,
        params: &I64_ONLY,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_HUB_WAIT_RECEIVE_INDEX,
        params: &I64_ONLY,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_HUB_WAIT_RECEIVE_VALUE,
        params: &I64_ONLY,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_MUTEX_CREATE,
        params: &[],
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_MUTEX_LOCK,
        params: &I64_ONLY,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_MUTEX_TRY_LOCK,
        params: &I64_ONLY,
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_MUTEX_UNLOCK,
        params: &I64_ONLY,
        returns: AbiReturnKind::Void,
    },
    BuiltinFnSpec {
        symbol: SYM_WAIT_GROUP_CREATE,
        params: &[],
        returns: AbiReturnKind::I64,
    },
    BuiltinFnSpec {
        symbol: SYM_WAIT_GROUP_ADD,
        params: &I64_I64,
        returns: AbiReturnKind::Void,
    },
    BuiltinFnSpec {
        symbol: SYM_WAIT_GROUP_DONE,
        params: &I64_ONLY,
        returns: AbiReturnKind::Void,
    },
    BuiltinFnSpec {
        symbol: SYM_WAIT_GROUP_WAIT,
        params: &I64_ONLY,
        returns: AbiReturnKind::I64,
    },
];
