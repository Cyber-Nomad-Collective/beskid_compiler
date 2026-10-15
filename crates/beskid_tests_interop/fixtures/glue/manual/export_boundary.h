#ifndef BESKID_GLUE_MANUAL_EXPORT_H
#define BESKID_GLUE_MANUAL_EXPORT_H
#include <stddef.h>
#include <stdint.h>
typedef struct { const uint8_t *pointer; size_t length; uint64_t token; } GlueOwnedView;
_Static_assert(sizeof(GlueOwnedView) == 24, "owned view size");
_Static_assert(_Alignof(GlueOwnedView) == 8, "owned view alignment");
_Static_assert(offsetof(GlueOwnedView, pointer) == 0, "pointer offset");
_Static_assert(offsetof(GlueOwnedView, length) == 8, "length offset");
_Static_assert(offsetof(GlueOwnedView, token) == 16, "token offset");
int8_t beskid_i8(int8_t); int16_t beskid_i16(int16_t);
int32_t beskid_i32(int32_t); int64_t beskid_i64(int64_t);
uint8_t beskid_u8(uint8_t); uint16_t beskid_u16(uint16_t);
uint32_t beskid_u32(uint32_t); uint64_t beskid_u64(uint64_t);
float beskid_f32(float); double beskid_f64(double);
uint8_t beskid_bool(uint8_t); uint32_t beskid_char(uint32_t);
int32_t beskid_utf8(const uint8_t *, size_t, GlueOwnedView *);
int32_t beskid_bytes(const uint8_t *, size_t, GlueOwnedView *);
int32_t beskid_glue_release_24b6992e581aa5005a6d6dbe54772382480884d151090eb990289bd7a5bcc439(uint64_t);
uint64_t beskid_handle(uint64_t); uintptr_t beskid_native_width(uintptr_t);
void beskid_unit(void);
#endif
