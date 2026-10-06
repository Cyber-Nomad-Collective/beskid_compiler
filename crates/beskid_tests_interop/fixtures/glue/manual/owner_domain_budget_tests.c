/* Live compiled-shape domains consume the same canonical owner byte budget.
 * This tests the real provider; it cannot qualify emitted signature authority. */
#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <stdalign.h>
#include <stdint.h>
static alignas(BESKID_RUNTIME_STATE_ALIGNMENT) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE];
static uint64_t shapes[65536];
int main(void) {
    assert(beskid_rt_v5_process_init(runtime));
    for(size_t i=0;i<65536;i++) shapes[i]=(uint64_t)i+1;
    uint64_t libraries[33]={0};
    for(size_t i=0;i<33;i++) {
        assert(!beskid_glue_v1_owner_open_library(7,&libraries[i]));
        assert(libraries[i]);
    }
    for(size_t i=0;i<32;i++)
        assert(!beskid_glue_v1_owner_bind_shapes(libraries[i],7,shapes,65536));
    assert(beskid_glue_v1_owner_bind_shapes(libraries[32],7,shapes,65536));
    assert(!beskid_glue_v1_owner_close_library(libraries[0],7));
    gc_collect();
    /* Rejection did not publish a partial domain: exact same binding can retry. */
    assert(!beskid_glue_v1_owner_bind_shapes(libraries[32],7,shapes,65536));
    for(size_t i=1;i<33;i++) assert(!beskid_glue_v1_owner_close_library(libraries[i],7));
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}
