// Native ELF TLS ownership for the canonical runtime. Initial-exec TLS keeps this helper within
// the existing Linux ABI import contract: it does not introduce a `__tls_get_addr` import.
static _Thread_local void *beskid_runtime_current_tls
    __attribute__((tls_model("initial-exec")));

void *beskid_rt_v5_intrinsic_tls_get(void) {
    return beskid_runtime_current_tls;
}

void beskid_rt_v5_intrinsic_tls_set(void *value) {
    beskid_runtime_current_tls = value;
}
