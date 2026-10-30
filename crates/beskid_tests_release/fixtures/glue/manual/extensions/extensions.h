/* Hand-written v0.6.1 Glue extension C ABI: a record by value, a fieldless enum discriminant and
 * a call-scoped callback. The generated Glue adapters reproduce these shapes; this header fixes
 * the record layout that the C ABI profile computes (declaration order, natural alignment). */
#ifndef BESKID_GLUE_EXTENSIONS_H
#define BESKID_GLUE_EXTENSIONS_H
#include <stdint.h>
#include <stddef.h>

typedef struct { uint8_t tag; int64_t x; double weight; uint8_t visible; uint32_t mark; } ExtPoint;
_Static_assert(sizeof(ExtPoint) == 32, "ExtPoint size");
_Static_assert(_Alignof(ExtPoint) == 8, "ExtPoint alignment");
_Static_assert(offsetof(ExtPoint, tag) == 0, "ExtPoint.tag");
_Static_assert(offsetof(ExtPoint, x) == 8, "ExtPoint.x");
_Static_assert(offsetof(ExtPoint, weight) == 16, "ExtPoint.weight");
_Static_assert(offsetof(ExtPoint, visible) == 24, "ExtPoint.visible");
_Static_assert(offsetof(ExtPoint, mark) == 28, "ExtPoint.mark");

/* Source-order variant indices of a three-variant fieldless enum. */
enum { EXT_COLOR_RED = 0, EXT_COLOR_GREEN = 1, EXT_COLOR_BLUE = 2, EXT_COLOR_COUNT = 3 };

/* Callback: the context token first, then the arguments, then the result out parameter. Status 0
 * is success, 1 an invalid argument, 2 a token with no live frame on the calling thread. */
typedef int32_t (*ExtVisit)(uint64_t context, int64_t index, int64_t *out);

int32_t ext_manual_point(ExtPoint value, ExtPoint *out);
int32_t ext_manual_next_color(uint32_t value, uint32_t *out);
int32_t ext_manual_bad_color(uint32_t *out);
int32_t ext_manual_sum_each(int64_t count, ExtVisit visit, uint64_t context, int64_t *out);
int32_t ext_manual_retain(ExtVisit visit, uint64_t context);
int32_t ext_manual_invoke_retained(int64_t index, int64_t *out);
/* Writes size, alignment and the five field offsets of the library's ExtPoint. */
int32_t ext_manual_point_layout(uint64_t values[7]);
#endif
