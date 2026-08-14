#include <windows.h>
#include "../../include/beskid_runtime_abi_v5.h"

static INIT_ONCE beskid_tls_once = INIT_ONCE_STATIC_INIT;
static DWORD beskid_tls_index = TLS_OUT_OF_INDEXES;
static const char beskid_tls_failure[] = "Windows TLS allocation failed";

static BOOL CALLBACK beskid_tls_initialize(PINIT_ONCE once, PVOID parameter, PVOID *context) {
    (void)once;
    (void)parameter;
    (void)context;
    beskid_tls_index = TlsAlloc();
    return beskid_tls_index != TLS_OUT_OF_INDEXES;
}

static DWORD beskid_tls_index_or_trap(void) {
    if (!InitOnceExecuteOnce(&beskid_tls_once, beskid_tls_initialize, NULL, NULL)) {
        beskid_rt_v5_trap(10, (void *)beskid_tls_failure, sizeof(beskid_tls_failure) - 1);
    }
    return beskid_tls_index;
}

void *beskid_rt_v5_intrinsic_tls_get(void) {
    return TlsGetValue(beskid_tls_index_or_trap());
}

void beskid_rt_v5_intrinsic_tls_set(void *value) {
    if (!TlsSetValue(beskid_tls_index_or_trap(), value)) {
        beskid_rt_v5_trap(10, (void *)beskid_tls_failure, sizeof(beskid_tls_failure) - 1);
    }
}
