//! Compiler-known callables (paths, ABI symbols, arity) merged into [`crate::resolve::Resolver`].

use std::collections::HashMap;

use crate::resolve::ItemId;

/// Parameter or return classification for a [`BuiltinSpec`] entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinType {
    String,
    Ptr,
    Usize,
    U64,
    Unit,
    Never,
}

/// One intrinsic or injected runtime entry point visible during resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinSpec {
    pub beskid_path: &'static [&'static str],
    pub runtime_symbol: &'static str,
    pub params: &'static [BuiltinType],
    pub returns: BuiltinType,
    pub injected: bool,
}

#[macro_export]
macro_rules! define_builtins {
    ($($path:expr => {
        symbol: $symbol:literal,
        params: [$($param:ident),* $(,)?],
        returns: $returns:ident,
        injected: $injected:expr $(,)?
    }),* $(,)?) => {
        const BUILTINS: &[$crate::builtins::BuiltinSpec] = &[
            $(
                $crate::builtins::BuiltinSpec {
                    beskid_path: $path,
                    runtime_symbol: $symbol,
                    params: &[$($crate::builtins::BuiltinType::$param),*],
                    returns: $crate::builtins::BuiltinType::$returns,
                    injected: $injected,
                },
            )*
        ];
    };
}

/// All table entries for [`BuiltinSpec`] (from `define_builtins!`).
pub fn builtin_specs() -> &'static [BuiltinSpec] {
    BUILTINS
}

pub fn builtin_for_path(path: &[String]) -> Option<(usize, &'static BuiltinSpec)> {
    for (index, spec) in BUILTINS.iter().enumerate() {
        if path_matches(spec.beskid_path, path) {
            return Some((index, spec));
        }
    }
    None
}

pub fn builtin_for_item(
    builtin_items: &HashMap<ItemId, usize>,
    item_id: ItemId,
) -> Option<&'static BuiltinSpec> {
    builtin_items
        .get(&item_id)
        .and_then(|index| BUILTINS.get(*index))
}

fn path_matches(expected: &[&str], actual: &[String]) -> bool {
    if expected.len() != actual.len() {
        return false;
    }
    expected
        .iter()
        .zip(actual.iter())
        .all(|(left, right)| *left == right)
}

define_builtins! {
    &["__alloc"] => {
        symbol: "alloc",
        params: [Usize, Ptr],
        returns: Ptr,
        injected: true,
    },
    &["__str_new"] => {
        symbol: "str_new",
        params: [Ptr, Usize],
        returns: Ptr,
        injected: true,
    },
    &["__array_new"] => {
        symbol: "array_new",
        params: [Usize, Usize],
        returns: Usize,
        injected: true,
    },
    &["__array_len"] => {
        symbol: "array_len",
        params: [Ptr],
        returns: Usize,
        injected: true,
    },
    &["__panic_str"] => {
        symbol: "panic_str",
        params: [String],
        returns: Never,
        injected: true,
    },
    &["__gc_write_barrier"] => {
        symbol: "gc_write_barrier",
        params: [Ptr, Ptr],
        returns: Unit,
        injected: true,
    },
    &["__gc_root_handle"] => {
        symbol: "gc_root_handle",
        params: [Ptr],
        returns: U64,
        injected: true,
    },
    &["__gc_unroot_handle"] => {
        symbol: "gc_unroot_handle",
        params: [U64],
        returns: Unit,
        injected: true,
    },
    &["__gc_register_root"] => {
        symbol: "gc_register_root",
        params: [Ptr],
        returns: Unit,
        injected: true,
    },
    &["__gc_unregister_root"] => {
        symbol: "gc_unregister_root",
        params: [Ptr],
        returns: Unit,
        injected: true,
    },
    &["__syscall_write"] => {
        symbol: "syscall_write",
        params: [U64, String],
        returns: Usize,
        injected: true,
    },
    &["__syscall_read"] => {
        symbol: "syscall_read",
        params: [U64, U64],
        returns: String,
        injected: true,
    },
    &["__str_len"] => {
        symbol: "str_len",
        params: [String],
        returns: Usize,
        injected: true,
    },
    &["__interop_dispatch_unit"] => {
        symbol: "interop_dispatch_unit",
        params: [Ptr],
        returns: Unit,
        injected: true,
    },
    &["__interop_dispatch_ptr"] => {
        symbol: "interop_dispatch_ptr",
        params: [Ptr],
        returns: Ptr,
        injected: true,
    },
    &["__interop_dispatch_usize"] => {
        symbol: "interop_dispatch_usize",
        params: [Ptr],
        returns: Usize,
        injected: true,
    },
        &["__test_bytes_ptr"] => {
            symbol: "test_bytes_ptr",
            params: [],
            returns: U64,
            injected: true,
        },
        &["__test_bytes_len"] => {
            symbol: "test_bytes_len",
            params: [],
            returns: U64,
            injected: true,
        },
    &["__fiber_spawn"] => {
        symbol: "fiber_spawn",
        params: [Ptr, Ptr],
        returns: U64,
        injected: true,
    },
    &["__fiber_join"] => {
        symbol: "fiber_join_status",
        params: [U64],
        returns: U64,
        injected: true,
    },
    &["__fiber_join_value"] => {
        symbol: "fiber_join_value",
        params: [U64],
        returns: U64,
        injected: true,
    },
    &["__fiber_detach"] => {
        symbol: "fiber_detach",
        params: [U64],
        returns: Unit,
        injected: true,
    },
    &["__fiber_cancel"] => {
        symbol: "fiber_cancel",
        params: [U64],
        returns: Unit,
        injected: true,
    },
    &["__fiber_yield"] => {
        symbol: "fiber_yield",
        params: [],
        returns: Unit,
        injected: true,
    },
    &["__fiber_now_millis"] => {
        symbol: "fiber_now_millis",
        params: [],
        returns: U64,
        injected: true,
    },
    &["__fiber_current_id"] => {
        symbol: "fiber_current_id",
        params: [],
        returns: U64,
        injected: true,
    },
    &["__fiber_processor_count"] => {
        symbol: "fiber_processor_count",
        params: [],
        returns: U64,
        injected: true,
    },
    &["__channel_create"] => {
        symbol: "channel_create",
        params: [U64, U64],
        returns: U64,
        injected: true,
    },
    &["__channel_send"] => {
        symbol: "channel_send",
        params: [U64, U64],
        returns: U64,
        injected: true,
    },
    &["__channel_receive"] => {
        symbol: "channel_receive_status",
        params: [U64],
        returns: U64,
        injected: true,
    },
    &["__channel_receive_value"] => {
        symbol: "channel_receive_value",
        params: [U64],
        returns: U64,
        injected: true,
    },
    &["__channel_try_send"] => {
        symbol: "channel_try_send",
        params: [U64, U64],
        returns: U64,
        injected: true,
    },
    &["__channel_try_receive"] => {
        symbol: "channel_try_receive",
        params: [U64, Ptr],
        returns: U64,
        injected: true,
    },
    &["__channel_close"] => {
        symbol: "channel_close",
        params: [U64],
        returns: Unit,
        injected: true,
    },
    &["__hub_create"] => {
        symbol: "hub_create",
        params: [],
        returns: U64,
        injected: true,
    },
    &["__hub_register"] => {
        symbol: "hub_register",
        params: [U64, U64, U64],
        returns: U64,
        injected: true,
    },
    &["__hub_unregister"] => {
        symbol: "hub_unregister",
        params: [U64, U64],
        returns: U64,
        injected: true,
    },
    &["__hub_wait_receive"] => {
        symbol: "hub_wait_receive_status",
        params: [U64],
        returns: U64,
        injected: true,
    },
    &["__hub_wait_receive_index"] => {
        symbol: "hub_wait_receive_index",
        params: [U64],
        returns: U64,
        injected: true,
    },
    &["__hub_wait_receive_value"] => {
        symbol: "hub_wait_receive_value",
        params: [U64],
        returns: U64,
        injected: true,
    },
    &["__mutex_create"] => {
        symbol: "mutex_create",
        params: [],
        returns: U64,
        injected: true,
    },
    &["__mutex_lock"] => {
        symbol: "mutex_lock",
        params: [U64],
        returns: U64,
        injected: true,
    },
    &["__mutex_try_lock"] => {
        symbol: "mutex_try_lock",
        params: [U64],
        returns: U64,
        injected: true,
    },
    &["__mutex_unlock"] => {
        symbol: "mutex_unlock",
        params: [U64],
        returns: Unit,
        injected: true,
    },
    &["__wait_group_create"] => {
        symbol: "wait_group_create",
        params: [],
        returns: U64,
        injected: true,
    },
    &["__wait_group_add"] => {
        symbol: "wait_group_add",
        params: [U64, U64],
        returns: Unit,
        injected: true,
    },
    &["__wait_group_done"] => {
        symbol: "wait_group_done",
        params: [U64],
        returns: Unit,
        injected: true,
    },
    &["__wait_group_wait"] => {
        symbol: "wait_group_wait",
        params: [U64],
        returns: U64,
        injected: true,
    },
}
