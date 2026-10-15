#ifndef GLUE_MANUAL_H
#define GLUE_MANUAL_H
#include <stdint.h>
#include <stddef.h>
int8_t glue_i8(int8_t value);
int16_t glue_i16(int16_t value);
int32_t glue_i32(int32_t value);
int64_t glue_i64(int64_t value);
uint8_t glue_u8(uint8_t value);
uint16_t glue_u16(uint16_t value);
uint32_t glue_u32(uint32_t value);
uint64_t glue_u64(uint64_t value);
float glue_f32(float value);
double glue_f64(double value);
int32_t glue_bool(uint8_t);
int32_t glue_char(uint32_t);
int32_t glue_utf8(const uint8_t*, size_t);
int32_t glue_bytes(const uint8_t*, size_t);
void glue_unit(void);
int32_t glue_native_width(uintptr_t, uint32_t);
uint64_t glue_allocate(size_t);
int32_t glue_handle(uint64_t,uint64_t,uint64_t);
int32_t glue_release(uint64_t);
int32_t glue_failure(uint32_t);
typedef struct { const uint8_t *pointer; size_t length; } GlueByteView;
int32_t glue_owned_view(uint64_t, GlueByteView*);
#endif
