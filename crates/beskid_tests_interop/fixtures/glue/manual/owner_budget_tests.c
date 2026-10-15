/* Canonical owner resource bound and release reclamation; not a Glue ABI qualification. */
#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <stdalign.h>
#include <stdint.h>
#include <string.h>
typedef struct { const uint8_t *pointer; size_t length; uint64_t token; } OwnedView;
extern int32_t beskid_glue_v1_owner_open_library(uint64_t,uint64_t*);
extern int32_t beskid_glue_v1_owner_bind_shapes(uint64_t,uint64_t,const uint64_t*,size_t);
extern int32_t beskid_glue_v1_owner_copy(uint64_t,uint64_t,uint64_t,uint64_t,const uint8_t*,size_t,OwnedView*);
extern int32_t beskid_glue_v1_owner_release(uint64_t,uint64_t,uint64_t,uint64_t,uint64_t);
static alignas(BESKID_RUNTIME_STATE_ALIGNMENT) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE];
static uint8_t bytes[65536];
int main(void) {
    assert(beskid_rt_v5_process_init(runtime));
    uint64_t library=0;
    assert(beskid_glue_v1_owner_open_library(7,&library)==0 && library);
    const uint64_t shapes[]={11};
    assert(beskid_glue_v1_owner_bind_shapes(library,7,shapes,1)==0);
    memset(bytes,0xA5,sizeof bytes);
    OwnedView owners[256]={{0}};
    for(size_t i=0;i<256;i++) {
        // Eight bytes of the exact process budget belong to the live shape domain.
        size_t length=i==255 ? sizeof bytes-8 : sizeof bytes;
        assert(beskid_glue_v1_owner_copy(library,11,7,2,bytes,length,&owners[i])==0);
        assert(owners[i].length==length && owners[i].token);
    }
    OwnedView failed={(void*)1,1,1};
    assert(beskid_glue_v1_owner_copy(library,11,7,2,bytes,1,&failed)!=0);
    assert(!failed.pointer && !failed.length && !failed.token);
    assert(beskid_glue_v1_owner_release(library,11,7,2,owners[0].token)==0);
    OwnedView replacement={0};
    assert(beskid_glue_v1_owner_copy(library,11,7,2,bytes,sizeof bytes,&replacement)==0);
    assert(replacement.token!=owners[0].token);
    gc_collect();
    assert(memcmp(replacement.pointer,bytes,sizeof bytes)==0);
    for(size_t i=1;i<256;i++) assert(beskid_glue_v1_owner_release(library,11,7,2,owners[i].token)==0);
    assert(beskid_glue_v1_owner_release(library,11,7,2,replacement.token)==0);
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}
