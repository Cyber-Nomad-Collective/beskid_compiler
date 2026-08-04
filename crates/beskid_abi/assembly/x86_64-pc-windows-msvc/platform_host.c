#include <stdint.h>
#include <stddef.h>
#include <windows.h>
#include <string.h>
#include "../../include/beskid_runtime_abi_v5.h"

struct BeskidStr { const uint8_t *ptr; size_t len; };
struct BeskidArgsState { int64_t count; struct BeskidStr *values; };
static struct BeskidArgsState beskid_args;

static __declspec(noreturn) void beskid_args_trap(uint8_t code, const char *message) {
    beskid_rt_v5_trap(code, (void *)message, strlen(message));
}

static size_t beskid_utf8_length(const uint16_t *input) {
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

static unsigned char *beskid_write_utf8(unsigned char *out, const uint16_t *input) {
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

void beskid_rt_v5_args_handoff_utf16(int64_t argc, const uint16_t *const *argv) {
    if (argc < 0 || (argc != 0 && argv == NULL)) beskid_args_trap(10, "Core.Args handoff is invalid");
    size_t headers = (size_t)argc * sizeof(struct BeskidStr), bytes = 0;
    if (argc != 0 && headers / sizeof(struct BeskidStr) != (size_t)argc) beskid_args_trap(5, "Core.Args storage allocation failed");
    for (int64_t i = 0; i < argc; ++i) { if (argv[i] == NULL) beskid_args_trap(10, "Core.Args handoff is invalid"); size_t len = beskid_utf8_length(argv[i]); if (len > SIZE_MAX - bytes) beskid_args_trap(5, "Core.Args storage allocation failed"); bytes += len; }
    if (headers > SIZE_MAX - bytes) beskid_args_trap(5, "Core.Args storage allocation failed");
    size_t total = headers + bytes; if (total == 0) total = 1;
    unsigned char *storage = VirtualAlloc(NULL, total, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    if (storage == NULL) beskid_args_trap(5, "Core.Args storage allocation failed");
    struct BeskidStr *values = (struct BeskidStr *)storage; unsigned char *cursor = storage + headers;
    for (int64_t i = 0; i < argc; ++i) { unsigned char *start = cursor; cursor = beskid_write_utf8(cursor, argv[i]); values[i] = (struct BeskidStr){ .ptr = start, .len = (size_t)(cursor - start) }; }
    beskid_args = (struct BeskidArgsState){ .count = argc, .values = values };
}

int64_t beskid_rt_v5_args_count(void) { return beskid_args.count; }
struct BeskidStr *beskid_rt_v5_args_get(int64_t index) { if (index < 0 || index >= beskid_args.count) beskid_args_trap(2, "Core.Args argument index is out of range"); return &beskid_args.values[index]; }

#define BESKID_GUARDED_STACK_MIN (64u * 1024u)
#define BESKID_GUARDED_STACK_MAX (8u * 1024u * 1024u)
#define BESKID_GUARDED_STACK_GUARD (64u * 1024u)

static int beskid_guarded_stack_size_valid(size_t usable_size) {
    return usable_size >= BESKID_GUARDED_STACK_MIN && usable_size <= BESKID_GUARDED_STACK_MAX
        && usable_size % BESKID_GUARDED_STACK_GUARD == 0;
}

void *beskid_rt_v5_intrinsic_guarded_stack_allocate(size_t usable_size) {
    if (!beskid_guarded_stack_size_valid(usable_size)) return NULL;
    size_t total_size = usable_size + BESKID_GUARDED_STACK_GUARD;
    if (total_size < usable_size) return NULL;
    unsigned char *reservation = (unsigned char *)VirtualAlloc(NULL, total_size, MEM_RESERVE, PAGE_NOACCESS);
    if (reservation == NULL) return NULL;
    unsigned char *usable_base = reservation + BESKID_GUARDED_STACK_GUARD;
    if (VirtualAlloc(usable_base, usable_size, MEM_COMMIT, PAGE_READWRITE) == NULL) {
        (void)VirtualFree(reservation, 0, MEM_RELEASE);
        return NULL;
    }
    return usable_base;
}

void beskid_rt_v5_intrinsic_guarded_stack_free(void *usable_base, size_t usable_size) {
    if (usable_base == NULL || !beskid_guarded_stack_size_valid(usable_size)) return;
    unsigned char *reservation = (unsigned char *)usable_base - BESKID_GUARDED_STACK_GUARD;
    (void)VirtualFree(reservation, 0, MEM_RELEASE);
}

#pragma comment(lib, "kernel32.lib")

int64_t beskid_rt_v5_intrinsic_clock_monotonic_nanos(void) {
    return (int64_t)(GetTickCount64() * UINT64_C(1000000));
}

int64_t beskid_rt_v5_intrinsic_clock_realtime_nanos(void) {
    FILETIME value;
    ULARGE_INTEGER ticks;
    GetSystemTimeAsFileTime(&value);
    ticks.LowPart = value.dwLowDateTime;
    ticks.HighPart = value.dwHighDateTime;
    return (int64_t)((ticks.QuadPart - UINT64_C(116444736000000000)) * UINT64_C(100));
}

void beskid_rt_v5_intrinsic_process_exit(int32_t code) { ExitProcess((UINT)code); }
int32_t beskid_rt_v5_intrinsic_process_getpid(void) { return (int32_t)GetCurrentProcessId(); }
