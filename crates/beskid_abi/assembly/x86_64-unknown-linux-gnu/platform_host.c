#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <time.h>
#include <unistd.h>

static int64_t beskid_clock_nanos(clockid_t clock_id) {
    struct timespec value;
    if (clock_gettime(clock_id, &value) != 0) return 0;
    return (int64_t)value.tv_sec * INT64_C(1000000000) + value.tv_nsec;
}

int64_t beskid_rt_v5_intrinsic_clock_monotonic_nanos(void) { return beskid_clock_nanos(CLOCK_MONOTONIC); }
int64_t beskid_rt_v5_intrinsic_clock_realtime_nanos(void) { return beskid_clock_nanos(CLOCK_REALTIME); }
void beskid_rt_v5_intrinsic_process_exit(int32_t code) { _exit(code); }
int32_t beskid_rt_v5_intrinsic_process_getpid(void) { return (int32_t)getpid(); }
