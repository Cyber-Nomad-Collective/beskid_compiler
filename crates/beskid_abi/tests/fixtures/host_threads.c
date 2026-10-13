#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <fcntl.h>
#include <io.h>
#else
#define _DARWIN_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <sched.h>
#include <time.h>
#include <unistd.h>
#endif

#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <limits.h>
#include <stdint.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>

/* Native evidence for the isolate-prerequisite host primitives: thread create
 * and join, the five atomic word operations, the host logical processor count
 * and its failure fallback, the process state block, and the reference-counted
 * worker pool. The production headers are included unchanged below. */

static _Atomic size_t live_allocations;
void *beskid_rt_v5_intrinsic_system_allocate(size_t size, size_t alignment) {
    (void)alignment;
    void *result = malloc(size);
    if (result) atomic_fetch_add(&live_allocations, 1);
    return result;
}
void beskid_rt_v5_intrinsic_system_free(void *address, size_t size) {
    (void)size;
    if (address) atomic_fetch_sub(&live_allocations, 1);
    free(address);
}
int64_t beskid_rt_v5_intrinsic_clock_monotonic_nanos(void) {
#ifdef _WIN32
    return (int64_t)GetTickCount64() * INT64_C(1000000);
#else
    struct timespec time;
    assert(clock_gettime(CLOCK_MONOTONIC, &time) == 0);
    return time.tv_sec * INT64_C(1000000000) + time.tv_nsec;
#endif
}
_Noreturn void beskid_rt_v5_trap(uint8_t code, void *message, size_t length) {
    (void)code; (void)message; (void)length; abort();
}

/* Test hook: 0 uses the real host query, otherwise the substituted result. */
static long long processor_query_override;
static long long fixture_processor_query(void);
#define BESKID_HOST_PROCESSOR_QUERY() fixture_processor_query()

#include "../../assembly/common/external_wait.h"
#include "../../assembly/common/host_threads.h"

static long long fixture_processor_query(void) {
    if (processor_query_override) return processor_query_override;
#ifdef _WIN32
    return (long long)GetActiveProcessorCount(ALL_PROCESSOR_GROUPS);
#else
    return (long long)sysconf(_SC_NPROCESSORS_ONLN);
#endif
}

static void *write_forty_two(void *argument) {
    *(size_t *)argument = 42;
    return argument;
}

struct Counter { size_t value; };
static void *add_one_thousand(void *argument) {
    struct Counter *counter = (struct Counter *)argument;
    for (int i = 0; i < 1000; ++i) beskid_rt_v5_intrinsic_atomic_word_fetch_add(&counter->value, 1, 3);
    return NULL;
}

static void thread_create_and_join(void) {
    size_t cell = 0;
    void *handle = NULL;
    assert(beskid_rt_v5_intrinsic_thread_create((void *)write_forty_two, &cell, &handle) == 0);
    assert(handle != NULL);
    assert(beskid_rt_v5_intrinsic_thread_join(handle) == 0);
    assert(cell == 42);
    void *untouched = (void *)0x1;
    assert(beskid_rt_v5_intrinsic_thread_create(NULL, &cell, &untouched) != 0);
    assert(untouched == (void *)0x1);
    assert(beskid_rt_v5_intrinsic_thread_join(NULL) != 0);
    printf("thread=create-join\n");
}

static void atomic_words(void) {
    size_t cell = 7;
    assert(beskid_rt_v5_intrinsic_atomic_word_compare_exchange(&cell, 7, 9, 4) == 7);
    assert(cell == 9);
    assert(beskid_rt_v5_intrinsic_atomic_word_compare_exchange(&cell, 7, 11, 4) == 9);
    assert(cell == 9);
    for (uint8_t order = 0; order < 6; ++order) {
        beskid_rt_v5_intrinsic_atomic_word_store(&cell, 100 + order, order);
        assert(beskid_rt_v5_intrinsic_atomic_word_load(&cell, order) == (size_t)(100 + order));
        assert(beskid_rt_v5_intrinsic_atomic_word_fetch_add(&cell, 2, order) == (size_t)(100 + order));
        assert(beskid_rt_v5_intrinsic_atomic_word_compare_exchange(&cell, 102 + order, 1, order) == (size_t)(102 + order));
        assert(cell == 1);
        beskid_rt_v5_intrinsic_atomic_fence(order);
    }
    struct Counter counter = {0};
    void *first = NULL, *second = NULL;
    assert(beskid_rt_v5_intrinsic_thread_create((void *)add_one_thousand, &counter, &first) == 0);
    assert(beskid_rt_v5_intrinsic_thread_create((void *)add_one_thousand, &counter, &second) == 0);
    assert(beskid_rt_v5_intrinsic_thread_join(first) == 0);
    assert(beskid_rt_v5_intrinsic_thread_join(second) == 0);
    assert(beskid_rt_v5_intrinsic_atomic_word_load(&counter.value, 1) == 2000);
    printf("atomics=fetch-add-2000\n");
}

static void processor_count(void) {
    size_t host = beskid_rt_v5_host_processor_count();
    assert(host >= 1);
    processor_query_override = 8;
    assert(beskid_rt_v5_host_processor_count() == 8);
    processor_query_override = -1;
    assert(beskid_rt_v5_host_processor_count() == 1);
    processor_query_override = 0;
    assert(beskid_rt_v5_host_processor_count() == host);
    printf("processors=%zu\n", host);
}

static void process_state(void) {
    size_t *root = (size_t *)beskid_rt_v5_intrinsic_process_state_root();
    assert(root != NULL);
    assert(root == (size_t *)beskid_rt_v5_intrinsic_process_state_root());
    for (int i = 0; i < BESKID_PROCESS_STATE_WORDS; ++i) assert(root[i] == 0);
    printf("process-state=zeroed\n");
}

static void shared_pool(void) {
    assert(BeskidWorkerPoolHolders() == 0);
    assert(beskid_rt_v5_intrinsic_worker_pool_init(0) != 0);
    assert(beskid_rt_v5_intrinsic_worker_pool_init(BESKID_WORKER_MAX + 1) != 0);
    assert(BeskidWorkerPoolHolders() == 0);
    assert(beskid_rt_v5_intrinsic_worker_pool_init(4) == 0);
    assert(BeskidWorkerPoolHolders() == 1 && beskid_worker_count == 4);
    assert(beskid_rt_v5_intrinsic_worker_pool_init(4) == 0);
    assert(BeskidWorkerPoolHolders() == 2 && beskid_worker_count == 4);
    beskid_rt_v5_intrinsic_worker_pool_shutdown();
    assert(BeskidWorkerPoolHolders() == 1 && beskid_worker_count == 4);
    beskid_rt_v5_intrinsic_worker_pool_shutdown();
    assert(BeskidWorkerPoolHolders() == 0 && beskid_worker_count == 0);
    /* A release without a holder is a no-op, and the pool can be created again. */
    beskid_rt_v5_intrinsic_worker_pool_shutdown();
    assert(beskid_rt_v5_intrinsic_worker_pool_init(2) == 0);
    assert(BeskidWorkerPoolHolders() == 1 && beskid_worker_count == 2);
    beskid_rt_v5_intrinsic_worker_pool_shutdown();
    assert(BeskidWorkerPoolHolders() == 0 && beskid_worker_count == 0);
    printf("pool=shared-refcounted\n");
}

int main(void) {
    process_state();
    thread_create_and_join();
    atomic_words();
    processor_count();
    shared_pool();
    assert(atomic_load(&live_allocations) == 0);
    printf("host-threads=ok\n");
    return 0;
}
