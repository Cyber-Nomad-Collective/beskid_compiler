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

/* The process barrier word of incremental marking (`export_data` in runtime_manifest.bsol): the
 * number of heaps in this process that are marking. Generated code loads it before a store that
 * overwrites a managed field or array element and calls beskid_rt_v5_gc_write_barrier only when
 * it is non-zero; the runtime changes it with atomic add and subtract through the address below.
 * Canonical artifacts link the kit statically, so the word is read without import indirection.
 * The explicit initializer makes this a real zero-initialized definition: a bare tentative
 * definition is a common symbol under Apple clang, which Mach-O archives and their symbol tables
 * do not record as a definition. */
size_t beskid_rt_v5_gc_barrier_active = 0;

void *beskid_rt_v5_intrinsic_gc_barrier_active_word(void) {
    return &beskid_rt_v5_gc_barrier_active;
}
