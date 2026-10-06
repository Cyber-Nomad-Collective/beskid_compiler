/* Root transport must name the managed value actually returned to adapters. */
#include "beskid_runtime_abi_v5.h"
#include "owner_identity_v1.h"
#include <assert.h>
#include <stdalign.h>
#include <stddef.h>
extern int32_t beskid_glue_v1_input_bytes(const uint8_t *,size_t,void **,size_t *);
static alignas(BESKID_RUNTIME_STATE_ALIGNMENT) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE];
int main(void) {
    assert(beskid_rt_v5_process_init(runtime));
    const uint8_t bytes[]={0,128,255,65};
    void *value=NULL;size_t root=0;
    assert(!beskid_glue_v1_input_bytes(bytes,sizeof bytes,&value,&root));
    assert(value && root);
    assert(gc_resolve_handle(root)==value);
    gc_collect();
    assert(gc_resolve_handle(root));
    gc_unroot_handle(root);
    assert(!gc_resolve_handle(root));
    value=NULL;root=0;
    assert(!beskid_glue_v1_input_bytes(NULL,0,&value,&root));
    assert(value && root && gc_resolve_handle(root)==value);
    gc_collect();assert(gc_resolve_handle(root));gc_unroot_handle(root);
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}
