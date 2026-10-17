// Native Darwin TLV ownership for the canonical runtime's trusted TLS intrinsics.
// This object deliberately exports only the manifest intrinsic boundary; the compiler never
// materializes a CLIF TLS global, which AArch64 Cranelift does not support.
#include <stddef.h>

static _Thread_local void *beskid_runtime_current_tls;

void *beskid_rt_v5_intrinsic_tls_get(void) {
    return beskid_runtime_current_tls;
}

void beskid_rt_v5_intrinsic_tls_set(void *value) {
    beskid_runtime_current_tls = value;
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
