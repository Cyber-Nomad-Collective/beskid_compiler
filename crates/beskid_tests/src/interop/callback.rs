use beskid_abi::{
    BESKID_USER_FFI_LAYOUT_BAND, RUNTIME_EXPORT_SYMBOLS, SYM_BESKID_REGISTER_CALLBACKS,
    SYM_BESKID_REGISTER_HANDLERS,
};
use beskid_runtime::{
    bootstrap_dispatch_handlers, install_callback_trampoline, registered_callbacks,
    CallbackTableEntry, beskid_register_callbacks,
};

#[test]
fn user_ffi_layout_band_is_one() {
    assert_eq!(BESKID_USER_FFI_LAYOUT_BAND, 1);
}

#[test]
fn register_callbacks_symbol_is_runtime_export() {
    assert!(RUNTIME_EXPORT_SYMBOLS.contains(&SYM_BESKID_REGISTER_CALLBACKS));
}

#[test]
fn register_handlers_symbol_is_runtime_export() {
    assert!(RUNTIME_EXPORT_SYMBOLS.contains(&SYM_BESKID_REGISTER_HANDLERS));
}

#[test]
fn bootstrap_dispatch_handlers_accepts_empty_table() {
    bootstrap_dispatch_handlers();
    bootstrap_dispatch_handlers();
}

extern "C" fn sample_export() -> i64 {
    42
}

#[test]
fn trampoline_resolves_symbol_id_and_invokes_target() {
    let export_ptr = sample_export as *const u8;
    let symbol_id = 7_u32;
    let trampoline = install_callback_trampoline(export_ptr, symbol_id);
    let table = [CallbackTableEntry {
        symbol_id,
        fn_ptr: trampoline,
        userdata: std::ptr::null_mut(),
    }];
    assert_eq!(
        beskid_register_callbacks(BESKID_USER_FFI_LAYOUT_BAND, table.as_ptr(), table.len()),
        0
    );

    let callable: extern "C" fn() -> i64 = unsafe { std::mem::transmute(trampoline) };
    assert_eq!(callable(), 42);
    assert_eq!(registered_callbacks()[0].symbol_id, symbol_id);
}
