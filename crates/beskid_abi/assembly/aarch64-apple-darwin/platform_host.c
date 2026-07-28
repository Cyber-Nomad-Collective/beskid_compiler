#define _DARWIN_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <stddef.h>
#include <stdint.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>

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
