// Native Darwin TLV ownership for the canonical runtime's trusted TLS intrinsics.
// This object deliberately exports only the manifest intrinsic boundary; the compiler never
// materializes a CLIF TLS global, which AArch64 Cranelift does not support.

static _Thread_local void *beskid_runtime_current_tls;

void *beskid_rt_v5_intrinsic_tls_get(void) {
    return beskid_runtime_current_tls;
}

void beskid_rt_v5_intrinsic_tls_set(void *value) {
    beskid_runtime_current_tls = value;
}
