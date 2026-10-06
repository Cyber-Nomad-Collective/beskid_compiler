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
    U32,
    I32,
    F64,
    Unit,
    Never,
    I8,
    I16,
    I64,
    U8,
    U16,
    F32,
    Isize,
    Bool,
    Char,
    /// The builtin's single declared source type parameter (`T`). Only source-typed
    /// Corelib value services use it; their runtime transport ABI stays in the manifest.
    TypeParameter,
    /// A managed array of the builtin's single source type parameter (`T[]`).
    TypeParameterArray,
    /// A managed byte array (`u8[]`). Its runtime transport is the array's `(pointer, usize)`
    /// data/length pair, which codegen derives from the managed header at the call boundary.
    Bytes,
}

/// One intrinsic or injected runtime entry point visible during resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinSpec {
    pub beskid_path: &'static [&'static str],
    pub runtime_symbol: &'static str,
    /// Source type parameters. Empty for every non-generic builtin.
    pub type_parameters: &'static [&'static str],
    pub params: &'static [BuiltinType],
    pub returns: BuiltinType,
    pub injected: bool,
}

#[macro_export]
macro_rules! define_builtins {
    ($($path:expr => {
        symbol: $symbol:literal,
        $(type_parameters: [$($type_parameter:literal),* $(,)?],)?
        params: [$($param:ident),* $(,)?],
        returns: $returns:ident,
        injected: $injected:expr $(,)?
    }),* $(,)?) => {
        const BUILTINS: &[$crate::builtins::BuiltinSpec] = &[
            $(
                $crate::builtins::BuiltinSpec {
                    beskid_path: $path,
                    runtime_symbol: $symbol,
                    type_parameters: &[$($($type_parameter),*)?],
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
        assert_eq!(join_status.params, &[BuiltinType::I64]);
        assert_eq!(join_status.returns, BuiltinType::I32);

        let legacy_join = ["__fiber_join".to_owned()];
        assert!(builtin_for_path(&legacy_join).is_none(), "legacy fiber join spelling must fail closed");

        let cancel = ["__fiber_cancel".to_owned()];
        let (_, cancel) = builtin_for_path(&cancel).expect("canonical fiber cancel builtin");
        assert_eq!(cancel.runtime_symbol, "fiber_cancel");
        assert_eq!(cancel.params, &[BuiltinType::I64, BuiltinType::I64]);
        assert_eq!(cancel.returns, BuiltinType::U8);
    }

    #[test]
    fn typed_value_services_are_generic_handle_only_source_signatures() {
        // The traced `[i64, pointer] -> u8` caller-slot transport is the runtime ABI, not the
        // source type: `__x_value<T>(handle)` yields the queued/joined `T` itself.
        for (name, symbol) in [
            ("__fiber_join_value", "fiber_join_value"),
            ("__channel_receive_value", "channel_receive_value"),
            ("__hub_wait_receive_value", "hub_wait_receive_value"),
        ] {
            let (_, spec) = builtin_for_path(&[name.to_owned()]).expect("typed value service builtin");
            assert_eq!(spec.runtime_symbol, symbol);
            assert_eq!(spec.type_parameters, &["T"], "{name} declares one source type parameter");
            assert_eq!(spec.params, &[BuiltinType::I64], "{name} takes only the runtime handle");
            assert_eq!(spec.returns, BuiltinType::TypeParameter, "{name} returns the source value");
        }
    }

    #[test]
    fn managed_source_services_declare_their_source_level_signatures() {
        let (_, utf8) = builtin_for_path(&["__str_from_bytes_utf8".to_owned()]).expect("utf8 decode builtin");
        assert_eq!(utf8.params, &[BuiltinType::Bytes], "decodes one managed byte array");
        assert_eq!(utf8.returns, BuiltinType::String);
        let (_, write) = builtin_for_path(&["__syscall_write_bytes".to_owned()]).expect("byte write builtin");
        assert_eq!(write.params, &[BuiltinType::I32, BuiltinType::Bytes], "native descriptor and managed bytes");
        assert_eq!(write.returns, BuiltinType::I64);
        let (_, array) = builtin_for_path(&["__array_new".to_owned()]).expect("typed array builtin");
        assert_eq!(array.type_parameters, &["T"], "the element type is the source type argument");
        assert_eq!(array.params, &[BuiltinType::Usize], "only the element count");
        assert_eq!(array.returns, BuiltinType::TypeParameterArray);
    }

    #[test]
    fn only_declared_generic_builtins_use_the_type_parameter() {
        for spec in builtin_specs() {
            let uses_parameter = [BuiltinType::TypeParameter, BuiltinType::TypeParameterArray]
                .into_iter()
                .any(|parameter| spec.returns == parameter || spec.params.contains(&parameter));
            if uses_parameter {
                assert_eq!(spec.type_parameters.len(), 1, "{:?} must declare exactly one type parameter", spec.beskid_path);
            } else {
                assert!(spec.type_parameters.is_empty(), "{:?} declares an unused type parameter", spec.beskid_path);
            }
        }
    }
}
