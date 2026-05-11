use beskid_abi::BESKID_RUNTIME_ABI_VERSION;

/// Runtime ABI version constant ([`BESKID_RUNTIME_ABI_VERSION`]) for host/runtime negotiation.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn beskid_runtime_abi_version() -> u32 {
    BESKID_RUNTIME_ABI_VERSION
}
