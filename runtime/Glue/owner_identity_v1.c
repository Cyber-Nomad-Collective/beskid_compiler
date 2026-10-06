#include "owner_identity_v1.h"
#include <stdatomic.h>
#include <stdint.h>

/* Neither heap teardown nor library/session startup can access or reset this authority. */
static _Atomic uint64_t process_identity = 0;

static uint64_t next_identity(_Atomic uint64_t *counter) {
    uint64_t previous = atomic_load_explicit(counter, memory_order_relaxed);
    for (;;) {
        if (previous == UINT64_MAX) return 0;
        const uint64_t next = previous + 1;
        if (atomic_compare_exchange_weak_explicit(counter, &previous, next,
                                                  memory_order_relaxed, memory_order_relaxed)) {
            return next;
        }
    }
}

uint64_t beskid_glue_v1_next_identity(void) {
    return next_identity(&process_identity);
}

#ifndef BESKID_GLUE_ISSUER_EXHAUSTION_TEST
#define BESKID_RUNTIME_ABI_LAYOUTS_ONLY
#include "include/beskid_runtime_abi_v5.h"
#undef BESKID_RUNTIME_ABI_LAYOUTS_ONLY
#include <stddef.h>
#include <string.h>

extern void *beskid_rt_v5_process_init(void *);
extern void beskid_rt_v5_process_shutdown(void *);
extern void *beskid_rt_v5_intrinsic_tls_get(void);

/* These transports are emitted from canonical typed OwnerRecord.bd. */
extern void *beskid_glue_v1_record_leaf(void *, size_t, size_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
extern void *beskid_glue_v1_record_link(void *, void *, size_t, size_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
extern void *beskid_glue_v1_checked_record_leaf(void *, size_t, size_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
extern void *beskid_glue_v1_checked_record_link(void *, void *, size_t, size_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
extern size_t beskid_glue_v1_record_next_count(void *);
extern void *beskid_glue_v1_record_previous(void *);
extern uint64_t beskid_glue_v1_record_tag(void *, size_t);
extern size_t beskid_glue_v1_record_bind_shapes(void *, void *);
extern size_t beskid_glue_v1_record_has_shape(void *, uint64_t);
extern size_t beskid_glue_v1_record_release(void *,void *);
extern size_t beskid_glue_v1_record_begin_close(void *);
extern size_t beskid_rt_v5_gc_try_register_root(void *);
extern void gc_unregister_root(void *);
extern void *beskid_glue_v1_record_payload(void *);
extern void *beskid_glue_v1_root_load(void);
extern uint8_t beskid_glue_v1_root_publish(void *);
extern void beskid_glue_v1_root_clear(void);
extern uint64_t beskid_glue_v1_runtime_generation(void);
extern void *beskid_rt_v5_intrinsic_system_allocate(size_t, size_t);
extern void beskid_rt_v5_intrinsic_system_free(void *, size_t);

/* The owning descriptor and field geometry are compiler-issued typed facts.
 * No descriptor is constructed here. Constructor return is rooted before any
 * further allocation. The two public words are an interior ABI view, not a
 * foreign managed-pointer fabrication. */
extern void *beskid_rt_v5_utf8_record_construct(void *, size_t);
extern void *beskid_rt_v5_checked_utf8_record_construct(void *, size_t);
extern void *beskid_rt_v5_checked_scope_current(void);
extern size_t beskid_rt_v5_checked_scope_failure_reason(void);
extern size_t beskid_rt_v5_gc_try_root_handle(void *);
extern void *beskid_rt_v5_utf8_record_payload(void *);
extern void beskid_rt_v5_utf8_record_set_data(void *, void *);
extern uint64_t beskid_rt_v5_managed_view_kind(void *);
extern size_t gc_root_handle(void *);
extern void gc_unroot_handle(size_t);
extern void *beskid_rt_v5_gc_resolve_handle(size_t);

void *beskid_rt_v5_intrinsic_utf8_view_new(void *source, size_t length) {
    if ((!source && length) || length > INT64_MAX) return NULL;
    int checked = beskid_rt_v5_checked_scope_current() != NULL;
    void *record = checked ? beskid_rt_v5_checked_utf8_record_construct(source, length)
                           : beskid_rt_v5_utf8_record_construct(source, length);
    if (!record || (checked && beskid_rt_v5_checked_scope_failure_reason())) return NULL;
    size_t root = checked ? beskid_rt_v5_gc_try_root_handle(record) : gc_root_handle(record);
    if (!root || (checked && beskid_rt_v5_checked_scope_failure_reason())) {
        if (root) gc_unroot_handle(root);
        return NULL;
    }
    record = beskid_rt_v5_gc_resolve_handle(root);
    if (!record) { gc_unroot_handle(root); return NULL; }
    void *payload = beskid_rt_v5_utf8_record_payload(record);
    if (!payload) { gc_unroot_handle(root); return NULL; }
    /* Read the canonical array view, whose physical closure was checked by
     * typed lowering. No allocation occurs between this read and publication. */
    void *data = NULL;
    size_t count = 0;
    memcpy(&data, payload, sizeof data);
    memcpy(&count, (unsigned char *)payload + sizeof(void *), sizeof count);
    if (!data || count != length) { gc_unroot_handle(root); return NULL; }
    beskid_rt_v5_utf8_record_set_data(record, data);
    void *view = (unsigned char *)record + 24;
    gc_unroot_handle(root);
    return view;
}

static _Atomic uint64_t process_stamp = 0;
static atomic_flag owner_operation = ATOMIC_FLAG_INIT;
enum { OWNER_OK=0, OWNER_INVALID=1, OWNER_FOREIGN=2, OWNER_BUSY=4, OWNER_SESSION=1, OWNER_LIBRARY=2, OWNER_BUFFER=3, OWNER_IMAGE_CLOSURE=6, OWNER_OPAQUE=7, OWNER_OPAQUE_DOMAIN=8, OWNER_OPAQUE_BORROW=9 };
#define OWNER_MAX_RECORDS 65536u
#define OWNER_MAX_BYTES (16u * 1024u * 1024u)

BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_host_open(void **out) {
    if(!out || (uintptr_t)out % _Alignof(void *)) return OWNER_INVALID;
    *out=NULL;
    if(beskid_rt_v5_intrinsic_tls_get()) return OWNER_BUSY;
    void *runtime=beskid_rt_v5_intrinsic_system_allocate(BESKID_RUNTIME_STATE_SIZE,BESKID_RUNTIME_STATE_ALIGNMENT);
    if(!runtime) return 3;
    memset(runtime,0,BESKID_RUNTIME_STATE_SIZE);
    if(beskid_rt_v5_process_init(runtime)!=runtime) {
        beskid_rt_v5_intrinsic_system_free(runtime,BESKID_RUNTIME_STATE_SIZE);
        return OWNER_INVALID;
    }
    *out=runtime;
    return OWNER_OK;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_host_close(void *runtime) {
    if(!runtime) return OWNER_INVALID;
    void *tls=beskid_rt_v5_intrinsic_tls_get(),*owner=NULL;
    if(!tls) return OWNER_FOREIGN;
    memcpy(&owner,(uint8_t *)tls+BESKID_TLS_STATE_RUNTIME_OFFSET,sizeof owner);
    if(owner!=runtime) return OWNER_FOREIGN;
    /* Only a linear same-thread host lease calls this hook. The canonical
     * shutdown drops registry roots and destroys the heap before images drop. */
    beskid_rt_v5_process_shutdown(runtime);
    beskid_rt_v5_intrinsic_system_free(runtime,BESKID_RUNTIME_STATE_SIZE);
    return OWNER_OK;
}
static uint64_t stamp(void) {
    uint64_t value = atomic_load_explicit(&process_stamp, memory_order_acquire);
    if (value) return value;
    uint64_t issued = beskid_glue_v1_next_identity();
    if (!issued) return 0;
    uint64_t empty = 0;
    if (atomic_compare_exchange_strong_explicit(&process_stamp, &empty, issued,
                                               memory_order_release, memory_order_acquire)) return issued;
    return empty;
}
static void *previous(void *record) {
    size_t count = beskid_glue_v1_record_next_count(record);
    if (count == 0) return NULL;
    if (count != 1) return NULL;
    return beskid_glue_v1_record_previous(record);
}
/* Closed bounded traversal. A malformed cardinality, issuer or session fails admission. */
static void *find_record(uint64_t token, uint64_t session, uint64_t issuer, size_t *count) {
    void *found = NULL, *record = beskid_glue_v1_root_load();
    *count = 0;
    while (record) {
        if (++*count > OWNER_MAX_RECORDS || beskid_glue_v1_record_next_count(record) > 1 ||
            beskid_glue_v1_record_tag(record,0) != issuer ||
            (session && beskid_glue_v1_record_tag(record,1) != session)) return NULL;
        if (beskid_glue_v1_record_tag(record,6) == token) found = record;
        record = previous(record);
    }
    return found;
}
static void *publish_record(void *head, void *buffer, size_t length, size_t allocation,
                            uint64_t issuer, uint64_t session, uint64_t library,
                            uint64_t generation, uint64_t kind, uint64_t shape, uint64_t token) {
    int checked=beskid_rt_v5_checked_scope_current()!=NULL;
    if(checked && beskid_rt_v5_checked_scope_failure_reason()) return NULL;
    void *record;
    if(checked) record=head ? beskid_glue_v1_checked_record_link(head,buffer,length,allocation,issuer,session,library,generation,kind,shape,token)
                          : beskid_glue_v1_checked_record_leaf(buffer,length,allocation,issuer,session,library,generation,kind,shape,token);
    else record=head ? beskid_glue_v1_record_link(head,buffer,length,allocation,issuer,session,library,generation,kind,shape,token)
                     : beskid_glue_v1_record_leaf(buffer,length,allocation,issuer,session,library,generation,kind,shape,token);
    if(checked && beskid_rt_v5_checked_scope_failure_reason()) return NULL;
    if (!record) return NULL;
    if (!beskid_glue_v1_root_publish(record)) return NULL;
    return record;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_open_library(uint64_t generation, uint64_t *out) {
    if (!out || (uintptr_t)out % _Alignof(uint64_t)) return OWNER_INVALID;
    *out = 0;
    if (!generation || !beskid_glue_v1_runtime_generation()) return OWNER_INVALID;
    if (atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    int32_t status = OWNER_INVALID;
    uint64_t issuer = stamp(), session = 0;
    void *head = beskid_glue_v1_root_load();
    if (issuer) {
        if (head) session = beskid_glue_v1_record_tag(head,1);
        else {
            session = beskid_glue_v1_next_identity();
            if (session) head = publish_record(NULL,NULL,0,0,issuer,session,session,
                beskid_glue_v1_runtime_generation(),OWNER_SESSION,session,session);
        }
        size_t count = 0;
        if (head && session && find_record(session,session,issuer,&count) && count < OWNER_MAX_RECORDS) {
            uint64_t library = beskid_glue_v1_next_identity();
            if (library && publish_record(head,NULL,0,0,issuer,session,library,generation,OWNER_LIBRARY,library,library)) {
                *out = library; status = OWNER_OK;
            }
        }
    }
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);
    return status;
}
static void *admit_library(uint64_t library, uint64_t generation, uint64_t *session, uint64_t *issuer, size_t *count) {
    void *head = beskid_glue_v1_root_load();
    if (!head || !beskid_glue_v1_runtime_generation()) return NULL;
    *session = beskid_glue_v1_record_tag(head,1); *issuer = stamp();
    void *record = find_record(library,*session,*issuer,count);
    if (!record || beskid_glue_v1_record_tag(record,4) != OWNER_LIBRARY ||
        beskid_glue_v1_record_tag(record,3) != generation || beskid_glue_v1_record_tag(record,7)) return NULL;
    return record;
}
/* Nonallocating pre-effect admission. The library's generation comes from its
 * canonical live record, never from untrusted adapter argument metadata. */
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_validate_binding(uint64_t library,uint64_t shape) {
    if(!library || !shape) return OWNER_INVALID;
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    int32_t status=OWNER_FOREIGN;
    void *head=beskid_glue_v1_root_load();
    if(head && beskid_glue_v1_runtime_generation()) {
        uint64_t session=beskid_glue_v1_record_tag(head,1),issuer=stamp(); size_t count=0;
        void *record=find_record(library,session,issuer,&count);
        if(record && beskid_glue_v1_record_tag(record,4)==OWNER_LIBRARY &&
            beskid_glue_v1_record_tag(record,3) && !beskid_glue_v1_record_tag(record,7) &&
            beskid_glue_v1_record_has_shape(record,shape)) status=OWNER_OK;
    }
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);
    return status;
}
static int release_record(void *record,uint64_t session,uint64_t issuer) {
    size_t count=0;
    void *owner=find_record(session,session,issuer,&count);
    if(!owner || beskid_glue_v1_record_tag(owner,4)!=OWNER_SESSION || beskid_glue_v1_record_tag(owner,7)) return 0;
    void *empty=beskid_glue_v1_record_payload(owner);
    return empty && beskid_glue_v1_record_release(record,empty);
}
static int managed_payload_length(void *record,size_t *length) {
    void *payload=beskid_glue_v1_record_payload(record);
    if(!payload) return 0;
    /* Canonical typed array-view transport; no independent object descriptor. */
    memcpy(length,(uint8_t *)payload+sizeof(void *),sizeof *length);
    return *length<=OWNER_MAX_BYTES;
}
static int owner_native_budget(void *record,uint64_t session,uint64_t issuer,size_t additional) {
    if(additional>OWNER_MAX_BYTES) return 0;
    size_t total=additional,count=0;
    while(record) {
        if(++count>OWNER_MAX_RECORDS || beskid_glue_v1_record_tag(record,0)!=issuer ||
            beskid_glue_v1_record_tag(record,1)!=session) return 0;
        uint64_t kind=beskid_glue_v1_record_tag(record,4);
        if((kind==OWNER_BUFFER+1 || kind==OWNER_BUFFER+2) && !beskid_glue_v1_record_tag(record,7)) {
            uint64_t allocation=beskid_glue_v1_record_tag(record,8);
            if(allocation>OWNER_MAX_BYTES-total) return 0;
            total+=(size_t)allocation;
        }
        if((kind==OWNER_LIBRARY || kind==OWNER_IMAGE_CLOSURE || kind==OWNER_OPAQUE || kind==OWNER_OPAQUE_DOMAIN || kind==OWNER_OPAQUE_BORROW) && beskid_glue_v1_record_tag(record,7)!=1) {
            size_t domain_bytes=0;
            if(!managed_payload_length(record,&domain_bytes) || domain_bytes>OWNER_MAX_BYTES-total) return 0;
            total+=domain_bytes;
        }
        size_t links=beskid_glue_v1_record_next_count(record);
        if(links>1) return 0;
        record=links ? beskid_glue_v1_record_previous(record) : NULL;
    }
    return 1;
}
static int strict_utf8(const uint8_t *bytes, size_t length) {
    for (size_t i=0; i<length;) {
        uint32_t value=bytes[i++]; size_t remaining=0; uint32_t minimum=0;
        if (value < 0x80) continue;
        if (value >= 0xC2 && value <= 0xDF) { value &= 0x1F; remaining=1; minimum=0x80; }
        else if (value >= 0xE0 && value <= 0xEF) { value &= 0x0F; remaining=2; minimum=0x800; }
        else if (value >= 0xF0 && value <= 0xF4) { value &= 7; remaining=3; minimum=0x10000; }
        else return 0;
        if (remaining > length-i) return 0;
        while (remaining--) { uint32_t next=bytes[i++]; if ((next & 0xC0)!=0x80) return 0; value=(value<<6)|(next&0x3F); }
        if (value < minimum || value > 0x10FFFF || (value >= 0xD800 && value <= 0xDFFF)) return 0;
    }
    return 1;
}
/* Closing a library revokes its tokens but never unloads descriptor images.
 * The host process lease retains those images until canonical heap teardown. */
static int32_t close_opaque_library(uint64_t,uint64_t);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_close_library(uint64_t library,uint64_t generation) {
    if(!library || !generation) return OWNER_INVALID;
    int32_t opaque_status=close_opaque_library(library,generation);
    if(opaque_status!=OWNER_OK && opaque_status!=6) return opaque_status;
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    uint64_t session=0,issuer=0; size_t count=0;
    void *head=beskid_glue_v1_root_load();
    issuer=stamp();session=head ? beskid_glue_v1_record_tag(head,1):0;
    void *domain=session ? find_record(library,session,issuer,&count):NULL;
    if(domain && beskid_glue_v1_record_tag(domain,4)==OWNER_LIBRARY &&
       beskid_glue_v1_record_tag(domain,3)==generation && beskid_glue_v1_record_tag(domain,7)==1) {
        atomic_flag_clear_explicit(&owner_operation,memory_order_release);return OWNER_OK;
    }
    if(domain && (beskid_glue_v1_record_tag(domain,4)!=OWNER_LIBRARY ||
       beskid_glue_v1_record_tag(domain,3)!=generation || beskid_glue_v1_record_tag(domain,7)!=2)) domain=NULL;
    int32_t status=OWNER_FOREIGN;
    if(domain && owner_native_budget(head,session,issuer,0)) {
        size_t session_count=0;
        void *owner=find_record(session,session,issuer,&session_count);
        void *empty=owner ? beskid_glue_v1_record_payload(owner) : NULL;
        if(empty && beskid_glue_v1_record_tag(owner,4)==OWNER_SESSION &&
            !beskid_glue_v1_record_tag(owner,7)) {
            status=opaque_status;
            for(void *record=head;record;record=previous(record)) {
                if(beskid_glue_v1_record_tag(record,2)==library &&
                    !beskid_glue_v1_record_release(record,empty)) {
                    status=OWNER_INVALID;
                    break;
                }
            }
        }
    }
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);
    return status;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_copy(uint64_t library, uint64_t shape,
    uint64_t generation, uint64_t kind, const uint8_t *bytes, size_t length, GlueOwnedViewV1 *out) {
    if (!out || (uintptr_t)out % _Alignof(GlueOwnedViewV1)) return OWNER_INVALID;
    memset(out,0,sizeof *out);
    if (!library || !shape || !generation || (kind != 1 && kind != 2) || length > OWNER_MAX_BYTES || (!bytes && length)) return OWNER_INVALID;
    if (kind == 1 && !strict_utf8(bytes,length)) return OWNER_INVALID;
    if (atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    uint64_t session=0, issuer=0; size_t count=0;
    int32_t status = OWNER_FOREIGN;
    void *library_record=admit_library(library,generation,&session,&issuer,&count);
    if (library_record && beskid_glue_v1_record_has_shape(library_record,shape) && count < OWNER_MAX_RECORDS &&
        owner_native_budget(beskid_glue_v1_root_load(),session,issuer,length ? length : 1)) {
        size_t allocation = length ? length : 1;
        void *buffer = beskid_rt_v5_intrinsic_system_allocate(allocation,1);
        if (buffer) {
            if (length) memcpy(buffer,bytes,length); else *(uint8_t *)buffer=0;
            uint64_t token = beskid_glue_v1_next_identity();
            if (token) {
                /* Output kind is carried separately from the internal library/session categories. */
                void *record = publish_record(beskid_glue_v1_root_load(),buffer,length,allocation,
                    issuer,session,library,generation,kind+OWNER_BUFFER,shape,token);
                if (record) { out->pointer=buffer; out->length=length; out->token=token; status=OWNER_OK; buffer=NULL; }
            }
            if (buffer) beskid_rt_v5_intrinsic_system_free(buffer,allocation);
        } else status=OWNER_INVALID;
    }
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);
    return status;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_release(uint64_t library, uint64_t shape,
    uint64_t generation, uint64_t kind, uint64_t token) {
    if (!library || !shape || !generation || (kind!=1 && kind!=2) || !token) return OWNER_INVALID;
    if (atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    uint64_t session=0, issuer=0; size_t count=0;
    int32_t status=OWNER_FOREIGN;
    if (admit_library(library,generation,&session,&issuer,&count)) {
        void *record=find_record(token,session,issuer,&count);
        if (record && beskid_glue_v1_record_tag(record,2)==library &&
            beskid_glue_v1_record_tag(record,3)==generation && beskid_glue_v1_record_tag(record,4)==kind+OWNER_BUFFER &&
            beskid_glue_v1_record_tag(record,5)==shape && !beskid_glue_v1_record_tag(record,7)) {
            if(release_record(record,session,issuer)) status=OWNER_OK;
        }
    }
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);
    return status;
}
/* Qualified image release transport. All shape/kind/generation facts come
 * from this live library's canonical source-owned record, never caller tags. */
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_release_token(uint64_t library,uint64_t token) {
    if(!library || !token) return OWNER_INVALID;
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    void *head=beskid_glue_v1_root_load();
    uint64_t issuer=stamp(),session=head ? beskid_glue_v1_record_tag(head,1) : 0;
    size_t count=0;
    void *domain=session ? find_record(library,session,issuer,&count) : NULL;
    int32_t status=OWNER_FOREIGN;
    if(domain && beskid_glue_v1_record_tag(domain,4)==OWNER_LIBRARY &&
        !beskid_glue_v1_record_tag(domain,7)) {
        uint64_t generation=beskid_glue_v1_record_tag(domain,3);
        void *record=find_record(token,session,issuer,&count);
        if(record && beskid_glue_v1_record_tag(record,2)==library &&
            beskid_glue_v1_record_tag(record,3)==generation &&
            (beskid_glue_v1_record_tag(record,4)==OWNER_BUFFER+1 ||
             beskid_glue_v1_record_tag(record,4)==OWNER_BUFFER+2) &&
            !beskid_glue_v1_record_tag(record,7) &&
            beskid_glue_v1_record_has_shape(domain,beskid_glue_v1_record_tag(record,5)) &&
            release_record(record,session,issuer)) status=OWNER_OK;
    }
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);
    return status;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_input_utf8(const uint8_t *bytes, size_t length,
    void **value, size_t *root) {
    if (!value || !root) return OWNER_INVALID;
    *value=NULL; *root=0;
    if (length > OWNER_MAX_BYTES || (!bytes && length) || !strict_utf8(bytes,length)) return OWNER_INVALID;
    void *result=beskid_rt_v5_intrinsic_utf8_view_new((void *)bytes,length);
    if (!result) return OWNER_INVALID;
    size_t retained=gc_root_handle(result);
    if (!retained) return OWNER_INVALID;
    *value=result; *root=retained; return OWNER_OK;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_input_bytes(const uint8_t *bytes, size_t length,
    void **value, size_t *root) {
    if (!value || !root) return OWNER_INVALID;
    *value=NULL; *root=0;
    if (length > OWNER_MAX_BYTES || (!bytes && length)) return OWNER_INVALID;
    void *record=beskid_rt_v5_utf8_record_construct((void *)bytes,length);
    if (!record) return OWNER_INVALID;
    size_t retained=gc_root_handle(record);
    if (!retained) return OWNER_INVALID;
    record=beskid_rt_v5_gc_resolve_handle(retained);
    void *payload=record ? beskid_rt_v5_utf8_record_payload(record) : NULL;
    size_t payload_root=payload ? gc_root_handle(payload) : 0;
    gc_unroot_handle(retained);
    if(!payload_root) return OWNER_INVALID;
    payload=beskid_rt_v5_gc_resolve_handle(payload_root);
    if(!payload) {gc_unroot_handle(payload_root);return OWNER_INVALID;}
    *value=payload; *root=payload_root; return OWNER_OK;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_bind_shapes(uint64_t library,
    uint64_t generation, const uint64_t *shapes, size_t count) {
    if(!library || !generation || !shapes || (uintptr_t)shapes % _Alignof(uint64_t) ||
        !count || count>65536) return OWNER_INVALID;
    uint64_t previous=0;
    for(size_t i=0;i<count;i++) {
        if(!shapes[i] || shapes[i]<=previous) return OWNER_INVALID;
        previous=shapes[i];
    }
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    uint64_t session=0,issuer=0; size_t records=0,root=0;
    void *managed=NULL,*buffer=NULL;
    int32_t status=OWNER_FOREIGN;
    void *domain=admit_library(library,generation,&session,&issuer,&records);
    if(!domain) goto done;
    size_t length=count*8,existing=0;
    if(!managed_payload_length(domain,&existing) || existing ||
        !owner_native_budget(beskid_glue_v1_root_load(),session,issuer,length)) {
        status=OWNER_INVALID; goto done;
    }
    buffer=beskid_rt_v5_intrinsic_system_allocate(length,1);
    if(!buffer) { status=OWNER_INVALID; goto done; }
    for(size_t i=0;i<count;i++) for(size_t byte=0;byte<8;byte++)
        ((uint8_t *)buffer)[i*8+byte]=(uint8_t)(shapes[i]>>(byte*8));
    status=beskid_glue_v1_input_bytes(buffer,length,&managed,&root);
    if(status) goto done;
    /* Allocation may collect; reacquire the library through canonical root
     * traversal, not a stale native local pointer retained across allocation. */
    void *record=admit_library(library,generation,&session,&issuer,&records);
    status=record && beskid_glue_v1_record_bind_shapes(record,managed) ? OWNER_OK : OWNER_INVALID;
done:
    if(root) gc_unroot_handle(root);
    if(buffer) beskid_rt_v5_intrinsic_system_free(buffer,count*8);
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);
    return status;
}
/* Private loader transport. The payload is an ordinary traced source u8[];
 * addresses inside it are scalar bytes, never a managed descriptor substitute. */
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_bind_image_closure(uint64_t library,
    uint64_t generation,const BeskidGlueImageClosureRowV1 *rows,size_t count) {
    _Static_assert(sizeof(BeskidGlueImageClosureRowV1)==80,"image closure V1 row");
    _Static_assert(sizeof(void *)==8,"image closure V1 target width");
    if(!library || !generation || !rows || (uintptr_t)rows % _Alignof(BeskidGlueImageClosureRowV1) ||
        !count || count>4096) return OWNER_INVALID;
    for(size_t i=0;i<count;i++) {
        uint8_t source=0,signature=0;
        for(size_t j=0;j<32;j++) {source|=rows[i].source_sha256[j];signature|=rows[i].signature_sha256[j];}
        if(!source || !signature || !rows[i].address || !rows[i].role || !((rows[i].role>=1 && rows[i].role<=6) || (rows[i].role>=13 && rows[i].role<=15)) ||
            (i && memcmp(&rows[i-1],&rows[i],sizeof *rows)>=0)) return OWNER_INVALID;
    }
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    uint64_t session=0,issuer=0; size_t records=0;
    int32_t status=OWNER_FOREIGN;
    if(!admit_library(library,generation,&session,&issuer,&records)) goto done;
    void *head=beskid_glue_v1_root_load();
    for(void *record=head;record;record=previous(record)) {
        if(beskid_glue_v1_record_tag(record,2)==library &&
            beskid_glue_v1_record_tag(record,4)==OWNER_IMAGE_CLOSURE) {status=OWNER_INVALID;goto done;}
    }
    size_t bytes=count*sizeof *rows;
    if(records>=OWNER_MAX_RECORDS || !owner_native_budget(head,session,issuer,bytes)) {status=OWNER_INVALID;goto done;}
    uint64_t token=beskid_glue_v1_next_identity();
    if(token && publish_record(head,(void *)rows,bytes,0,issuer,session,library,generation,
                              OWNER_IMAGE_CLOSURE,library,token)) status=OWNER_OK;
done:
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);
    return status;
}
/* Caller holds owner_operation and has already admitted the library. No native
 * address is dereferenced here: only canonical typed owner payload is examined. */
static int admitted_image_address(uint64_t library,uint64_t generation,const uint8_t *source,
    const uint8_t signature[32],void *address,uint64_t role) {
    if(!library || !generation || !signature || !address || !role || !((role>=1 && role<=6) || (role>=13 && role<=15))) return 0;
    size_t records=0;
    for(void *record=beskid_glue_v1_root_load();record;record=previous(record)) {
        if(++records>OWNER_MAX_RECORDS) return 0;
        if(beskid_glue_v1_record_tag(record,2)!=library ||
            beskid_glue_v1_record_tag(record,3)!=generation ||
            beskid_glue_v1_record_tag(record,4)!=OWNER_IMAGE_CLOSURE ||
            beskid_glue_v1_record_tag(record,7)) continue;
        void *payload=beskid_glue_v1_record_payload(record); const uint8_t *data=NULL; size_t bytes=0;
        if(!payload) return 0;
        memcpy(&data,payload,sizeof data);memcpy(&bytes,(uint8_t *)payload+sizeof(void *),sizeof bytes);
        if(!data || !bytes || bytes>4096*80 || bytes%80) return 0;
        for(size_t i=0;i<bytes/80;i++) {
            BeskidGlueImageClosureRowV1 row;
            memcpy(&row,data+i*80,sizeof row);
            if(row.address==(uint64_t)(uintptr_t)address && row.role==role &&
                !memcmp(row.signature_sha256,signature,32) &&
                (!source || !memcmp(row.source_sha256,source,32))) return 1;
        }
    }
    return 0;
}
/* Checked counterparts are producer-issued image rows, not a token/name grant.
 * The ordinary and guarded entry must share the exact source/signature closure.
 * No callback or descriptor address is dereferenced during lookup. */
static void *admitted_checked_address(uint64_t library,uint64_t generation,void *ordinary,uint64_t role) {
    if(!ordinary || role<3 || role>5) return NULL;
    BeskidGlueImageClosureRowV1 original; int found=0; void *guarded=NULL; size_t records=0;
    for(void *record=beskid_glue_v1_root_load();record;record=previous(record)) {
        if(++records>OWNER_MAX_RECORDS) return NULL;
        if(beskid_glue_v1_record_tag(record,2)!=library || beskid_glue_v1_record_tag(record,3)!=generation ||
           beskid_glue_v1_record_tag(record,4)!=OWNER_IMAGE_CLOSURE || beskid_glue_v1_record_tag(record,7)) continue;
        void *payload=beskid_glue_v1_record_payload(record); const uint8_t *data=NULL; size_t bytes=0;
        if(!payload) return NULL;
        memcpy(&data,payload,sizeof data); memcpy(&bytes,(uint8_t *)payload+sizeof(void *),sizeof bytes);
        if(!data || !bytes || bytes>4096*80 || bytes%80) return NULL;
        for(size_t i=0;i<bytes/80;i++) {
            BeskidGlueImageClosureRowV1 row; memcpy(&row,data+i*80,sizeof row);
            if(row.address==(uint64_t)(uintptr_t)ordinary && row.role==role) {
                if(found++) return NULL;
                original=row;
            }
        }
        if(!found) return NULL;
        for(size_t i=0;i<bytes/80;i++) {
            BeskidGlueImageClosureRowV1 row; memcpy(&row,data+i*80,sizeof row);
            if(row.role==role+10 && !memcmp(row.source_sha256,original.source_sha256,32) &&
               !memcmp(row.signature_sha256,original.signature_sha256,32)) {
                if(guarded) return NULL;
                guarded=(void *)(uintptr_t)row.address;
            }
        }
        return guarded;
    }
    return NULL;
}
typedef struct { uint8_t brand[32]; uint64_t address; uint64_t destructor; } OpaquePayloadV1;
static uint64_t opaque_shape(const uint8_t brand[32]) {
    uint64_t shape=0; if(!brand) return 0;
    for(size_t i=0;i<8;i++) shape|=(uint64_t)brand[i]<<(i*8);
    return shape;
}
static int record_bytes(void *record,const uint8_t **data,size_t *length) {
    void *payload=beskid_glue_v1_record_payload(record);
    if(!payload) return 0;
    memcpy(data,payload,sizeof *data);memcpy(length,(uint8_t *)payload+sizeof(void *),sizeof *length);
    return *data && *length<=OWNER_MAX_BYTES;
}
static void *live_library(uint64_t library,uint64_t *generation,uint64_t *session,uint64_t *issuer,size_t *count) {
    void *head=beskid_glue_v1_root_load();if(!head || !library) return NULL;
    *session=beskid_glue_v1_record_tag(head,1);*issuer=stamp();
    void *record=find_record(library,*session,*issuer,count);
    if(!record || beskid_glue_v1_record_tag(record,4)!=OWNER_LIBRARY || beskid_glue_v1_record_tag(record,7)) return NULL;
    *generation=beskid_glue_v1_record_tag(record,3);
    return admit_library(library,*generation,session,issuer,count);
}
static int image_signature(uint64_t library,uint64_t generation,const uint8_t signature[32],uint64_t role) {
    size_t count=0;
    for(void *record=beskid_glue_v1_root_load();record;record=previous(record)) {
        if(++count>OWNER_MAX_RECORDS) return 0;
        if(beskid_glue_v1_record_tag(record,2)!=library || beskid_glue_v1_record_tag(record,3)!=generation ||
           beskid_glue_v1_record_tag(record,4)!=OWNER_IMAGE_CLOSURE || beskid_glue_v1_record_tag(record,7)) continue;
        const uint8_t *data=NULL;size_t length=0;
        if(!record_bytes(record,&data,&length) || length%80 || length>4096*80) return 0;
        for(size_t i=0;i<length/80;i++) {BeskidGlueImageClosureRowV1 row;memcpy(&row,data+i*80,sizeof row);
            if(row.role==role && !memcmp(row.signature_sha256,signature,32)) return 1;
        }
    }return 0;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_bind_opaque_domains(uint64_t library,
    uint64_t generation,const BeskidGlueOpaqueDomainRowV1 *rows,size_t count) {
    _Static_assert(sizeof(BeskidGlueOpaqueDomainRowV1)==80,"opaque domain V1 row");
    if(!library || !generation || !rows || (uintptr_t)rows%_Alignof(BeskidGlueOpaqueDomainRowV1) || !count || count>1024) return OWNER_INVALID;
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    int32_t status=OWNER_FOREIGN;uint64_t session=0,issuer=0;size_t records=0;
    void *consumer=admit_library(library,generation,&session,&issuer,&records);
    if(!consumer) goto domain_done;
    for(void *record=beskid_glue_v1_root_load();record;record=previous(record)) {
        if(beskid_glue_v1_record_tag(record,2)==library && beskid_glue_v1_record_tag(record,4)==OWNER_OPAQUE_DOMAIN) goto domain_done;
    }
    for(size_t i=0;i<count;i++) {
        uint64_t owner_session=0,owner_issuer=0;size_t owner_count=0;
        void *owner=admit_library(rows[i].owner_library,rows[i].owner_generation,&owner_session,&owner_issuer,&owner_count);
        if(!owner || owner_session!=session || owner_issuer!=issuer ||
           !beskid_glue_v1_record_has_shape(consumer,opaque_shape(rows[i].consumer_brand)) ||
           !beskid_glue_v1_record_has_shape(owner,opaque_shape(rows[i].owner_brand)) ||
           !image_signature(library,generation,rows[i].consumer_brand,1) ||
           !image_signature(rows[i].owner_library,rows[i].owner_generation,rows[i].owner_brand,6) ||
           (i && memcmp(rows[i-1].consumer_brand,rows[i].consumer_brand,32)>=0)) goto domain_done;
    }
    if(records>=OWNER_MAX_RECORDS || !owner_native_budget(beskid_glue_v1_root_load(),session,issuer,count*80)) {status=OWNER_INVALID;goto domain_done;}
    {uint64_t token=beskid_glue_v1_next_identity();
     if(token && publish_record(beskid_glue_v1_root_load(),(void *)rows,count*80,0,issuer,session,library,generation,OWNER_OPAQUE_DOMAIN,library,token)) status=OWNER_OK;}
domain_done:
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);return status;
}
static int opaque_payload(void *record,const uint8_t brand[32],OpaquePayloadV1 *out) {
    const uint8_t *data=NULL;size_t length=0;
    if(!record_bytes(record,&data,&length) || length!=sizeof *out ||
       beskid_glue_v1_record_tag(record,4)!=OWNER_OPAQUE || beskid_glue_v1_record_tag(record,7)) return 0;
    memcpy(out,data,sizeof *out);return out->address && out->destructor && !memcmp(out->brand,brand,32);
}
static void *opaque_record(uint64_t library,const uint8_t brand[32],uint64_t token,
    OpaquePayloadV1 *payload,uint64_t *session,uint64_t *issuer) {
    uint64_t generation=0;size_t count=0;
    if(!brand || !token || !live_library(library,&generation,session,issuer,&count)) return NULL;
    void *record=find_record(token,*session,*issuer,&count);
    if(!record || beskid_glue_v1_record_tag(record,2)!=library || beskid_glue_v1_record_tag(record,3)!=generation ||
       beskid_glue_v1_record_tag(record,5)!=opaque_shape(brand) || !opaque_payload(record,brand,payload) ||
       !admitted_image_address(library,generation,NULL,brand,(void *)(uintptr_t)payload->destructor,6)) return NULL;
    return record;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_opaque_create(uint64_t library,const uint8_t *brand,
    void *address,BeskidGlueOpaqueDestructorV1 destructor,uint64_t *out) {
    _Static_assert(sizeof(OpaquePayloadV1)==48,"opaque payload V1");
    _Static_assert(sizeof(BeskidGlueOpaqueDestructorV1)==8,"opaque destructor V1 target width");
    if(!out || (uintptr_t)out%_Alignof(uint64_t)) return OWNER_INVALID;*out=0;
    if(!library || !opaque_shape(brand) || !address || !destructor) return OWNER_INVALID;
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    uint64_t generation=0,session=0,issuer=0;size_t count=0;int32_t status=OWNER_FOREIGN;
    void *domain=live_library(library,&generation,&session,&issuer,&count);
    OpaquePayloadV1 payload;memcpy(payload.brand,brand,32);payload.address=(uint64_t)(uintptr_t)address;memcpy(&payload.destructor,&destructor,8);
    if(!domain || !beskid_glue_v1_record_has_shape(domain,opaque_shape(brand)) ||
       !admitted_image_address(library,generation,NULL,brand,(void *)(uintptr_t)payload.destructor,6)) goto opaque_create_done;
    for(void *record=beskid_glue_v1_root_load();record;record=previous(record)) {
        OpaquePayloadV1 existing;
        if(beskid_glue_v1_record_tag(record,2)==library && opaque_payload(record,brand,&existing) && existing.address==payload.address) goto opaque_create_done;
    }
    if(count>=OWNER_MAX_RECORDS || !owner_native_budget(beskid_glue_v1_root_load(),session,issuer,sizeof payload)) {status=OWNER_INVALID;goto opaque_create_done;}
    {uint64_t token=beskid_glue_v1_next_identity();
     if(token && publish_record(beskid_glue_v1_root_load(),&payload,sizeof payload,0,issuer,session,library,generation,OWNER_OPAQUE,opaque_shape(brand),token)) {*out=token;status=OWNER_OK;}}
opaque_create_done:
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);return status;
}
/* Borrow metadata uses the canonical traced OwnerRecord payload. No foreign
 * address is stored as a managed reference or independently registered root. */
typedef struct { uint8_t brand[32]; uint64_t target; } OpaqueBorrowV1;
_Static_assert(sizeof(OpaqueBorrowV1)==40,"opaque borrow payload width");
static int active_opaque_borrow(uint64_t library,uint64_t generation,uint64_t target,
    uint64_t session,uint64_t issuer) {
    size_t count=0;
    for(void *record=beskid_glue_v1_root_load();record;record=previous(record)) {
        if(++count>OWNER_MAX_RECORDS || beskid_glue_v1_record_next_count(record)>1 ||
           beskid_glue_v1_record_tag(record,0)!=issuer || beskid_glue_v1_record_tag(record,1)!=session) return -1;
        if(beskid_glue_v1_record_tag(record,4)!=OWNER_OPAQUE_BORROW || beskid_glue_v1_record_tag(record,7) ||
           beskid_glue_v1_record_tag(record,2)!=library || beskid_glue_v1_record_tag(record,3)!=generation) continue;
        const uint8_t *bytes=NULL;size_t length=0;OpaqueBorrowV1 borrow;
        if(!record_bytes(record,&bytes,&length) || length!=sizeof borrow) return -1;
        memcpy(&borrow,bytes,sizeof borrow);
        if(!borrow.target || !opaque_shape(borrow.brand) ||
           beskid_glue_v1_record_tag(record,5)!=opaque_shape(borrow.brand)) return -1;
        if(!target || borrow.target==target) return 1;
    }
    return 0;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_opaque_borrow_begin(uint64_t library,
    const uint8_t *brand,uint64_t token,void **out,uint64_t *lease_out) {
    if(out && (uintptr_t)out%_Alignof(void *)==0) *out=NULL;
    if(lease_out && (uintptr_t)lease_out%_Alignof(uint64_t)==0) *lease_out=0;
    if(!out || !lease_out || (uintptr_t)out%_Alignof(void *) || (uintptr_t)lease_out%_Alignof(uint64_t)) return OWNER_INVALID;
    if(!library || !brand || !token) return OWNER_INVALID;
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    OpaquePayloadV1 payload;uint64_t session=0,issuer=0;size_t count=0;
    int32_t status=OWNER_FOREIGN;
    void *target=opaque_record(library,brand,token,&payload,&session,&issuer);
    if(target) {
        uint64_t generation=beskid_glue_v1_record_tag(target,3);
        OpaqueBorrowV1 borrow;memcpy(borrow.brand,brand,32);borrow.target=token;
        (void)find_record(token,session,issuer,&count);
        if(count<OWNER_MAX_RECORDS && owner_native_budget(beskid_glue_v1_root_load(),session,issuer,sizeof borrow)) {
            uint64_t lease=beskid_glue_v1_next_identity();
            if(lease && publish_record(beskid_glue_v1_root_load(),&borrow,sizeof borrow,0,issuer,session,
                library,generation,OWNER_OPAQUE_BORROW,opaque_shape(brand),lease)) {
                /* The operation lock prevents release between validation and
                 * rooted lease publication; only scalar Box address survives. */
                *lease_out=lease;*out=(void *)(uintptr_t)payload.address;status=OWNER_OK;
            } else status=OWNER_INVALID;
        } else status=OWNER_INVALID;
    }
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);return status;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_opaque_borrow_end(uint64_t library,
    const uint8_t *brand,uint64_t lease) {
    if(!library || !brand || !lease) return OWNER_INVALID;
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    uint64_t generation=0,session=0,issuer=0;size_t count=0;int32_t status=OWNER_FOREIGN;
    if(live_library(library,&generation,&session,&issuer,&count)) {
        void *record=find_record(lease,session,issuer,&count);
        const uint8_t *bytes=NULL;size_t length=0;OpaqueBorrowV1 borrow;
        if(record && beskid_glue_v1_record_tag(record,4)==OWNER_OPAQUE_BORROW &&
           beskid_glue_v1_record_tag(record,2)==library && beskid_glue_v1_record_tag(record,3)==generation &&
           !beskid_glue_v1_record_tag(record,7) && beskid_glue_v1_record_tag(record,5)==opaque_shape(brand) &&
           record_bytes(record,&bytes,&length) && length==sizeof borrow) {
            memcpy(&borrow,bytes,sizeof borrow);
            if(borrow.target && !memcmp(borrow.brand,brand,32) && release_record(record,session,issuer)) status=OWNER_OK;
        }
    }
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);return status;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_opaque_release(uint64_t library,const uint8_t *brand,uint64_t token) {
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    OpaquePayloadV1 payload;uint64_t session=0,issuer=0;
    void *record=opaque_record(library,brand,token,&payload,&session,&issuer);
    if(record) {
        int borrowed=active_opaque_borrow(library,beskid_glue_v1_record_tag(record,3),token,session,issuer);
        if(borrowed) {atomic_flag_clear_explicit(&owner_operation,memory_order_release);return borrowed>0?OWNER_BUSY:OWNER_INVALID;}
    }
    int released=record && release_record(record,session,issuer);
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);
    if(!released) return OWNER_FOREIGN;
    // No managed pointer survives this callback. The record is tombstoned and
    // its payload cleared first; reentrancy cannot consume the token twice.
    BeskidGlueOpaqueDestructorV1 destructor;memcpy(&destructor,&payload.destructor,8);
    return destructor((void *)(uintptr_t)payload.address)==0 ? OWNER_OK : 6;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_validate_opaque(uint64_t library,const uint8_t *brand,uint64_t token) {
    if(!library || !brand || !token) return OWNER_INVALID;
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    uint64_t generation=0,session=0,issuer=0;size_t count=0;int32_t status=OWNER_FOREIGN;
    if(!live_library(library,&generation,&session,&issuer,&count)) goto validate_opaque_done;
    for(void *record=beskid_glue_v1_root_load();record;record=previous(record)) {
        if(beskid_glue_v1_record_tag(record,2)!=library || beskid_glue_v1_record_tag(record,3)!=generation ||
           beskid_glue_v1_record_tag(record,4)!=OWNER_OPAQUE_DOMAIN || beskid_glue_v1_record_tag(record,7)) continue;
        const uint8_t *data=NULL;size_t length=0;
        if(!record_bytes(record,&data,&length) || !length || length%80 || length>1024*80) goto validate_opaque_done;
        for(size_t i=0;i<length/80;i++) {BeskidGlueOpaqueDomainRowV1 row;memcpy(&row,data+i*80,sizeof row);
            if(!memcmp(row.consumer_brand,brand,32)) {
                uint64_t owner_session=0,owner_issuer=0;size_t owner_count=0;OpaquePayloadV1 payload;
                if(admit_library(row.owner_library,row.owner_generation,&owner_session,&owner_issuer,&owner_count) &&
                   opaque_record(row.owner_library,row.owner_brand,token,&payload,&owner_session,&owner_issuer)) status=OWNER_OK;
                goto validate_opaque_done;
            }
        }
    }
validate_opaque_done:
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);return status;
}
/* Insert beside owner_validate_opaque in the existing canonical provider.
 * This service resolves only a loader-issued consumer domain; no caller can
 * provide an owning library or destructor address. No managed pointer escapes
 * the operation lock and no allocation occurs before the scalar tuple copy. */
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_release_consumer_opaque(
    uint64_t library,const uint8_t *brand,uint64_t token) {
    if(!library || !brand || !token) return OWNER_INVALID;
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    uint64_t generation=0,session=0,issuer=0,owner_library=0;size_t count=0;
    uint8_t owner_brand[32]={0};int32_t status=OWNER_FOREIGN;
    if(!live_library(library,&generation,&session,&issuer,&count)) goto release_consumer_done;
    for(void *record=beskid_glue_v1_root_load();record;record=previous(record)) {
        if(beskid_glue_v1_record_tag(record,2)!=library || beskid_glue_v1_record_tag(record,3)!=generation ||
           beskid_glue_v1_record_tag(record,4)!=OWNER_OPAQUE_DOMAIN || beskid_glue_v1_record_tag(record,7)) continue;
        const uint8_t *data=NULL;size_t length=0;
        if(!record_bytes(record,&data,&length) || !length || length%80 || length>1024*80) goto release_consumer_done;
        for(size_t i=0;i<length/80;i++) {
            BeskidGlueOpaqueDomainRowV1 row;memcpy(&row,data+i*80,sizeof row);
            if(memcmp(row.consumer_brand,brand,32)) continue;
            uint64_t owner_session=0,owner_issuer=0;size_t owner_count=0;OpaquePayloadV1 payload;
            if(admit_library(row.owner_library,row.owner_generation,&owner_session,&owner_issuer,&owner_count) &&
               owner_session==session && owner_issuer==issuer &&
               opaque_record(row.owner_library,row.owner_brand,token,&payload,&owner_session,&owner_issuer)) {
                owner_library=row.owner_library;memcpy(owner_brand,row.owner_brand,32);status=OWNER_OK;
            }
            goto release_consumer_done;
        }
    }
release_consumer_done:
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);
    /* The existing release repeats live-owner, complete-brand and exact image
     * checks before tombstoning. A concurrent close can only reject this tuple;
     * no unrooted managed pointer was retained through the unlocked callback. */
    return status==OWNER_OK ? beskid_glue_v1_owner_opaque_release(owner_library,owner_brand,token):status;
}

static int32_t close_opaque_library(uint64_t library,uint64_t generation) {
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    uint64_t session=0,issuer=0;size_t count=0;
    void *record=beskid_glue_v1_root_load();
    issuer=stamp();session=record ? beskid_glue_v1_record_tag(record,1):0;
    void *domain=session ? find_record(library,session,issuer,&count):NULL;
    if(!domain || beskid_glue_v1_record_tag(domain,4)!=OWNER_LIBRARY ||
       beskid_glue_v1_record_tag(domain,3)!=generation || !beskid_glue_v1_runtime_generation()) {
        atomic_flag_clear_explicit(&owner_operation,memory_order_release);return OWNER_FOREIGN;
    }
    uint64_t closing=beskid_glue_v1_record_tag(domain,7);
    if(closing==1) {atomic_flag_clear_explicit(&owner_operation,memory_order_release);return OWNER_OK;}
    if(closing!=0 && closing!=2) {atomic_flag_clear_explicit(&owner_operation,memory_order_release);return OWNER_FOREIGN;}
    int borrowed=active_opaque_borrow(library,generation,0,session,issuer);
    if(borrowed) {atomic_flag_clear_explicit(&owner_operation,memory_order_release);return borrowed>0?OWNER_BUSY:OWNER_INVALID;}
    if(!beskid_rt_v5_gc_try_register_root(&record)) {atomic_flag_clear_explicit(&owner_operation,memory_order_release);return OWNER_INVALID;}
    // Root admission may move the head. Reissue the scalar library lookup before
    // revoking admission; no unrooted managed pointer crosses the callback.
    domain=find_record(library,session,issuer,&count);
    if(!domain || (!closing && !beskid_glue_v1_record_begin_close(domain))) {
        gc_unregister_root(&record);atomic_flag_clear_explicit(&owner_operation,memory_order_release);return OWNER_FOREIGN;
    }
    int32_t status=OWNER_OK;count=0;
    while(record) {
        if(++count>OWNER_MAX_RECORDS) {status=OWNER_INVALID;break;}
        if(beskid_glue_v1_record_tag(record,2)==library && beskid_glue_v1_record_tag(record,3)==generation &&
           beskid_glue_v1_record_tag(record,4)==OWNER_OPAQUE && !beskid_glue_v1_record_tag(record,7)) {
            const uint8_t *data=NULL;size_t length=0;OpaquePayloadV1 payload;
            if(!record_bytes(record,&data,&length) || length!=sizeof payload) {status=OWNER_INVALID;break;}
            memcpy(&payload,data,sizeof payload);
            if(!admitted_image_address(library,generation,NULL,payload.brand,(void *)(uintptr_t)payload.destructor,6) ||
               !release_record(record,session,issuer)) {status=OWNER_INVALID;break;}
            BeskidGlueOpaqueDestructorV1 destructor;memcpy(&destructor,&payload.destructor,8);
            atomic_flag_clear_explicit(&owner_operation,memory_order_release);
            int32_t foreign=destructor((void *)(uintptr_t)payload.address);
            // Reentrant calls observe closing state and cannot add a new owner.
            // The rooted local is updated by canonical GC if the callback allocates.
            if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) {
                gc_unregister_root(&record);return OWNER_BUSY;
            }
            if(foreign && status==OWNER_OK) status=6;
        }
        record=previous(record);
    }
    gc_unregister_root(&record);atomic_flag_clear_explicit(&owner_operation,memory_order_release);return status;
}
static int32_t result_view(uint64_t library, uint64_t shape, uint64_t kind,
    void *value, GlueOwnedViewV1 *out) {
    if (!out || (uintptr_t)out % _Alignof(GlueOwnedViewV1)) return OWNER_INVALID;
    memset(out,0,sizeof *out);
    if (!value || beskid_rt_v5_managed_view_kind(value)!=kind) return OWNER_INVALID;
    size_t root=gc_root_handle(value);
    if (!root) return OWNER_INVALID;
    uint64_t generation=0;
    if (atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) {
        gc_unroot_handle(root); return OWNER_BUSY;
    }
    void *head=beskid_glue_v1_root_load();
    size_t count=0;
    void *record=head ? find_record(library,beskid_glue_v1_record_tag(head,1),
        beskid_glue_v1_record_tag(head,0),&count) : NULL;
    if (record && beskid_glue_v1_record_tag(record,4)==OWNER_LIBRARY &&
        !beskid_glue_v1_record_tag(record,7)) generation=beskid_glue_v1_record_tag(record,3);
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);
    const uint8_t *bytes=NULL; size_t length=0;
    memcpy(&bytes,value,sizeof bytes); memcpy(&length,(uint8_t *)value+sizeof(void *),sizeof length);
    int32_t status=beskid_glue_v1_owner_copy(library,shape,generation,kind,bytes,length,out);
    gc_unroot_handle(root); return status;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_result_utf8(uint64_t library,uint64_t shape,
    void *value,GlueOwnedViewV1 *out) { return result_view(library,shape,1,value,out); }
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_result_bytes(uint64_t library,uint64_t shape,
    void *value,GlueOwnedViewV1 *out) { return result_view(library,shape,2,value,out); }

BESKID_GLUE_PROVIDER_EXPORT uint8_t beskid_glue_v1_owner_shutdown(void) {
    /* Destroy foreign owners while their admitted image closures are still live.
     * Close revokes the library before invoking any destructor. Restart from the
     * rooted chain after each close because callbacks may move managed objects. */
    for(size_t pass=0;pass<OWNER_MAX_RECORDS;pass++) {
        void *cursor=beskid_glue_v1_root_load(); size_t scanned=0;
        uint64_t library=0,generation=0;
        while(cursor && ++scanned<=OWNER_MAX_RECORDS) {
            if(beskid_glue_v1_record_tag(cursor,4)==OWNER_LIBRARY &&
               beskid_glue_v1_record_tag(cursor,7)!=1) {
                library=beskid_glue_v1_record_tag(cursor,2);
                generation=beskid_glue_v1_record_tag(cursor,3); break;
            }
            cursor=previous(cursor);
        }
        if(scanned>OWNER_MAX_RECORDS) return 0;
        if(!library) break;
        int32_t status=beskid_glue_v1_owner_close_library(library,generation);
        if(status!=OWNER_OK && status!=6) return 0;
        if(pass+1==OWNER_MAX_RECORDS) return 0;
    }
    /* Called after canonical worker shutdown; competing owner operations are forbidden. */
    if (atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return 0;
    void *record=beskid_glue_v1_root_load(); size_t count=0;
    uint64_t session=record ? beskid_glue_v1_record_tag(record,1) : 0;
    uint64_t issuer=stamp();
    /* Validate the complete rooted chain before releasing any record. The
     * session's preadmitted empty array makes teardown allocation-free. */
    size_t session_count=0;
    void *owner=record ? find_record(session,session,issuer,&session_count) : NULL;
    void *empty=owner ? beskid_glue_v1_record_payload(owner) : NULL;
    if(record && (!owner || !empty || beskid_glue_v1_record_tag(owner,4)!=OWNER_SESSION ||
        beskid_glue_v1_record_tag(owner,7))) {
        atomic_flag_clear_explicit(&owner_operation,memory_order_release);
        return 0;
    }
    void *cursor=record;
    while(cursor) {
        if(++count>OWNER_MAX_RECORDS || beskid_glue_v1_record_tag(cursor,0)!=issuer ||
            beskid_glue_v1_record_tag(cursor,1)!=session ||
            beskid_glue_v1_record_next_count(cursor)>1) {
            atomic_flag_clear_explicit(&owner_operation,memory_order_release);
            return 0;
        }
        cursor=previous(cursor);
    }
    while(record) {
        void *next=previous(record);
        if(!beskid_glue_v1_record_release(record,empty)) {
            atomic_flag_clear_explicit(&owner_operation,memory_order_release);
            return 0;
        }
        record=next;
    }
    beskid_glue_v1_root_clear();
    atomic_flag_clear_explicit(&owner_operation,memory_order_release);
    return 1;
}
uint8_t beskid_rt_v5_intrinsic_glue_owner_shutdown(void) { return beskid_glue_v1_owner_shutdown(); }

/* Dynamic uses callback-registry root1056 and the same process issuer/operation
 * gate as Glue. All managed registry objects are constructed by typed source. */
extern void *beskid_dynamic_v1_root_load(void);
extern uint8_t beskid_dynamic_v1_root_publish(void *);
extern void beskid_dynamic_v1_root_clear(void);
extern void *beskid_dynamic_v1_registry_construct(uint64_t,uint64_t,uint64_t);
extern uint64_t beskid_dynamic_v1_registry_tag(void *,size_t);
extern void beskid_dynamic_v1_registry_close(void *);
extern size_t beskid_dynamic_v1_registry_shape_count(void *);
extern void *beskid_dynamic_v1_registry_shape_at(void *,size_t);
extern void beskid_dynamic_v1_registry_append_shape(void *,void *);
extern void *beskid_dynamic_v1_shape_construct(void *,void *,void *,void *,void *,void *,void *,void *,uint64_t,uint64_t,uint64_t);
extern void *beskid_dynamic_v1_shape_pointer(void *,size_t);
extern uint64_t beskid_dynamic_v1_shape_tag(void *,size_t);
extern void *beskid_dynamic_v1_shape_signature(void *);
extern void *beskid_dynamic_v1_shape_digest(void *);
extern void *beskid_dynamic_v1_shape_source(void *);
extern void *beskid_dynamic_v1_shape_owner(void *);
extern void *beskid_dynamic_v1_signature_digest(void *);
extern uint8_t beskid_dynamic_v1_descriptor_valid(void *,uint8_t);
extern void *beskid_dynamic_v1_mapping_construct(void *,void *,void *,void *,uint64_t,uint64_t,uint64_t);
extern void beskid_dynamic_v1_registry_append_mapping(void *,void *);
extern size_t beskid_dynamic_v1_registry_mapping_count(void *);
extern void *beskid_dynamic_v1_registry_mapping_at(void *,size_t);
extern void *beskid_dynamic_v1_mapping_source(void *);
extern void *beskid_dynamic_v1_mapping_destination(void *);
extern void *beskid_dynamic_v1_mapping_signature(void *);
extern void *beskid_dynamic_v1_mapping_transform(void *);
extern uint64_t beskid_dynamic_v1_mapping_tag(void *,size_t);
static int bytes_equal(void *array,const uint8_t *bytes,size_t length) {
    const uint8_t *data=NULL; size_t count=0;
    if(!array) return 0;
    memcpy(&data,array,sizeof data); memcpy(&count,(uint8_t *)array+sizeof(void *),sizeof count);
    return count==length && (!length || (data && !memcmp(data,bytes,length)));
}
static void *dynamic_registry(void) {
    void *owner=beskid_dynamic_v1_root_load();
    if(!owner || !beskid_glue_v1_runtime_generation() ||
        beskid_dynamic_v1_registry_tag(owner,0)!=stamp() ||
        beskid_dynamic_v1_registry_tag(owner,2)!=beskid_glue_v1_runtime_generation() ||
        beskid_dynamic_v1_registry_tag(owner,3)) return NULL;
    return owner;
}
static void *dynamic_shape(uint64_t tag) {
    void *owner=dynamic_registry();
    if(!owner || !tag) return NULL;
    size_t count=beskid_dynamic_v1_registry_shape_count(owner);
    if(count>65536) return NULL;
    for(size_t i=0;i<count;i++) {
        void *shape=beskid_dynamic_v1_registry_shape_at(owner,i);
        if(beskid_dynamic_v1_shape_owner(shape)!=owner) return NULL;
        if(beskid_dynamic_v1_shape_tag(shape,2)==tag) {
            uint64_t session=0,issuer=0; size_t records=0;
            return admit_library(beskid_dynamic_v1_shape_tag(shape,3),
                beskid_dynamic_v1_shape_tag(shape,1),&session,&issuer,&records) ? shape : NULL;
        }
    }
    return NULL;
}
static int dynamic_signature_budget(void *owner,size_t additional) {
    size_t total=additional;
    size_t shapes=beskid_dynamic_v1_registry_shape_count(owner);
    size_t mappings=beskid_dynamic_v1_registry_mapping_count(owner);
    if(shapes>65536 || mappings>65536 || total>OWNER_MAX_BYTES) return 0;
    for(size_t i=0;i<shapes+mappings;i++) {
        void *signature=i<shapes ? beskid_dynamic_v1_shape_signature(beskid_dynamic_v1_registry_shape_at(owner,i)) :
            beskid_dynamic_v1_mapping_signature(beskid_dynamic_v1_registry_mapping_at(owner,i-shapes));
        size_t length=0;
        if(!signature) return 0;
        memcpy(&length,(uint8_t *)signature+sizeof(void *),sizeof length);
        if(length>1048576 || total>OWNER_MAX_BYTES-length) return 0;
        total+=length;
    }
    return 1;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_dynamic_v1_register_shape(const BeskidDynamicShapeRegistrationV1 *dto,uint64_t *out) {
    if(!out || (uintptr_t)out % _Alignof(uint64_t)) return OWNER_INVALID;
    *out=0;
    _Static_assert(sizeof(BeskidDynamicShapeRegistrationV1)==136,"Dynamic V1 physical registration DTO");
    if(!dto || (uintptr_t)dto % _Alignof(BeskidDynamicShapeRegistrationV1) || dto->version!=1 || dto->size!=sizeof(*dto) ||
        !dto->library || !dto->generation || !dto->signature || !dto->signature_length || dto->signature_length>1048576 ||
        !dto->construct || !dto->read || !dto->payload_descriptor || !dto->cell_descriptor) return OWNER_INVALID;
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    uint64_t session=0,issuer=0; size_t records=0;
    int32_t status=OWNER_FOREIGN; size_t signature_root=0,digest_root=0,source_root=0,shape_root=0;
    void *signature=NULL,*digest=NULL,*source=NULL;
    void *constructor=NULL,*reader=NULL;
    _Static_assert(sizeof constructor==sizeof dto->construct,"constructor pointer representation");
    _Static_assert(sizeof reader==sizeof dto->read,"reader pointer representation");
    memcpy(&constructor,&dto->construct,sizeof constructor); memcpy(&reader,&dto->read,sizeof reader);
    if(!admit_library(dto->library,dto->generation,&session,&issuer,&records)) goto done;
    if(!admitted_image_address(dto->library,dto->generation,dto->source_sha256,dto->signature_sha256,dto->payload_descriptor,1) ||
       !admitted_image_address(dto->library,dto->generation,dto->source_sha256,dto->signature_sha256,dto->cell_descriptor,2) ||
       !admitted_image_address(dto->library,dto->generation,dto->source_sha256,dto->signature_sha256,constructor,3) ||
       !admitted_image_address(dto->library,dto->generation,dto->source_sha256,dto->signature_sha256,reader,4)) goto done;
    if(!beskid_dynamic_v1_descriptor_valid(dto->payload_descriptor,0) ||
       !beskid_dynamic_v1_descriptor_valid(dto->cell_descriptor,1)) {status=OWNER_INVALID;goto done;}
    void *owner=dynamic_registry();
    if(!owner) {
        uint64_t identity=beskid_glue_v1_next_identity();
        if(!identity) goto done;
        owner=beskid_dynamic_v1_registry_construct(issuer,identity,beskid_glue_v1_runtime_generation());
        if(!owner || !beskid_dynamic_v1_root_publish(owner)) goto done;
    }
    size_t count=beskid_dynamic_v1_registry_shape_count(owner);
    if(count>=65536) goto done;
    if(!dynamic_signature_budget(owner,dto->signature_length)) goto done;
    /* Byte copies are rooted before hashing or allocating subsequent records. */
    if(beskid_glue_v1_input_bytes(dto->signature,dto->signature_length,&signature,&signature_root)) goto done;
    void *computed=beskid_dynamic_v1_signature_digest(signature);
    if(!bytes_equal(computed,dto->signature_sha256,32)) { status=OWNER_INVALID; goto done; }
    if(beskid_glue_v1_input_bytes(dto->signature_sha256,32,&digest,&digest_root) ||
        beskid_glue_v1_input_bytes(dto->source_sha256,32,&source,&source_root)) goto done;
    owner=dynamic_registry();
    signature=beskid_rt_v5_gc_resolve_handle(signature_root);digest=beskid_rt_v5_gc_resolve_handle(digest_root);source=beskid_rt_v5_gc_resolve_handle(source_root);
    if(!owner || !signature || !digest || !source) goto done;
    for(size_t i=0;i<count;i++) {
        void *existing=beskid_dynamic_v1_registry_shape_at(owner,i);
        if(beskid_dynamic_v1_shape_tag(existing,3)==dto->library &&
            bytes_equal(beskid_dynamic_v1_shape_digest(existing),dto->signature_sha256,32)) {
            if(!bytes_equal(beskid_dynamic_v1_shape_signature(existing),dto->signature,dto->signature_length) ||
                !bytes_equal(beskid_dynamic_v1_shape_source(existing),dto->source_sha256,32) ||
                beskid_dynamic_v1_shape_tag(existing,1)!=dto->generation ||
                beskid_dynamic_v1_shape_pointer(existing,0)!=dto->payload_descriptor ||
                beskid_dynamic_v1_shape_pointer(existing,1)!=dto->cell_descriptor ||
                beskid_dynamic_v1_shape_pointer(existing,2)!=constructor ||
                beskid_dynamic_v1_shape_pointer(existing,3)!=reader) { status=OWNER_INVALID; goto done; }
            *out=beskid_dynamic_v1_shape_tag(existing,2); status=OWNER_OK; goto done;
        }
    }
    uint64_t tag=beskid_glue_v1_next_identity();
    if(!tag || tag>UINT32_MAX) goto done;
    void *shape=beskid_dynamic_v1_shape_construct(owner,signature,digest,dto->payload_descriptor,dto->cell_descriptor,
        constructor,reader,source,dto->generation,dto->library,tag);
    shape_root=shape ? gc_root_handle(shape) : 0;
    if(!shape_root) goto done;
    owner=dynamic_registry();shape=beskid_rt_v5_gc_resolve_handle(shape_root);
    if(!owner || !shape) goto done;
    beskid_dynamic_v1_registry_append_shape(owner,shape);
    *out=tag; status=OWNER_OK;
done:
    if(shape_root) gc_unroot_handle(shape_root);
    if(source_root) gc_unroot_handle(source_root);
    if(digest_root) gc_unroot_handle(digest_root);
    if(signature_root) gc_unroot_handle(signature_root);
    atomic_flag_clear_explicit(&owner_operation,memory_order_release); return status;
}
static void *dynamic_callback_address(void *shape,void *ordinary,uint64_t role) {
    if(!beskid_rt_v5_checked_scope_current()) return ordinary;
    return admitted_checked_address(beskid_dynamic_v1_shape_tag(shape,3),
        beskid_dynamic_v1_shape_tag(shape,1),ordinary,role);
}
static int32_t dynamic_cell(void *cell,uint64_t tag,void **out,int create) {
    if(!out || (uintptr_t)out % _Alignof(void *)) return OWNER_INVALID;
    *out=NULL;
    if(!cell || !tag) return OWNER_INVALID;
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    int32_t status=OWNER_FOREIGN; size_t root=0;
    void *shape=dynamic_shape(tag);
    if(!shape || !(root=gc_root_handle(cell))) goto done;
    cell=beskid_rt_v5_gc_resolve_handle(root);shape=dynamic_shape(tag);
    if(!cell || !shape) goto done;
    void *descriptor=NULL; memcpy(&descriptor,cell,sizeof descriptor);
    if(create) {
        if(descriptor!=beskid_dynamic_v1_shape_pointer(shape,0)) goto done;
        BeskidDynamicConstructV1 construct=NULL; void *address=dynamic_callback_address(shape,beskid_dynamic_v1_shape_pointer(shape,2),3);
        if(!address) goto done;
        memcpy(&construct,&address,sizeof construct);
        void *result=construct(cell,shape);
        if(beskid_rt_v5_checked_scope_current() && beskid_rt_v5_checked_scope_failure_reason()) goto done;
        size_t retained=result ? gc_root_handle(result) : 0;
        if(!retained) goto done;
        result=beskid_rt_v5_gc_resolve_handle(retained);shape=dynamic_shape(tag);
        if(!result || !shape) {gc_unroot_handle(retained);goto done;}
        void *actual=NULL,*actual_shape=NULL; uint64_t actual_tag=0;
        memcpy(&actual,result,sizeof actual); memcpy(&actual_shape,(uint8_t *)result+24,sizeof actual_shape);
        memcpy(&actual_tag,(uint8_t *)result+32,sizeof actual_tag);
        if(actual==beskid_dynamic_v1_shape_pointer(shape,1) && actual_shape==shape && actual_tag==tag) { *out=result; status=OWNER_OK; }
        gc_unroot_handle(retained);
    } else {
        void *actual_shape=NULL; uint64_t actual_tag=0;
        if(descriptor!=beskid_dynamic_v1_shape_pointer(shape,1)) goto done;
        memcpy(&actual_shape,(uint8_t *)cell+24,sizeof actual_shape); memcpy(&actual_tag,(uint8_t *)cell+32,sizeof actual_tag);
        if(actual_shape!=shape || actual_tag!=tag) goto done;
        BeskidDynamicReadV1 read=NULL; void *address=dynamic_callback_address(shape,beskid_dynamic_v1_shape_pointer(shape,3),4); if(!address) goto done; memcpy(&read,&address,sizeof read);
        void *payload=read(cell);
        if(beskid_rt_v5_checked_scope_current() && beskid_rt_v5_checked_scope_failure_reason()) goto done;
        if(!payload) goto done;
        size_t retained=gc_root_handle(payload);
        if(!retained) goto done;
        payload=beskid_rt_v5_gc_resolve_handle(retained);shape=dynamic_shape(tag);
        if(!payload || !shape) {gc_unroot_handle(retained);goto done;}
        void *actual=NULL; memcpy(&actual,payload,sizeof actual);
        if(actual==beskid_dynamic_v1_shape_pointer(shape,0)) { *out=payload; status=OWNER_OK; }
        gc_unroot_handle(retained);
    }
done:
    if(root) gc_unroot_handle(root);
    atomic_flag_clear_explicit(&owner_operation,memory_order_release); return status;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_dynamic_v1_create(void *payload,uint64_t tag,void **out) { return dynamic_cell(payload,tag,out,1); }
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_dynamic_v1_cast(void *cell,uint64_t tag,void **out) { return dynamic_cell(cell,tag,out,0); }

BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_dynamic_v1_register_mapping(const BeskidDynamicMappingRegistrationV1 *dto,uint64_t *out) {
    if(!out || (uintptr_t)out % _Alignof(uint64_t)) return OWNER_INVALID;
    *out=0;
    _Static_assert(sizeof(BeskidDynamicMappingRegistrationV1)==64,"Dynamic mapping V1 DTO");
    if(!dto || (uintptr_t)dto % _Alignof(BeskidDynamicMappingRegistrationV1) || dto->version!=1 || dto->size!=sizeof(*dto) ||
        !dto->library || !dto->generation || !dto->source_tag || !dto->destination_tag || !dto->transform ||
        !dto->signature || !dto->signature_length || dto->signature_length>1048576) return OWNER_INVALID;
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    int32_t status=OWNER_FOREIGN; size_t signature_root=0,mapping_root=0,records=0;
    uint64_t session=0,issuer=0;
    if(!admit_library(dto->library,dto->generation,&session,&issuer,&records)) goto done;
    void *owner=dynamic_registry(),*source=dynamic_shape(dto->source_tag),*destination=dynamic_shape(dto->destination_tag);
    if(!owner || !source || !destination || beskid_dynamic_v1_shape_tag(source,1)!=dto->generation ||
        beskid_dynamic_v1_shape_tag(destination,1)!=dto->generation) goto done;
    size_t count=beskid_dynamic_v1_registry_mapping_count(owner);
    if(count>=65536 || !dynamic_signature_budget(owner,dto->signature_length)) goto done;
    void *address=NULL; memcpy(&address,&dto->transform,sizeof address);
    for(size_t i=0;i<count;i++) {
        void *mapping=beskid_dynamic_v1_registry_mapping_at(owner,i);
        if(beskid_dynamic_v1_mapping_tag(mapping,2)==dto->library &&
            beskid_dynamic_v1_mapping_source(mapping)==source && beskid_dynamic_v1_mapping_destination(mapping)==destination &&
            bytes_equal(beskid_dynamic_v1_mapping_signature(mapping),dto->signature,dto->signature_length)) {
            if(beskid_dynamic_v1_mapping_tag(mapping,0)!=dto->generation || beskid_dynamic_v1_mapping_transform(mapping)!=address) {
                status=OWNER_INVALID; goto done;
            }
            *out=beskid_dynamic_v1_mapping_tag(mapping,1); status=OWNER_OK; goto done;
        }
    }
    void *signature=NULL;
    if(beskid_glue_v1_input_bytes(dto->signature,dto->signature_length,&signature,&signature_root)) goto done;
    void *digest=beskid_dynamic_v1_signature_digest(signature); const uint8_t *digest_bytes=NULL; size_t digest_length=0;
    if(!digest) goto done;
    memcpy(&digest_bytes,digest,sizeof digest_bytes);memcpy(&digest_length,(uint8_t *)digest+sizeof(void *),sizeof digest_length);
    if(!digest_bytes || digest_length!=32 ||
        !admitted_image_address(dto->library,dto->generation,NULL,digest_bytes,address,5)) goto done;
    owner=dynamic_registry();source=dynamic_shape(dto->source_tag);destination=dynamic_shape(dto->destination_tag);
    signature=beskid_rt_v5_gc_resolve_handle(signature_root);
    if(!owner || !source || !destination || !signature) goto done;
    uint64_t token=beskid_glue_v1_next_identity();
    if(!token) goto done;
    void *mapping=beskid_dynamic_v1_mapping_construct(source,destination,signature,address,dto->generation,dto->library,token);
    mapping_root=mapping ? gc_root_handle(mapping) : 0;
    if(!mapping_root) goto done;
    owner=dynamic_registry();mapping=beskid_rt_v5_gc_resolve_handle(mapping_root);
    if(!owner || !mapping) goto done;
    beskid_dynamic_v1_registry_append_mapping(owner,mapping);
    *out=token; status=OWNER_OK;
done:
    if(mapping_root) gc_unroot_handle(mapping_root);
    if(signature_root) gc_unroot_handle(signature_root);
    atomic_flag_clear_explicit(&owner_operation,memory_order_release); return status;
}
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_dynamic_v1_map(void *cell,uint64_t token,void **out) {
    if(!out || (uintptr_t)out % _Alignof(void *)) return OWNER_INVALID;
    *out=NULL;
    if(!cell || !token) return OWNER_INVALID;
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return OWNER_BUSY;
    int32_t status=OWNER_FOREIGN; size_t root=0,original_root=0,payload_root=0,result_root=0;
    void *owner=dynamic_registry(),*mapping=NULL;
    if(!owner) goto done;
    size_t count=beskid_dynamic_v1_registry_mapping_count(owner);
    if(count>65536) goto done;
    for(size_t i=0;i<count;i++) {
        void *candidate=beskid_dynamic_v1_registry_mapping_at(owner,i);
        if(beskid_dynamic_v1_mapping_tag(candidate,1)==token) { mapping=candidate; break; }
    }
    if(!mapping || !(root=gc_root_handle(cell))) goto done;
    void *source=beskid_dynamic_v1_mapping_source(mapping),*destination=beskid_dynamic_v1_mapping_destination(mapping);
    uint64_t session=0,issuer=0; size_t records=0;
    if(!admit_library(beskid_dynamic_v1_mapping_tag(mapping,2),beskid_dynamic_v1_mapping_tag(mapping,0),
        &session,&issuer,&records) ||
        dynamic_shape(beskid_dynamic_v1_shape_tag(source,2))!=source ||
        dynamic_shape(beskid_dynamic_v1_shape_tag(destination,2))!=destination) goto done;
    if(beskid_dynamic_v1_shape_owner(source)!=owner || beskid_dynamic_v1_shape_owner(destination)!=owner ||
        beskid_dynamic_v1_mapping_tag(mapping,0)!=beskid_dynamic_v1_shape_tag(source,1) ||
        beskid_dynamic_v1_mapping_tag(mapping,0)!=beskid_dynamic_v1_shape_tag(destination,1)) goto done;
    uint64_t source_tag=beskid_dynamic_v1_shape_tag(source,2),destination_tag=beskid_dynamic_v1_shape_tag(destination,2);
    void *transform_address=beskid_dynamic_v1_mapping_transform(mapping);
    uint64_t mapping_library=beskid_dynamic_v1_mapping_tag(mapping,2);
    uint64_t mapping_generation=beskid_dynamic_v1_mapping_tag(mapping,0);
    cell=beskid_rt_v5_gc_resolve_handle(root);
    if(!cell) goto done;
    void *descriptor=NULL,*shape=NULL; uint64_t tag=0;
    memcpy(&descriptor,cell,sizeof descriptor);
    if(descriptor!=beskid_dynamic_v1_shape_pointer(source,1)) goto done;
    memcpy(&shape,(uint8_t *)cell+24,sizeof shape); memcpy(&tag,(uint8_t *)cell+32,sizeof tag);
    if(shape!=source || tag!=beskid_dynamic_v1_shape_tag(source,2)) goto done;
    BeskidDynamicReadV1 read=NULL; void *address=dynamic_callback_address(source,beskid_dynamic_v1_shape_pointer(source,3),4); if(!address) goto done; memcpy(&read,&address,sizeof read);
    void *original=read(cell);
    if(beskid_rt_v5_checked_scope_current() && beskid_rt_v5_checked_scope_failure_reason()) goto done;
    if(!original || !(original_root=gc_root_handle(original))) goto done;
    original=beskid_rt_v5_gc_resolve_handle(original_root);source=dynamic_shape(source_tag);
    if(!original || !source) goto done;
    memcpy(&descriptor,original,sizeof descriptor);
    if(descriptor!=beskid_dynamic_v1_shape_pointer(source,0)) goto done;
    if(beskid_rt_v5_checked_scope_current()) {
        transform_address=admitted_checked_address(mapping_library,mapping_generation,transform_address,5);
        if(!transform_address) goto done;
    }
    BeskidDynamicTransformV1 transform=NULL; memcpy(&transform,&transform_address,sizeof transform);
    void *payload=transform(original);
    if(beskid_rt_v5_checked_scope_current() && beskid_rt_v5_checked_scope_failure_reason()) goto done;
    original=beskid_rt_v5_gc_resolve_handle(original_root);
    if(!payload || payload==original || !(payload_root=gc_root_handle(payload))) goto done;
    payload=beskid_rt_v5_gc_resolve_handle(payload_root);destination=dynamic_shape(destination_tag);
    if(!payload || !destination) goto done;
    memcpy(&descriptor,payload,sizeof descriptor);
    if(descriptor!=beskid_dynamic_v1_shape_pointer(destination,0)) goto done;
    BeskidDynamicConstructV1 construct=NULL; address=dynamic_callback_address(destination,beskid_dynamic_v1_shape_pointer(destination,2),3); if(!address) goto done; memcpy(&construct,&address,sizeof construct);
    void *result=construct(payload,destination);
    if(beskid_rt_v5_checked_scope_current() && beskid_rt_v5_checked_scope_failure_reason()) goto done;
    cell=beskid_rt_v5_gc_resolve_handle(root);
    if(!result || result==cell || !(result_root=gc_root_handle(result))) goto done;
    result=beskid_rt_v5_gc_resolve_handle(result_root);destination=dynamic_shape(destination_tag);
    if(!result || !destination) goto done;
    memcpy(&descriptor,result,sizeof descriptor); memcpy(&shape,(uint8_t *)result+24,sizeof shape); memcpy(&tag,(uint8_t *)result+32,sizeof tag);
    if(descriptor!=beskid_dynamic_v1_shape_pointer(destination,1) || shape!=destination ||
        tag!=beskid_dynamic_v1_shape_tag(destination,2)) goto done;
    *out=result; status=OWNER_OK;
done:
    if(result_root) gc_unroot_handle(result_root);
    if(payload_root) gc_unroot_handle(payload_root);
    if(original_root) gc_unroot_handle(original_root);
    if(root) gc_unroot_handle(root);
    atomic_flag_clear_explicit(&owner_operation,memory_order_release); return status;
}
BESKID_GLUE_PROVIDER_EXPORT uint8_t beskid_dynamic_v1_shutdown(void) {
    if(atomic_flag_test_and_set_explicit(&owner_operation,memory_order_acquire)) return 0;
    void *owner=beskid_dynamic_v1_root_load();
    if(owner) beskid_dynamic_v1_registry_close(owner);
    beskid_dynamic_v1_root_clear();
    atomic_flag_clear_explicit(&owner_operation,memory_order_release); return 1;
}
uint8_t beskid_rt_v5_intrinsic_dynamic_registry_shutdown_v1(void) { return beskid_dynamic_v1_shutdown(); }
/* Insert in the single canonical provider, inside its runtime guard. */
static void *dynamic_owned(void *value,uint64_t token,size_t *status,int operation) {
    if(!status || (uintptr_t)status % _Alignof(size_t)) return NULL;
    *status=OWNER_INVALID;
    void *result=NULL;
    int32_t code=operation==0 ? beskid_dynamic_v1_create(value,token,&result) :
        operation==1 ? beskid_dynamic_v1_cast(value,token,&result) : beskid_dynamic_v1_map(value,token,&result);
    *status=(size_t)code;
    return code==OWNER_OK ? result : NULL;
}
BESKID_GLUE_PROVIDER_EXPORT void *beskid_dynamic_v1_create_owned(void *payload,uint64_t tag,size_t *status) {
    return dynamic_owned(payload,tag,status,0);
}
BESKID_GLUE_PROVIDER_EXPORT void *beskid_dynamic_v1_cast_owned(void *cell,uint64_t tag,size_t *status) {
    return dynamic_owned(cell,tag,status,1);
}
BESKID_GLUE_PROVIDER_EXPORT void *beskid_dynamic_v1_map_owned(void *cell,uint64_t mapping,size_t *status) {
    return dynamic_owned(cell,mapping,status,2);
}
/* Source-issued guarded Result factories consume stack-owned status. An active
 * checked operation never allocates scratch storage or calls an ordinary Result
 * constructor which could store through a failed allocation. */
extern void *beskid_dynamic_v1_checked_create_result_factory(void *,uint64_t,size_t *);
extern void *beskid_dynamic_v1_checked_cast_result_factory(void *,uint64_t,size_t *);
extern void *beskid_dynamic_v1_checked_map_result_factory(void *,uint64_t,size_t *);
static void *dynamic_checked_result(void *value,uint64_t token,int operation) {
    if(!beskid_rt_v5_checked_scope_current() || beskid_rt_v5_checked_scope_failure_reason()) return NULL;
    size_t status=OWNER_INVALID;
    void *result=operation==0 ? beskid_dynamic_v1_checked_create_result_factory(value,token,&status) :
        operation==1 ? beskid_dynamic_v1_checked_cast_result_factory(value,token,&status) :
                       beskid_dynamic_v1_checked_map_result_factory(value,token,&status);
    if(beskid_rt_v5_checked_scope_failure_reason()) return NULL;
    return result;
}
BESKID_GLUE_PROVIDER_EXPORT void *beskid_dynamic_v1_checked_create_result_dispatch(void *value,uint64_t tag) {
    return dynamic_checked_result(value,tag,0);
}
BESKID_GLUE_PROVIDER_EXPORT void *beskid_dynamic_v1_checked_cast_result_dispatch(void *value,uint64_t tag) {
    return dynamic_checked_result(value,tag,1);
}
BESKID_GLUE_PROVIDER_EXPORT void *beskid_dynamic_v1_checked_map_result_dispatch(void *value,uint64_t token) {
    return dynamic_checked_result(value,token,2);
}

#endif
