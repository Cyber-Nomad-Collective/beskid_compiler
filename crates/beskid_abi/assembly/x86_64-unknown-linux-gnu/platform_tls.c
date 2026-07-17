// Native ELF TLS ownership for the canonical runtime. Do not force initial-exec TLS here: the
// shared runtime is loaded with dlopen, which cannot reserve static TLS for a late-loaded DSO.
// The compiler selects a dynamically resolved TLS model (local-dynamic for this private symbol)
// through the ELF loader's __tls_get_addr boundary.
static _Thread_local void *beskid_runtime_current_tls;

void *beskid_rt_v5_intrinsic_tls_get(void) {
    return beskid_runtime_current_tls;
}

void beskid_rt_v5_intrinsic_tls_set(void *value) {
    beskid_runtime_current_tls = value;
}
