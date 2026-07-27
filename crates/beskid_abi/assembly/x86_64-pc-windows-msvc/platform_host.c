#include <stdint.h>
#include <stddef.h>
#include <windows.h>

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
