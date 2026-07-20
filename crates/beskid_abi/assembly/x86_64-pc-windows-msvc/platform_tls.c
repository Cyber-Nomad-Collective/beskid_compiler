// Windows TLS storage for the ABI-v5 runtime. `__declspec(thread)` gives each thread that uses
// the runtime an independent slot; the compiler never emits unsupported CLIF TLS globals.

#pragma comment(lib, "kernel32.lib")

static __declspec(thread) void *beskid_runtime_current_tls;

void *beskid_rt_v5_intrinsic_tls_get(void) {
    return beskid_runtime_current_tls;
}

void beskid_rt_v5_intrinsic_tls_set(void *value) {
    beskid_runtime_current_tls = value;
}
