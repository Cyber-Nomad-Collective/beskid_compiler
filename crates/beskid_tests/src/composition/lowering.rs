//! Codegen-side checks for the native DI lowering.
//!
//! The runtime tests in [`super::container`] and [`super::host_e2e`] exercise the
//! container API itself. These tests confirm that the codegen feature gate is on and that
//! the ABI surface required by the new lowering exists.

use beskid_abi::{
    BUILTIN_SPECS, SYM_COMPOSITION_BIND_PLURAL, SYM_COMPOSITION_CONTAINER_CREATE,
    SYM_COMPOSITION_CONTAINER_DROP, SYM_COMPOSITION_LAUNCH, SYM_COMPOSITION_REGISTER,
    SYM_COMPOSITION_RESOLVE, SYM_COMPOSITION_RESOLVE_PLURAL, SYM_COMPOSITION_SCOPE_DEPTH,
    SYM_COMPOSITION_SCOPE_ENTER, SYM_COMPOSITION_SCOPE_LEAVE, SYM_COMPOSITION_SHUTDOWN,
};
use beskid_codegen::lowering::composition::with_statement::scope_id_from_name;
use beskid_codegen::lowering::composition_policy::RUNTIME_CONTAINER_LOWERING_ENABLED;

#[test]
fn runtime_container_lowering_gate_is_on() {
    assert!(
        RUNTIME_CONTAINER_LOWERING_ENABLED,
        "v0.3 ships with native DI lowering enabled"
    );
}

#[test]
fn composition_builtin_symbols_are_registered() {
    let required = [
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
    ];
    for sym in required {
        assert!(
            BUILTIN_SPECS.iter().any(|spec| spec.symbol == sym),
            "BUILTIN_SPECS must list {sym}",
        );
        assert!(
            beskid_abi::RUNTIME_EXPORT_SYMBOLS.contains(&sym),
            "RUNTIME_EXPORT_SYMBOLS must list {sym}",
        );
    }
}

#[test]
fn scope_id_from_name_is_deterministic_and_nonzero() {
    let a = scope_id_from_name("request");
    let b = scope_id_from_name("request");
    assert_eq!(a, b);
    assert_ne!(a, 0, "scope ids must not collide with ScopeId::GLOBAL");
    assert_ne!(
        scope_id_from_name("request"),
        scope_id_from_name("session"),
        "different names should hash to different scope ids"
    );
}
