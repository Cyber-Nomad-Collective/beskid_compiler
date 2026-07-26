#include <stdint.h>
#include <windows.h>

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
