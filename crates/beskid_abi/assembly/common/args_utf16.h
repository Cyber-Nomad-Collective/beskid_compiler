#ifndef BESKID_ARGS_UTF16_H
#define BESKID_ARGS_UTF16_H

#include <stddef.h>
#include <stdint.h>

static size_t beskid_args_utf8_length(const uint16_t *input) {
    size_t result = 0;
    for (size_t i = 0; input[i] != 0; ++i) {
        uint16_t unit = input[i];
        if (unit >= 0xD800 && unit <= 0xDBFF && input[i + 1] >= 0xDC00 && input[i + 1] <= 0xDFFF) { result += 4; ++i; }
        else if (unit >= 0xD800 && unit <= 0xDFFF) result += 3;
        else if (unit < 0x80) result += 1;
        else if (unit < 0x800) result += 2;
        else result += 3;
    }
    return result;
}

static unsigned char *beskid_args_write_utf8(unsigned char *out, const uint16_t *input) {
    for (size_t i = 0; input[i] != 0; ++i) {
        uint32_t scalar = input[i];
        if (scalar >= 0xD800 && scalar <= 0xDBFF && input[i + 1] >= 0xDC00 && input[i + 1] <= 0xDFFF) { scalar = 0x10000 + ((scalar - 0xD800) << 10) + (input[++i] - 0xDC00); }
        else if (scalar >= 0xD800 && scalar <= 0xDFFF) scalar = 0xFFFD;
        if (scalar < 0x80) *out++ = (unsigned char)scalar;
        else if (scalar < 0x800) { *out++ = 0xC0 | (scalar >> 6); *out++ = 0x80 | (scalar & 0x3F); }
        else if (scalar < 0x10000) { *out++ = 0xE0 | (scalar >> 12); *out++ = 0x80 | ((scalar >> 6) & 0x3F); *out++ = 0x80 | (scalar & 0x3F); }
        else { *out++ = 0xF0 | (scalar >> 18); *out++ = 0x80 | ((scalar >> 12) & 0x3F); *out++ = 0x80 | ((scalar >> 6) & 0x3F); *out++ = 0x80 | (scalar & 0x3F); }
    }
    return out;
}

#endif
