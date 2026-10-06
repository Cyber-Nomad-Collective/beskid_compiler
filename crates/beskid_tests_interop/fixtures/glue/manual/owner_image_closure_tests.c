/* Exact canonical provider gate: unadmitted native pointers must never be read.
 * Positive descriptor admission uses actual compiler-owned getter symbols in the
 * native producer gate; this fixture does not manufacture a descriptor. */
#define BESKID_RUNTIME_ABI_LAYOUTS_ONLY
#include "beskid_runtime_abi_v5.h"
#undef BESKID_RUNTIME_ABI_LAYOUTS_ONLY
#include "owner_identity_v1.h"
#include <assert.h>
#include <stdalign.h>
#include <string.h>
/* Exact signatures from the canonical ABI catalog; the versioned owner header
 * supplies its typed DTO transports without conflicting generic void* names. */
extern void *beskid_rt_v5_process_init(void *);
extern void beskid_rt_v5_process_shutdown(void *);
extern size_t gc_collect(void);
static alignas(BESKID_RUNTIME_STATE_ALIGNMENT) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE];
static void *must_not_construct(void *payload,void *shape) {(void)payload;(void)shape;assert(0);return NULL;}
static void *must_not_read(void *cell) {(void)cell;assert(0);return NULL;}
int main(void) {
    assert(beskid_rt_v5_process_init(runtime));
    uint64_t library=0;
    assert(!beskid_glue_v1_owner_open_library(7,&library) && library);
    const uint64_t shapes[]={11};
    assert(!beskid_glue_v1_owner_bind_shapes(library,7,shapes,1));
    const uint8_t unadmitted_brand[32]={11};
    /* No loader-issued owner-domain row means no destructor authority, even
     * with a live library and a nonzero token-shaped scalar. */
    assert(beskid_glue_v1_owner_release_consumer_opaque(library,unadmitted_brand,UINT64_MAX)!=0);
    assert(beskid_glue_v1_owner_release_consumer_opaque(library,unadmitted_brand,0)!=0);
    assert(beskid_glue_v1_owner_release_consumer_opaque(library,NULL,UINT64_MAX)!=0);
    void *borrowed=(void *)(uintptr_t)1;uint64_t borrow=UINT64_MAX;
    assert(beskid_glue_v1_owner_opaque_borrow_begin(library,unadmitted_brand,UINT64_MAX,&borrowed,&borrow)!=0);
    assert(!borrowed && !borrow);
    borrowed=(void *)(uintptr_t)1;borrow=UINT64_MAX;
    assert(beskid_glue_v1_owner_opaque_borrow_begin(library,NULL,UINT64_MAX,&borrowed,&borrow)!=0);
    assert(!borrowed && !borrow);
    borrowed=(void *)(uintptr_t)1;
    assert(beskid_glue_v1_owner_opaque_borrow_begin(library,unadmitted_brand,UINT64_MAX,&borrowed,NULL)!=0 && !borrowed);
    assert(beskid_glue_v1_owner_opaque_borrow_end(library,unadmitted_brand,UINT64_MAX)!=0);
    assert(beskid_glue_v1_owner_opaque_borrow_end(library,NULL,UINT64_MAX)!=0);
    const uint8_t signature[]={1};
    BeskidDynamicShapeRegistrationV1 registration={0};
    registration.version=1;registration.size=sizeof registration;
    registration.library=library;registration.generation=7;
    memset(registration.source_sha256,1,32);memset(registration.signature_sha256,2,32);
    registration.signature=signature;registration.signature_length=sizeof signature;
    registration.payload_descriptor=(void *)(uintptr_t)1;
    registration.cell_descriptor=(void *)(uintptr_t)1;
    registration.construct=must_not_construct;registration.read=must_not_read;
    uint64_t tag=UINT64_MAX;
    assert(beskid_dynamic_v1_register_shape(&registration,&tag)!=0 && !tag);
    gc_collect();
    tag=UINT64_MAX;
    assert(beskid_dynamic_v1_register_shape(&registration,&tag)!=0 && !tag);
    assert(!beskid_glue_v1_owner_close_library(library,7));
    assert(beskid_glue_v1_owner_release_consumer_opaque(library,unadmitted_brand,UINT64_MAX)!=0);
    tag=UINT64_MAX;
    assert(beskid_dynamic_v1_register_shape(&registration,&tag)!=0 && !tag);
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}
