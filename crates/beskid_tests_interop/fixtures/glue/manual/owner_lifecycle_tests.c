/* Real canonical provider API gate; no fixture descriptor or owner registry. */
#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <stdalign.h>

typedef struct { const uint8_t *pointer; size_t length; uint64_t token; } OwnedView;
extern int32_t beskid_glue_v1_owner_open_library(uint64_t, uint64_t *);
extern int32_t beskid_glue_v1_owner_bind_shapes(uint64_t,uint64_t,const uint64_t*,size_t);
extern int32_t beskid_glue_v1_owner_copy(uint64_t, uint64_t, uint64_t, uint64_t,
                                      const uint8_t *, size_t, OwnedView *);
extern int32_t beskid_glue_v1_owner_release(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
static alignas(BESKID_RUNTIME_STATE_ALIGNMENT) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE];

int main(void) {
    assert(beskid_rt_v5_process_init(runtime));
    uint64_t first = 0, second = 0;
    assert(beskid_glue_v1_owner_open_library(7, &first) == 0 && first);
    assert(beskid_glue_v1_owner_open_library(7, &second) == 0 && second && first != second);
    const uint64_t shapes[]={11};
    assert(beskid_glue_v1_owner_bind_shapes(first,7,shapes,1)==0);
    assert(beskid_glue_v1_owner_bind_shapes(second,7,shapes,1)==0);
    const uint8_t bytes[] = {0, 128, 255, 65};
    OwnedView owned = {0};
    assert(beskid_glue_v1_owner_copy(first, 11, 7, 2, bytes, sizeof bytes, &owned) == 0);
    assert(owned.token && owned.pointer && owned.length == sizeof bytes);
    uintptr_t callback_row[2] = {77, 1};
    beskid_register_callbacks(callback_row, 1);
    gc_collect();
    assert(memcmp(owned.pointer, bytes, sizeof bytes) == 0);
    assert(beskid_glue_v1_owner_release(second, 11, 7, 2, owned.token) != 0);
    assert(beskid_glue_v1_owner_release(first, 12, 7, 2, owned.token) != 0);
    assert(beskid_glue_v1_owner_release(first, 11, 8, 2, owned.token) != 0);
    assert(beskid_glue_v1_owner_release(first, 11, 7, 1, owned.token) != 0);
    assert(memcmp(owned.pointer, bytes, sizeof bytes) == 0);
    assert(beskid_glue_v1_owner_release(first, 11, 7, 2, owned.token) == 0);
    assert(beskid_glue_v1_owner_release(first, 11, 7, 2, owned.token) != 0);
    OwnedView qualified={0};
    assert(!beskid_glue_v1_owner_copy(first,11,7,2,bytes,sizeof bytes,&qualified));
    assert(beskid_glue_v1_owner_release_token(second,qualified.token));
    assert(memcmp(qualified.pointer,bytes,sizeof bytes)==0);
    assert(!beskid_glue_v1_owner_release_token(first,qualified.token));
    assert(beskid_glue_v1_owner_release_token(first,qualified.token));
    OwnedView failed = {(void *)1, 1, 1};
    assert(beskid_glue_v1_owner_copy(first, 11, 7, 2, NULL, 1, &failed) != 0);
    assert(!failed.pointer && !failed.length && !failed.token);
    const uint8_t invalid_utf8[] = {0xED, 0xA0, 0x80};
    failed = (OwnedView){(void *)1, 1, 1};
    assert(beskid_glue_v1_owner_copy(first, 11, 7, 1, invalid_utf8, sizeof invalid_utf8, &failed) != 0);
    assert(!failed.pointer && !failed.length && !failed.token);
    OwnedView leaked = {0};
    assert(beskid_glue_v1_owner_copy(first, 11, 7, 2, bytes, sizeof bytes, &leaked) == 0);
    beskid_rt_v5_process_shutdown(runtime);
    assert(beskid_rt_v5_process_init(runtime));
    uint64_t fresh = 0;
    assert(beskid_glue_v1_owner_open_library(7, &fresh) == 0 && fresh && fresh != first);
    assert(beskid_glue_v1_owner_bind_shapes(fresh,7,shapes,1)==0);
    assert(beskid_glue_v1_owner_release(first, 11, 7, 2, leaked.token) != 0);
    assert(beskid_glue_v1_owner_release(fresh, 11, 7, 2, leaked.token) != 0);
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}
