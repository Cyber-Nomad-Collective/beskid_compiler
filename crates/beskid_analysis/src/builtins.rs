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
    I32,
    F64,
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

include!("generated/builtins.inc.rs");

/// All table entries for [`BuiltinSpec`] (from `define_builtins!`).
pub fn builtin_specs() -> &'static [BuiltinSpec] {
    BUILTINS
}

/// Look up a builtin spec by its Beskid path segments.
pub fn builtin_for_path(path: &[String]) -> Option<(usize, &'static BuiltinSpec)> {
    for (index, spec) in BUILTINS.iter().enumerate() {
        if path_matches(spec.beskid_path, path) {
            return Some((index, spec));
        }
    }
    None
}

/// Look up a builtin spec by its resolved [`ItemId`] and builtin index mapping.
pub fn builtin_for_item(builtin_items: &HashMap<ItemId, usize>, item_id: ItemId) -> Option<&'static BuiltinSpec> {
    builtin_items.get(&item_id).and_then(|index| BUILTINS.get(*index))
}

fn path_matches(expected: &[&str], actual: &[String]) -> bool {
    if expected.len() != actual.len() {
        return false;
    }
    expected.iter().zip(actual.iter()).all(|(left, right)| *left == right)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{BuiltinType, builtin_for_path, builtin_specs};

    #[test]
    fn builtin_paths_are_unique_and_fiber_yield_is_manifest_owned() {
        let mut paths = HashSet::new();
        for spec in builtin_specs() {
            assert!(paths.insert(spec.beskid_path), "duplicate builtin path {:?}", spec.beskid_path);
        }

        let fiber_yield = builtin_specs().iter().find(|spec| spec.beskid_path == ["__fiber_yield"]).unwrap();
        assert_eq!(fiber_yield.runtime_symbol, "beskid_rt_v5_fiber_yield");
    }

    #[test]
    fn foundation_env_set_exposes_the_native_i32_status() {
        let path = ["__env_set".to_owned()];
        let (_, builtin) = builtin_for_path(&path).expect("__env_set builtin");

        assert_eq!(format!("{:?}", builtin.returns), "I32");
    }

    #[test]
    fn fiber_builtins_match_the_manifest_owned_join_and_cancel_contract() {
        let join_status = ["__fiber_join_status".to_owned()];
        let (_, join_status) = builtin_for_path(&join_status).expect("canonical fiber join-status builtin");
        assert_eq!(join_status.runtime_symbol, "fiber_join_status");
        assert_eq!(join_status.params, &[BuiltinType::U64]);
        assert_eq!(join_status.returns, BuiltinType::I32);

        let legacy_join = ["__fiber_join".to_owned()];
        assert!(builtin_for_path(&legacy_join).is_none(), "legacy fiber join spelling must fail closed");

        let cancel = ["__fiber_cancel".to_owned()];
        let (_, cancel) = builtin_for_path(&cancel).expect("canonical fiber cancel builtin");
        assert_eq!(cancel.runtime_symbol, "fiber_cancel");
        assert_eq!(cancel.params, &[BuiltinType::U64, BuiltinType::U64]);
        assert_eq!(cancel.returns, BuiltinType::U64);
    }
}
