//! ABI-surface checks for native composition.

use beskid_abi::generated::abi_v5_contract::ABI_V5_CORELIB_SERVICE_BINDINGS;
use beskid_abi::runtime_source::{CorelibServiceAbiType, canonical_corelib_service_abi_for_adapter};

/// The compiler-planned composition calls (`COMPOSITION_SERVICES` in `beskid_codegen`) with their
/// canonical ABI-v5 shapes from `runtime_manifest.bsol`.
fn composition_calls() -> Vec<(&'static str, Vec<CorelibServiceAbiType>, CorelibServiceAbiType)> {
    use CorelibServiceAbiType::{I32, Pointer, U8, Usize, Void};
    vec![
        ("composition_container_create", vec![Usize], Pointer),
        ("composition_container_drop", vec![Pointer], Void),
        ("composition_launch", vec![Pointer], U8),
        ("composition_scope_enter", vec![Pointer, Usize, Usize], Void),
        ("composition_scope_leave", vec![], Void),
        ("composition_scope_depth", vec![], I32),
        ("composition_shutdown", vec![Pointer], Void),
    ]
}

#[test]
fn composition_runtime_calls_are_canonical_runtime_imports() {
    for (symbol, parameters, result) in composition_calls() {
        assert!(beskid_abi::is_runtime_import_symbol(symbol), "{symbol} must be a canonical runtime import");
        for target in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
            let bindings = ABI_V5_CORELIB_SERVICE_BINDINGS
                .iter()
                .filter(|binding| binding.adapter == symbol && binding.target == target)
                .collect::<Vec<_>>();
            assert_eq!(bindings.len(), 1, "{symbol} needs one exact binding for {target}");
            assert_eq!(bindings[0].implementation, symbol, "{symbol} must be a direct runtime export on {target}");
        }
        let abi = canonical_corelib_service_abi_for_adapter(symbol).unwrap_or_else(|| panic!("{symbol} has no shape"));
        assert_eq!(abi.parameters, parameters, "{symbol} parameters");
        assert_eq!(abi.result, result, "{symbol} result");
    }
}

#[test]
fn retired_composition_entry_points_are_not_runtime_imports() {
    for symbol in ["composition_register", "composition_bind_plural", "composition_resolve", "composition_resolve_plural"] {
        assert!(!beskid_abi::is_runtime_import_symbol(symbol), "{symbol} has no ABI-v5 manifest authority");
        assert!(beskid_abi::is_runtime_owned_ffi_symbol(symbol), "{symbol} stays reserved from user FFI");
    }
}
