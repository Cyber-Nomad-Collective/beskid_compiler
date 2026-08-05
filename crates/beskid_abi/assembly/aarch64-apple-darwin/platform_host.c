#define _DARWIN_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <stddef.h>
#include <stdint.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#include "../../include/beskid_runtime_abi_v5.h"

struct BeskidStr { const uint8_t *ptr; size_t len; };
struct BeskidArgsState { int64_t count; struct BeskidStr *values; };
static struct BeskidArgsState beskid_args;

static _Noreturn void beskid_args_trap(uint8_t code, const char *message) {
    beskid_rt_v5_trap(code, (void *)message, __builtin_strlen(message));
}

void beskid_rt_v5_args_handoff_utf8(int64_t argc, const char *const *argv) {
    if (argc < 0 || (argc != 0 && argv == NULL)) beskid_args_trap(10, "Core.Args handoff is invalid");
    size_t headers = (size_t)argc * sizeof(struct BeskidStr);
    if (argc != 0 && headers / sizeof(struct BeskidStr) != (size_t)argc) beskid_args_trap(5, "Core.Args storage allocation failed");
    size_t bytes = 0;
    for (int64_t i = 0; i < argc; ++i) { if (argv[i] == NULL) beskid_args_trap(10, "Core.Args handoff is invalid"); size_t len = __builtin_strlen(argv[i]); if (len > SIZE_MAX - bytes) beskid_args_trap(5, "Core.Args storage allocation failed"); bytes += len; }
    if (headers > SIZE_MAX - bytes) beskid_args_trap(5, "Core.Args storage allocation failed");
    size_t total = headers + bytes; if (total == 0) total = 1;
    unsigned char *storage = mmap(NULL, total, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANON, -1, 0);
    if (storage == MAP_FAILED) beskid_args_trap(5, "Core.Args storage allocation failed");
    struct BeskidStr *values = (struct BeskidStr *)storage; unsigned char *cursor = storage + headers;
    for (int64_t i = 0; i < argc; ++i) { size_t len = __builtin_strlen(argv[i]); __builtin_memcpy(cursor, argv[i], len); values[i] = (struct BeskidStr){ .ptr = cursor, .len = len }; cursor += len; }
    beskid_args = (struct BeskidArgsState){ .count = argc, .values = values };
}

int64_t beskid_rt_v5_args_count(void) { return beskid_args.count; }
struct BeskidStr *beskid_rt_v5_args_get(int64_t index) { if (index < 0 || index >= beskid_args.count) beskid_args_trap(2, "Core.Args argument index is out of range"); return &beskid_args.values[index]; }

#ifndef MAP_ANON
#ifdef MAP_ANONYMOUS
#define MAP_ANON MAP_ANONYMOUS
#else
#define MAP_ANON 0x1000
#endif
#endif

#define BESKID_GUARDED_STACK_MIN (64u * 1024u)
#define BESKID_GUARDED_STACK_MAX (8u * 1024u * 1024u)
#define BESKID_GUARDED_STACK_GUARD (16u * 1024u)

static int beskid_guarded_stack_size_valid(size_t usable_size) {
    return usable_size >= BESKID_GUARDED_STACK_MIN && usable_size <= BESKID_GUARDED_STACK_MAX
        && usable_size % BESKID_GUARDED_STACK_GUARD == 0;
}

void *beskid_rt_v5_intrinsic_guarded_stack_allocate(size_t usable_size) {
    if (!beskid_guarded_stack_size_valid(usable_size)) return NULL;
    size_t total_size = usable_size + BESKID_GUARDED_STACK_GUARD;
    if (total_size < usable_size) return NULL;
    void *reservation = mmap(NULL, total_size, PROT_NONE, MAP_PRIVATE | MAP_ANON, -1, 0);
    if (reservation == MAP_FAILED) return NULL;
    unsigned char *usable_base = (unsigned char *)reservation + BESKID_GUARDED_STACK_GUARD;
    if (mprotect(usable_base, usable_size, PROT_READ | PROT_WRITE) != 0) {
        (void)munmap(reservation, total_size);
        return NULL;
    }
    return usable_base;
}

void beskid_rt_v5_intrinsic_guarded_stack_free(void *usable_base, size_t usable_size) {
    if (usable_base == NULL || !beskid_guarded_stack_size_valid(usable_size)) return;
    unsigned char *reservation = (unsigned char *)usable_base - BESKID_GUARDED_STACK_GUARD;
    (void)munmap(reservation, usable_size + BESKID_GUARDED_STACK_GUARD);
}

static int64_t beskid_clock_nanos(clockid_t clock_id) {
    struct timespec value;
    if (clock_gettime(clock_id, &value) != 0) return 0;
    return (int64_t)value.tv_sec * INT64_C(1000000000) + value.tv_nsec;
}

int64_t beskid_rt_v5_intrinsic_clock_monotonic_nanos(void) { return beskid_clock_nanos(CLOCK_MONOTONIC); }
int64_t beskid_rt_v5_intrinsic_clock_realtime_nanos(void) { return beskid_clock_nanos(CLOCK_REALTIME); }
void beskid_rt_v5_intrinsic_process_exit(int32_t code) { _exit(code); }
int32_t beskid_rt_v5_intrinsic_process_getpid(void) { return (int32_t)getpid(); }
