#ifndef BESKID_GLUE_OWNER_IDENTITY_V1_H
#define BESKID_GLUE_OWNER_IDENTITY_V1_H
#include <stdint.h>
#include <stddef.h>
#ifdef _WIN32
#define BESKID_GLUE_PROVIDER_EXPORT __declspec(dllexport)
#else
#define BESKID_GLUE_PROVIDER_EXPORT __attribute__((visibility("default")))
#endif
/* One canonical shared provider for the process lifetime. Zero is permanent exhaustion. */
BESKID_GLUE_PROVIDER_EXPORT uint64_t beskid_glue_v1_next_identity(void);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_host_open(void **runtime_out);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_host_close(void *runtime);
typedef struct { const uint8_t *pointer; size_t length; uint64_t token; } GlueOwnedViewV1;
typedef int32_t (*BeskidGlueOpaqueDestructorV1)(void *);
typedef struct {
    uint8_t consumer_brand[32];
    uint64_t owner_library;
    uint8_t owner_brand[32];
    uint64_t owner_generation;
} BeskidGlueOpaqueDomainRowV1;
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_bind_opaque_domains(uint64_t,uint64_t,const BeskidGlueOpaqueDomainRowV1 *,size_t);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_opaque_create(uint64_t,const uint8_t *,void *,BeskidGlueOpaqueDestructorV1,uint64_t *);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_opaque_borrow_begin(uint64_t,const uint8_t *,uint64_t,void **,uint64_t *);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_opaque_borrow_end(uint64_t,const uint8_t *,uint64_t);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_opaque_release(uint64_t,const uint8_t *,uint64_t);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_validate_opaque(uint64_t,const uint8_t *,uint64_t);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_release_consumer_opaque(uint64_t,const uint8_t *,uint64_t);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_open_library(uint64_t generation, uint64_t *library_out);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_close_library(uint64_t library, uint64_t generation);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_release_token(uint64_t library, uint64_t token);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_validate_binding(uint64_t library, uint64_t shape);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_bind_shapes(uint64_t library, uint64_t generation, const uint64_t *shapes, size_t count);
/* Loader-only native transport; never a portable value or public shape grant.
 * Roles: payload descriptor=1, cell descriptor=2, constructor=3, reader=4,
 * transform=5, checked opaque destructor=6;
 * source-issued checked counterparts use roles13/14/15 respectively.
 * Each checked counterpart shares the ordinary row source/signature identity.
 * Addresses are admitted only after exact pinned image ownership. */
typedef struct {
    uint8_t source_sha256[32];
    uint8_t signature_sha256[32];
    uint64_t address;
    uint64_t role;
} BeskidGlueImageClosureRowV1;
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_bind_image_closure(uint64_t library, uint64_t generation, const BeskidGlueImageClosureRowV1 *rows, size_t count);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_copy(uint64_t library, uint64_t shape, uint64_t generation, uint64_t kind, const uint8_t *bytes, size_t length, GlueOwnedViewV1 *out);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_glue_v1_owner_release(uint64_t library, uint64_t shape, uint64_t generation, uint64_t kind, uint64_t token);

typedef void *(*BeskidDynamicConstructV1)(void *payload, void *shape_record);
typedef void *(*BeskidDynamicReadV1)(void *cell);
typedef struct {
    uint32_t version;
    uint32_t size;
    uint64_t library;
    uint64_t generation;
    uint8_t source_sha256[32];
    uint8_t signature_sha256[32];
    const uint8_t *signature;
    size_t signature_length;
    void *payload_descriptor;
    void *cell_descriptor;
    BeskidDynamicConstructV1 construct;
    BeskidDynamicReadV1 read;
} BeskidDynamicShapeRegistrationV1;
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_dynamic_v1_register_shape(const BeskidDynamicShapeRegistrationV1 *, uint64_t *);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_dynamic_v1_create(void *,uint64_t,void **);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_dynamic_v1_cast(void *,uint64_t,void **);
BESKID_GLUE_PROVIDER_EXPORT uint8_t beskid_dynamic_v1_shutdown(void);


typedef void *(*BeskidDynamicTransformV1)(void *source_box);
typedef struct {
    uint32_t version;
    uint32_t size;
    uint64_t library;
    uint64_t generation;
    uint64_t source_tag;
    uint64_t destination_tag;
    const uint8_t *signature;
    size_t signature_length;
    BeskidDynamicTransformV1 transform;
} BeskidDynamicMappingRegistrationV1;
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_dynamic_v1_register_mapping(const BeskidDynamicMappingRegistrationV1 *,uint64_t *);
BESKID_GLUE_PROVIDER_EXPORT int32_t beskid_dynamic_v1_map(void *,uint64_t,void **);
BESKID_GLUE_PROVIDER_EXPORT void *beskid_dynamic_v1_create_owned(void *,uint64_t,size_t *);
BESKID_GLUE_PROVIDER_EXPORT void *beskid_dynamic_v1_cast_owned(void *,uint64_t,size_t *);
BESKID_GLUE_PROVIDER_EXPORT void *beskid_dynamic_v1_map_owned(void *,uint64_t,size_t *);

/* Compiler-issued checked facade dispatch; no standalone source recovery grant. */
BESKID_GLUE_PROVIDER_EXPORT void *beskid_dynamic_v1_checked_create_result_dispatch(void *,uint64_t);
BESKID_GLUE_PROVIDER_EXPORT void *beskid_dynamic_v1_checked_cast_result_dispatch(void *,uint64_t);
BESKID_GLUE_PROVIDER_EXPORT void *beskid_dynamic_v1_checked_map_result_dispatch(void *,uint64_t);

#endif
