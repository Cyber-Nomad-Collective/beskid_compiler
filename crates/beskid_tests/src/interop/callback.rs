use beskid_abi::{
    BESKID_USER_FFI_LAYOUT_BAND, RUNTIME_EXPORT_SYMBOLS, SYM_BESKID_REGISTER_CALLBACKS,
};

#[test]
fn user_ffi_layout_band_is_one() {
    assert_eq!(BESKID_USER_FFI_LAYOUT_BAND, 1);
}

#[test]
fn register_callbacks_symbol_is_runtime_export() {
    assert!(RUNTIME_EXPORT_SYMBOLS.contains(&SYM_BESKID_REGISTER_CALLBACKS));
}
