/* Provider domain prerequisite only; compiled image/signature proof is separate. */
#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <stdint.h>
#include <stddef.h>
#include <stdalign.h>
typedef struct { const uint8_t *pointer; size_t length; uint64_t token; } OwnedView;
extern int32_t beskid_glue_v1_owner_open_library(uint64_t, uint64_t *);
extern int32_t beskid_glue_v1_owner_bind_shapes(uint64_t, uint64_t, const uint64_t *, size_t);
extern int32_t beskid_glue_v1_owner_copy(uint64_t,uint64_t,uint64_t,uint64_t,const uint8_t *,size_t,OwnedView *);
extern int32_t beskid_glue_v1_owner_release(uint64_t,uint64_t,uint64_t,uint64_t,uint64_t);
static alignas(BESKID_RUNTIME_STATE_ALIGNMENT) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE];
int main(void) {
    assert(beskid_rt_v5_process_init(runtime));
    uint64_t library=0;
    assert(!beskid_glue_v1_owner_open_library(7,&library) && library);
    uint8_t byte=65;
    OwnedView output={(void *)1,1,1};
    assert(beskid_glue_v1_owner_copy(library,11,7,2,&byte,1,&output));
    assert(!output.pointer && !output.length && !output.token);
    uint64_t duplicate[]={11,11}, reversed[]={12,11}, zero[]={0,11};
    assert(beskid_glue_v1_owner_bind_shapes(library,7,duplicate,2));
    assert(beskid_glue_v1_owner_bind_shapes(library,7,reversed,2));
    assert(beskid_glue_v1_owner_bind_shapes(library,7,zero,2));
    uint64_t shapes[]={11,UINT64_MAX};
    assert(!beskid_glue_v1_owner_bind_shapes(library,7,shapes,2));
    assert(beskid_glue_v1_owner_bind_shapes(library,7,shapes,2));
    output=(OwnedView){(void *)1,1,1};
    assert(beskid_glue_v1_owner_copy(library,12,7,2,&byte,1,&output));
    assert(!output.pointer && !output.length && !output.token);
    assert(!beskid_glue_v1_owner_copy(library,UINT64_MAX,7,2,&byte,1,&output));
    assert(output.token && output.pointer && output.length==1);
    gc_collect();
    assert(output.pointer[0]==byte);
    assert(!beskid_glue_v1_owner_release(library,UINT64_MAX,7,2,output.token));
    assert(!beskid_glue_v1_owner_copy(library,11,7,2,&byte,1,&output));
    uint64_t revoked=output.token;
    assert(beskid_glue_v1_owner_close_library(library,8));
    assert(output.pointer[0]==byte);
    assert(!beskid_glue_v1_owner_close_library(library,7));
    assert(beskid_glue_v1_owner_close_library(library,7));
    assert(beskid_glue_v1_owner_release(library,11,7,2,revoked));
    output=(OwnedView){(void *)1,1,1};
    assert(beskid_glue_v1_owner_copy(library,11,7,2,&byte,1,&output));
    assert(!output.pointer && !output.length && !output.token);
    gc_collect();
    beskid_rt_v5_process_shutdown(runtime);
    assert(beskid_rt_v5_process_init(runtime));
    assert(beskid_glue_v1_owner_bind_shapes(library,7,shapes,2));
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}
