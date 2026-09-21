#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <process.h>
#else
#define _DARWIN_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
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

/* Test-only rendezvous at the actual OS wait boundary. The production mailbox,
 * lock, condition variable, and queue are included unchanged below. */
#ifdef _WIN32
typedef SRWLOCK FixtureMutex;
typedef CONDITION_VARIABLE FixtureCondition;
typedef HANDLE FixtureThread;
#define FIXTURE_MUTEX_INIT SRWLOCK_INIT
#define FIXTURE_CONDITION_INIT CONDITION_VARIABLE_INIT
static FixtureMutex gate = FIXTURE_MUTEX_INIT;
static FixtureCondition gate_changed = FIXTURE_CONDITION_INIT;
static void fixture_lock(FixtureMutex *lock) { AcquireSRWLockExclusive(lock); }
static void fixture_unlock(FixtureMutex *lock) { ReleaseSRWLockExclusive(lock); }
static void fixture_wait(FixtureCondition *condition, FixtureMutex *lock) {
    assert(SleepConditionVariableSRW(condition, lock, INFINITE, 0));
}
static void fixture_signal(FixtureCondition *condition) { WakeAllConditionVariable(condition); }
static void fixture_thread_start(FixtureThread *thread, unsigned (__stdcall *entry)(void *), void *argument) {
    uintptr_t handle = _beginthreadex(NULL, 0, entry, argument, 0, NULL);
    assert(handle);
    *thread = (HANDLE)handle;
}
static void fixture_thread_join(FixtureThread thread) {
    assert(WaitForSingleObject(thread, INFINITE) == WAIT_OBJECT_0);
    assert(CloseHandle(thread));
}
static void fixture_yield(void) { SwitchToThread(); }
static int64_t fixture_monotonic_nanos(void) { return (int64_t)GetTickCount64() * INT64_C(1000000); }
static void fixture_pipe_open(uintptr_t descriptors[2]) {
    HANDLE read_end, write_end;
    assert(CreatePipe(&read_end, &write_end, NULL, 0));
    assert(read_end != INVALID_HANDLE_VALUE && write_end != INVALID_HANDLE_VALUE);
    descriptors[0] = (uintptr_t)read_end;
    descriptors[1] = (uintptr_t)write_end;
}
static void fixture_pipe_write(uintptr_t descriptor, const void *bytes, size_t length) {
    DWORD written;
    assert(length <= UINT32_MAX);
    assert(WriteFile((HANDLE)descriptor, bytes, (DWORD)length, &written, NULL));
    assert(written == length);
}
static void fixture_pipe_close(uintptr_t descriptor) { assert(CloseHandle((HANDLE)descriptor)); }
#else
typedef pthread_mutex_t FixtureMutex;
typedef pthread_cond_t FixtureCondition;
typedef pthread_t FixtureThread;
#define FIXTURE_MUTEX_INIT PTHREAD_MUTEX_INITIALIZER
#define FIXTURE_CONDITION_INIT PTHREAD_COND_INITIALIZER
static FixtureMutex gate = FIXTURE_MUTEX_INIT;
static FixtureCondition gate_changed = FIXTURE_CONDITION_INIT;
static void fixture_lock(FixtureMutex *lock) { assert(pthread_mutex_lock(lock) == 0); }
static void fixture_unlock(FixtureMutex *lock) { assert(pthread_mutex_unlock(lock) == 0); }
static void fixture_wait(FixtureCondition *condition, FixtureMutex *lock) { assert(pthread_cond_wait(condition, lock) == 0); }
static void fixture_signal(FixtureCondition *condition) { assert(pthread_cond_broadcast(condition) == 0); }
static void fixture_thread_start(FixtureThread *thread, void *(*entry)(void *), void *argument) {
    assert(pthread_create(thread, NULL, entry, argument) == 0);
}
static void fixture_thread_join(FixtureThread thread) { assert(pthread_join(thread, NULL) == 0); }
static void fixture_yield(void) { sched_yield(); }
static int64_t fixture_monotonic_nanos(void) {
    struct timespec time;
    assert(clock_gettime(CLOCK_MONOTONIC, &time) == 0);
    return time.tv_sec * INT64_C(1000000000) + time.tv_nsec;
}
static void fixture_pipe_open(uintptr_t descriptors[2]) {
    int native_descriptors[2];
    assert(pipe(native_descriptors) == 0);
    descriptors[0] = (uintptr_t)native_descriptors[0];
    descriptors[1] = (uintptr_t)native_descriptors[1];
}
static void fixture_pipe_write(uintptr_t descriptor, const void *bytes, size_t length) {
    assert(descriptor <= INT_MAX);
    assert(write((int)descriptor, bytes, length) == (ssize_t)length);
}
static void fixture_pipe_close(uintptr_t descriptor) {
    assert(descriptor <= INT_MAX);
    assert(close((int)descriptor) == 0);
}
#endif

static int entered, proceed, attempted;
static int intercept_wait = 1;
static _Atomic size_t live_allocations;
#ifdef _WIN32
static BOOL CheckedSleepConditionVariableSRW(PCONDITION_VARIABLE condition, PSRWLOCK lock, DWORD timeout, ULONG flags) {
    if (!intercept_wait) return SleepConditionVariableSRW(condition, lock, timeout, flags);
    fixture_lock(&gate);
    entered = 1;
    fixture_signal(&gate_changed);
    while (!proceed) fixture_wait(&gate_changed, &gate);
    fixture_unlock(&gate);
    return SleepConditionVariableSRW(condition, lock, timeout, flags);
}
#else
static int CheckedCondWait(pthread_cond_t *condition, pthread_mutex_t *mutex) {
    if (!intercept_wait) return pthread_cond_wait(condition, mutex);
    fixture_lock(&gate);
    entered = 1;
    fixture_signal(&gate_changed);
    while (!proceed) fixture_wait(&gate_changed, &gate);
    fixture_unlock(&gate);
    return pthread_cond_wait(condition, mutex);
}
#endif

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
int64_t beskid_rt_v5_intrinsic_clock_monotonic_nanos(void) { return fixture_monotonic_nanos(); }
_Noreturn void beskid_rt_v5_trap(uint8_t code, void *message, size_t length) {
    (void)code; (void)message; (void)length; abort();
}
#ifdef _WIN32
#define SleepConditionVariableSRW CheckedSleepConditionVariableSRW
#else
#define pthread_cond_wait CheckedCondWait
#endif
#include "../../assembly/common/external_wait.h"
#ifdef _WIN32
#undef SleepConditionVariableSRW
#else
#undef pthread_cond_wait
#endif

static uint64_t owner;
static void waiter_body(void *unused) {
    (void)unused;
    beskid_rt_v5_intrinsic_owner_wait(owner, -1);
}
static void poster_body(void *unused) {
    (void)unused;
    fixture_lock(&gate);
    attempted = 1;
    fixture_signal(&gate_changed);
    fixture_unlock(&gate);
    assert(beskid_rt_v5_intrinsic_owner_post(owner, 123, 1));
}
#ifdef _WIN32
static unsigned __stdcall waiter(void *unused) { waiter_body(unused); return 0; }
static unsigned __stdcall poster(void *unused) { poster_body(unused); return 0; }
#else
static void *waiter(void *unused) { waiter_body(unused); return NULL; }
static void *poster(void *unused) { poster_body(unused); return NULL; }
#endif
static void await_entry(void) {
    fixture_lock(&gate);
    while (!entered) fixture_wait(&gate_changed, &gate);
    fixture_unlock(&gate);
}
static void release_entry(void) {
    fixture_lock(&gate);
    proceed = 1;
    fixture_signal(&gate_changed);
    fixture_unlock(&gate);
}
static void consume(void) {
    struct BeskidCommand command;
    assert(beskid_rt_v5_intrinsic_owner_pop(owner, &command));
    assert(command.wait == 123 && command.source == 1);
    assert(!beskid_rt_v5_intrinsic_owner_pop(owner, &command));
}
static struct BeskidWorkerRequest *request_for(uintptr_t native_handle, uint64_t id, uint64_t token) {
    struct BeskidWorkerRequest *request = beskid_rt_v5_intrinsic_system_allocate(sizeof(*request), 8);
    *request = (struct BeskidWorkerRequest){0};
    request->operation = BESKID_WORKER_READ;
    request->native_handle = native_handle;
    request->buffer = beskid_rt_v5_intrinsic_system_allocate(1, 8);
    request->length = 1;
    request->owner = id;
    request->tag = token;
    return request;
}
static void await_running(struct BeskidWorkerRequest *request) {
    for (;;) {
        BeskidLock(); int running = request->state == BESKID_WORKER_RUNNING; BeskidUnlock();
        if (running) return;
        fixture_yield();
    }
}
int main(void) {
    owner = beskid_rt_v5_intrinsic_owner_create();
    uint64_t other = beskid_rt_v5_intrinsic_owner_create();
    assert(owner && other && owner != other);
    assert(beskid_rt_v5_intrinsic_owner_post(other, 456, 2));
    struct BeskidCommand command;
    assert(!beskid_rt_v5_intrinsic_owner_pop(owner, &command));
    assert(beskid_rt_v5_intrinsic_owner_pop(other, &command));
    assert(command.wait == 456 && command.source == 2);
    for (size_t repeat = 0; repeat < 64; ++repeat) {
        /* Before: queued signal is durable even if nobody was sleeping. */
        assert(beskid_rt_v5_intrinsic_owner_post(owner, 123, 1));
        beskid_rt_v5_intrinsic_owner_wait(owner, -1);
        consume();
        /* During: pause after empty check while the transport lock is held.
         * Publication attempts then contend with the atomic release-and-sleep. */
        entered = proceed = attempted = 0;
        FixtureThread waiting, posting;
        fixture_thread_start(&waiting, waiter, NULL);
        await_entry();
        fixture_thread_start(&posting, poster, NULL);
        fixture_lock(&gate);
        while (!attempted) fixture_wait(&gate_changed, &gate);
        fixture_unlock(&gate);
        release_entry();
        fixture_thread_join(posting);
        fixture_thread_join(waiting);
        consume();
        /* After: acquiring the same transport lock proves the waiter has
         * released it into the OS condition wait before we publish. */
        entered = proceed = 0;
        fixture_thread_start(&waiting, waiter, NULL);
        await_entry(); release_entry();
        BeskidLock();
        assert(BeskidOwnerPostLocked(owner, 123, 1));
        BeskidUnlock();
        fixture_thread_join(waiting);
        consume();
    }
    intercept_wait = 0;
    assert(beskid_rt_v5_intrinsic_worker_pool_init(2) == 0);
    uintptr_t descriptors[2];
    fixture_pipe_open(descriptors);
    struct BeskidWorkerRequest *canceled = request_for(descriptors[0], owner, 123);
    assert(beskid_rt_v5_intrinsic_worker_submit(canceled) == 0);
    await_running(canceled); /* deterministic in-flight OS read, not merely queued */
    size_t retained = atomic_load(&live_allocations);
    beskid_rt_v5_intrinsic_worker_release(canceled);
    assert(atomic_load(&live_allocations) == retained); /* native memory is still worker-owned */
    beskid_rt_v5_intrinsic_owner_destroy(owner);
    uint64_t replacement = beskid_rt_v5_intrinsic_owner_create();
    assert(replacement != owner && replacement != other);
    assert(!beskid_rt_v5_intrinsic_owner_post(owner, 123, 1));
    assert(!beskid_rt_v5_intrinsic_owner_pop(replacement, &command));
    fixture_pipe_write(descriptors[1], "x", 1);
    for (;;) {
        BeskidLock(); size_t pending = beskid_request_count; BeskidUnlock();
        if (!pending) break;
        fixture_yield();
    }
    assert(atomic_load(&live_allocations) == retained - 2);
    assert(!beskid_rt_v5_intrinsic_owner_pop(replacement, &command));
    struct BeskidWorkerRequest *current = request_for(descriptors[0], replacement, 456);
    assert(beskid_rt_v5_intrinsic_worker_submit(current) == 0);
    await_running(current);
    fixture_pipe_write(descriptors[1], "y", 1);
    beskid_rt_v5_intrinsic_owner_wait(replacement, -1);
    assert(beskid_rt_v5_intrinsic_owner_pop(replacement, &command));
    assert(command.wait == 456 && command.source == 1);
    assert(beskid_rt_v5_intrinsic_worker_poll(current) && current->result == 1 && current->buffer[0] == 'y');
    beskid_rt_v5_intrinsic_worker_release(current);
    beskid_rt_v5_intrinsic_worker_pool_shutdown();
    fixture_pipe_close(descriptors[0]); fixture_pipe_close(descriptors[1]);
    beskid_rt_v5_intrinsic_owner_destroy(replacement);
    beskid_rt_v5_intrinsic_owner_destroy(other);
    assert(atomic_load(&live_allocations) == 0);
    puts("owner_transport matrix=64 stale-owner=rejected in-flight=worker-owned shutdown=complete");
    return 0;
}
