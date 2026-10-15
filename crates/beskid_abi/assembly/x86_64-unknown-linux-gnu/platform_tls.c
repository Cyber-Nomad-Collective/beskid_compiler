// Native ELF TLS ownership for the canonical runtime. Do not force initial-exec TLS here: the
// shared runtime is loaded with dlopen, which cannot reserve static TLS for a late-loaded DSO.
// The compiler selects a dynamically resolved TLS model (local-dynamic for this private symbol)
// through the ELF loader's __tls_get_addr boundary.
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
 * Canonical artifacts link the kit statically, so the word is read without import indirection. */
size_t beskid_rt_v5_gc_barrier_active;

void *beskid_rt_v5_intrinsic_gc_barrier_active_word(void) {
    return &beskid_rt_v5_gc_barrier_active;
}
