use beskid_abi::BESKID_RUNTIME_ABI_VERSION;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn beskid_runtime_abi_version() -> u32 {
    BESKID_RUNTIME_ABI_VERSION
}
