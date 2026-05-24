//! ABI surface checks for dynamic builtins.

use beskid_abi::{
    BUILTIN_SPECS, RUNTIME_EXPORT_SYMBOLS, SYM_DYNAMIC_CAST_CHECKED, SYM_DYNAMIC_CELL_CREATE,
    SYM_DYNAMIC_CELL_WRAP, SYM_DYNAMIC_MAP_AOT, SYM_DYNAMIC_MAP_FALLBACK, SYM_DYNAMIC_OBJECT_ALLOC,
};

#[test]
fn dynamic_builtin_symbols_are_registered() {
    let required = [
        SYM_DYNAMIC_CELL_CREATE,
        SYM_DYNAMIC_CELL_WRAP,
        SYM_DYNAMIC_CAST_CHECKED,
        SYM_DYNAMIC_MAP_AOT,
        SYM_DYNAMIC_MAP_FALLBACK,
        SYM_DYNAMIC_OBJECT_ALLOC,
    ];
    for sym in required {
        assert!(
            BUILTIN_SPECS.iter().any(|spec| spec.symbol == sym),
            "BUILTIN_SPECS must list {sym}",
        );
        assert!(
            RUNTIME_EXPORT_SYMBOLS.contains(&sym),
            "RUNTIME_EXPORT_SYMBOLS must list {sym}",
        );
    }
}
