#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>

#ifdef _WIN32
#include <windows.h>
#include <fcntl.h>
#include <io.h>
#include <process.h>

typedef SRWLOCK FixtureMutex;
typedef CONDITION_VARIABLE FixtureCondition;
typedef HANDLE FixtureThread;
#define FIXTURE_MUTEX_INIT SRWLOCK_INIT
#define FIXTURE_CONDITION_INIT CONDITION_VARIABLE_INIT
#define FIXTURE_THREAD_ENTRY unsigned __stdcall
#define FIXTURE_THREAD_RETURN(value) return (unsigned)(uintptr_t)(value)

static void fixture_lock(FixtureMutex *lock) {
    AcquireSRWLockExclusive(lock);
}

static void fixture_unlock(FixtureMutex *lock) {
    ReleaseSRWLockExclusive(lock);
}

static void fixture_wait(FixtureCondition *condition, FixtureMutex *lock) {
    assert(SleepConditionVariableSRW(condition, lock, INFINITE, 0));
}

static void fixture_signal(FixtureCondition *condition) {
    WakeConditionVariable(condition);
}

static void fixture_thread_start(
    FixtureThread *thread, unsigned (__stdcall *entry)(void *), void *argument) {
    uintptr_t handle = _beginthreadex(NULL, 0, entry, argument, 0, NULL);
    assert(handle != 0);
    *thread = (HANDLE)handle;
}

static void fixture_thread_join(FixtureThread thread) {
    assert(WaitForSingleObject(thread, INFINITE) == WAIT_OBJECT_0);
    assert(CloseHandle(thread));
}

static void fixture_pipe_open(int32_t descriptors[2]) {
    assert(_pipe(descriptors, 4096, _O_BINARY) == 0);
    assert(descriptors[0] >= 0 && descriptors[1] >= 0);
}

static void fixture_pipe_write(int32_t descriptor, const void *bytes, size_t count) {
    assert(count <= INT_MAX);
    assert(_write(descriptor, bytes, (unsigned int)count) == (int)count);
}

static void fixture_pipe_close(int32_t descriptor) {
    assert(_close(descriptor) == 0);
}

static void fixture_descriptor_rebind(int32_t source, int32_t destination) {
    assert(_dup2(source, destination) == 0);
}
#else
#include <unistd.h>
#include <pthread.h>

typedef pthread_mutex_t FixtureMutex;
typedef pthread_cond_t FixtureCondition;
typedef pthread_t FixtureThread;
#define FIXTURE_MUTEX_INIT PTHREAD_MUTEX_INITIALIZER
#define FIXTURE_CONDITION_INIT PTHREAD_COND_INITIALIZER
#define FIXTURE_THREAD_ENTRY void *
#define FIXTURE_THREAD_RETURN(value) return (value)

static void fixture_lock(FixtureMutex *lock) {
    assert(pthread_mutex_lock(lock) == 0);
}

static void fixture_unlock(FixtureMutex *lock) {
    assert(pthread_mutex_unlock(lock) == 0);
}

static void fixture_wait(FixtureCondition *condition, FixtureMutex *lock) {
    assert(pthread_cond_wait(condition, lock) == 0);
}

static void fixture_signal(FixtureCondition *condition) {
    assert(pthread_cond_signal(condition) == 0);
}

static void fixture_thread_start(
    FixtureThread *thread, void *(*entry)(void *), void *argument) {
    assert(pthread_create(thread, NULL, entry, argument) == 0);
}

static void fixture_thread_join(FixtureThread thread) {
    assert(pthread_join(thread, NULL) == 0);
}

static void fixture_pipe_open(int32_t descriptors[2]) {
    assert(pipe(descriptors) == 0);
}

static void fixture_pipe_write(int32_t descriptor, const void *bytes, size_t count) {
    assert(write(descriptor, bytes, count) == (ssize_t)count);
}

static void fixture_pipe_close(int32_t descriptor) {
    assert(close(descriptor) == 0);
}

static void fixture_descriptor_rebind(int32_t source, int32_t destination) {
    assert(dup2(source, destination) == destination);
}
#endif

static void *value;
typedef uint8_t (*TryComplete)(uintptr_t, uintptr_t, uintptr_t);
static TryComplete complete_wait;
static uintptr_t owner, token, old_token;
static uintptr_t first_source, second_source;
static int phase;
static int completions;
static int64_t old_fiber;
static FixtureMutex barrier_lock = FIXTURE_MUTEX_INIT;
static FixtureCondition barrier_ready = FIXTURE_CONDITION_INIT;
static int publish;

static void join_success(int64_t fiber) {
    _Alignas(8) unsigned char result[BESKID_ABI_VALUE_SIZE] = {0};
    int status = fiber_join_status(fiber);
    if (status) fprintf(stderr, "join handle=%lld status=%d phase=%d completions=%d\n", (long long)fiber, status, phase, completions);
    assert(status == 0);
    assert(fiber_join_value(fiber, result));
    assert(beskid_rt_v5_abi_value_clear(result));
}

static void post_pair(void) {
    assert(beskid_rt_v5_external_wait_post(owner, token, first_source));
    assert(beskid_rt_v5_external_wait_post(owner, token, second_source));
}
static void *post_entry(void *argument) { post_pair(); return argument; }
static FIXTURE_THREAD_ENTRY foreign_post(void *unused) {
    (void)unused;
    fixture_lock(&barrier_lock);
    while (!publish) fixture_wait(&barrier_ready, &barrier_lock);
    fixture_unlock(&barrier_lock);
    post_pair(); /* this thread has no attached runtime/TLS */
    FIXTURE_THREAD_RETURN(NULL);
}
static void *wait_entry(void *argument) {
    int64_t current_fiber = fiber_current_id();
    assert(current_fiber >= 0);
    if (old_fiber) assert(!fiber_cancel(old_fiber, 9));
    token = beskid_rt_v5_external_wait_register(current_fiber, 9, -1);
    assert(token && beskid_rt_v5_external_active_count() == 1);
    if (old_token) {
        assert(token != old_token);
        for (uintptr_t source = 1; source <= 4; ++source)
            assert(!complete_wait(old_token & UINT32_MAX, old_token >> 32, source));
        assert(beskid_rt_v5_external_active_count() == 1);
    }
    if (phase == 0) post_pair(); /* published before park checks */
    if (phase == 2) {
        fixture_lock(&barrier_lock);
        publish = 1;
        fixture_signal(&barrier_ready);
        fixture_unlock(&barrier_lock);
    }
    assert(beskid_rt_v5_external_wait_park(token) == first_source);
    ++completions;
    assert(beskid_rt_v5_external_active_count() == 0);
    assert(!complete_wait(token & UINT32_MAX, token >> 32, second_source));
    assert(beskid_rt_v5_external_wait_release(token));
    old_token = token;
    old_fiber = current_fiber;
    return argument;
}

static void race_matrix(void) {
    /* Each ordered pair, including redelivery of readiness, close, cancel,
     * timeout and duplicate notification. Both orderings are exercised. */
    const uintptr_t sources[] = {1, 2, 3, 4, 1};
    for (int repeat = 0; repeat < 8; ++repeat) {
        for (phase = 0; phase < 3; ++phase) {
            for (size_t a = 0; a < 5; ++a) for (size_t b = 0; b < 5; ++b) {
                first_source = sources[a]; second_source = sources[b];
                publish = 0;
                FixtureThread producer;
                if (phase == 2) fixture_thread_start(&producer, foreign_post, NULL);
                int64_t fiber = fiber_spawn((void *)wait_entry, value);
                int64_t controller = phase == 1 ? fiber_spawn((void *)post_entry, value) : -1;
                join_success(fiber);
                if (controller >= 0) join_success(controller);
                if (phase == 2) fixture_thread_join(producer);
                /* Flush the losing command before reusing the slot. */
                beskid_rt_v5_external_pump(clock_monotonic_nanos());
            }
        }
    }
    assert(completions == 600);
    fprintf(stderr, "owner=%llu generation=%llu winner=%llu active=%llu matrix=600\n",
            (unsigned long long)owner, (unsigned long long)(old_token >> 32),
            (unsigned long long)first_source, (unsigned long long)beskid_rt_v5_external_active_count());
}

enum {
    BESKID_TEST_SCHEDULER_FIBER_CAPACITY =
        (BESKID_SCHEDULER_STATE_SIZE - BESKID_SCHEDULER_STATE_FIBERS_OFFSET) /
        BESKID_FIBER_RECORD_SIZE,
    CAPACITY_WAITERS = BESKID_TEST_SCHEDULER_FIBER_CAPACITY - 1,
};
_Static_assert(BESKID_TEST_SCHEDULER_FIBER_CAPACITY >= 2,
    "scheduler requires controller plus waiter");

static uintptr_t capacity_tokens[CAPACITY_WAITERS];
static int64_t capacity_handles[CAPACITY_WAITERS];
static size_t capacity_registered, capacity_released;
static int64_t capacity_controller_handle;
static uintptr_t capacity_old_token, capacity_current_token;
static size_t capacity_active_waiters, capacity_active_full, capacity_active_duplicate;
static size_t capacity_active_timeout, capacity_active_reused, capacity_active_current_release;

static void *capacity_waiter_entry(void *argument) {
    size_t index = capacity_registered;
    int64_t handle = fiber_current_id();
    uintptr_t current = beskid_rt_v5_external_wait_register(handle, 9, -1);
    assert(handle >= 0 && current != 0);
    capacity_handles[index] = handle;
    capacity_tokens[index] = current;
    ++capacity_registered;
    assert(beskid_rt_v5_external_wait_park(current) == 1);
    assert(beskid_rt_v5_external_wait_release(current));
    ++capacity_released;
    return argument;
}

static void *capacity_controller_entry(void *argument) {
    while (capacity_registered != CAPACITY_WAITERS) beskid_rt_v5_fiber_yield();
    capacity_active_waiters = beskid_rt_v5_external_active_count();
    assert(capacity_active_waiters == CAPACITY_WAITERS);

    int64_t handle = fiber_current_id();
    int64_t capacity_deadline = clock_monotonic_nanos() + 1000000000;
    capacity_old_token = beskid_rt_v5_external_wait_register(handle, 9, capacity_deadline);
    capacity_active_full = beskid_rt_v5_external_active_count();
    assert(capacity_old_token && capacity_active_full == BESKID_TEST_SCHEDULER_FIBER_CAPACITY);
    assert(!beskid_rt_v5_external_wait_register(handle, 9, -1));
    capacity_active_duplicate = beskid_rt_v5_external_active_count();
    assert(capacity_active_duplicate == BESKID_TEST_SCHEDULER_FIBER_CAPACITY);

    beskid_rt_v5_external_pump(capacity_deadline);
    assert(beskid_rt_v5_external_wait_park(capacity_old_token) == 4);
    capacity_active_timeout = beskid_rt_v5_external_active_count();
    assert(capacity_active_timeout == CAPACITY_WAITERS);
    assert(beskid_rt_v5_external_wait_release(capacity_old_token));

    capacity_current_token = beskid_rt_v5_external_wait_register(handle, 9, -1);
    capacity_active_reused = beskid_rt_v5_external_active_count();
    assert(capacity_current_token && capacity_current_token != capacity_old_token);
    assert(capacity_active_reused == BESKID_TEST_SCHEDULER_FIBER_CAPACITY);
    assert(beskid_rt_v5_external_wait_post(owner, capacity_old_token, 3));
    assert(beskid_rt_v5_external_wait_post(owner, capacity_current_token, 1));
    beskid_rt_v5_external_pump(-1);
    assert(beskid_rt_v5_external_wait_park(capacity_current_token) == 1);
    assert(beskid_rt_v5_external_wait_release(capacity_current_token));
    capacity_active_current_release = beskid_rt_v5_external_active_count();
    assert(capacity_active_current_release == CAPACITY_WAITERS);

    for (size_t i = 0; i < CAPACITY_WAITERS; ++i)
        assert(beskid_rt_v5_external_wait_post(owner, capacity_tokens[i], 1));
    beskid_rt_v5_external_pump(-1);
    while (capacity_released != CAPACITY_WAITERS) beskid_rt_v5_fiber_yield();
    assert(beskid_rt_v5_external_active_count() == 0);
    fprintf(stderr,
        "capacity=%u registered=%zu active=%zu/%zu/%zu/%zu/%zu/%zu old_generation=%llu current_generation=%llu final=%zu\n",
        (unsigned)BESKID_TEST_SCHEDULER_FIBER_CAPACITY, capacity_registered + 1,
        capacity_active_waiters, capacity_active_full, capacity_active_duplicate,
        capacity_active_timeout, capacity_active_reused, capacity_active_current_release,
        (unsigned long long)(capacity_old_token >> 32),
        (unsigned long long)(capacity_current_token >> 32),
        (size_t)beskid_rt_v5_external_active_count());
    return argument;
}

static void *capacity_replacement_entry(void *argument) {
    assert(!fiber_cancel(capacity_controller_handle, 9));
    for (uintptr_t source = 1; source <= 4; ++source)
        assert(!complete_wait(capacity_old_token & UINT32_MAX, capacity_old_token >> 32, source));
    uintptr_t current = beskid_rt_v5_external_wait_register(fiber_current_id(), 9, -1);
    assert(current);
    assert(beskid_rt_v5_external_wait_post(owner, current, 1));
    beskid_rt_v5_external_pump(-1);
    assert(beskid_rt_v5_external_wait_park(current) == 1);
    assert(beskid_rt_v5_external_wait_release(current));
    return argument;
}

static void full_legal_capacity(void) {
    int64_t waiters[CAPACITY_WAITERS];
    memset(capacity_tokens, 0, sizeof(capacity_tokens));
    memset(capacity_handles, 0, sizeof(capacity_handles));
    capacity_registered = capacity_released = 0;
    for (size_t i = 0; i < CAPACITY_WAITERS; ++i)
        waiters[i] = fiber_spawn((void *)capacity_waiter_entry, value);
    capacity_controller_handle = fiber_spawn((void *)capacity_controller_entry, value);
    join_success(capacity_controller_handle);
    int64_t replacement = fiber_spawn((void *)capacity_replacement_entry, value);
    assert((uint32_t)replacement == (uint32_t)capacity_controller_handle);
    assert(replacement != capacity_controller_handle);
    join_success(replacement);
    for (size_t i = 0; i < CAPACITY_WAITERS; ++i) join_success(waiters[i]);
}

static void *timer_entry(void *argument) {
    int64_t start = clock_monotonic_nanos();
    assert(beskid_rt_v5_external_sleep_until(start + 2000000) == 4);
    assert(clock_monotonic_nanos() >= start + 2000000);
    return argument;
}
static void *timer_reuse_entry(void *argument) {
    uintptr_t handle = fiber_current_id();
    int64_t now = clock_monotonic_nanos();
    uintptr_t expired = beskid_rt_v5_external_wait_register(handle, 3, now + 5000000000);
    assert(complete_wait(expired & UINT32_MAX, expired >> 32, 3));
    assert(beskid_rt_v5_external_wait_park(expired) == 3);
    assert(beskid_rt_v5_external_wait_release(expired));
    uintptr_t current = beskid_rt_v5_external_wait_register(handle, 3, now + 10000000000);
    assert(current != expired);
    assert(beskid_rt_v5_external_wait_post(owner, expired, 4));
    beskid_rt_v5_external_pump(now + 5000000000);
    assert(beskid_rt_v5_external_active_count() == 1);
    beskid_rt_v5_external_pump(now + 10000000000);
    assert(beskid_rt_v5_external_wait_park(current) == 4);
    assert(beskid_rt_v5_external_active_count() == 0);
    assert(beskid_rt_v5_external_wait_release(current));
    return argument;
}

static void *deadlock_entry(void *argument) {
    (void)argument;
    int64_t channel = channel_create(1, 0);
    channel_receive_status(channel);
    assert(!"an unowned parked wait must report deadlock");
    return NULL;
}

static int32_t descriptors[2];
static int32_t retry_descriptors[2];
static int32_t rebound_descriptors[2];
static int64_t reader;
static int resumed;
static void *read_entry(void *argument) {
    unsigned char byte = 0;
    assert(syscall_read_bytes(descriptors[0], &byte, 1) == -1);
    resumed++;
    assert(beskid_rt_v5_external_active_count() == 0);
    /* Cancellation is a persistent fiber property, not a one-wait message.
     * Repeated Cancel is accepted without publishing another cancel command. */
    assert(fiber_cancel(fiber_current_id(), 10));
    assert(beskid_rt_v5_external_sleep_until(clock_monotonic_nanos() + 30000000000) == 3);
    assert(beskid_rt_v5_external_active_count() == 0);
    assert(fiber_cancel(fiber_current_id(), 11));
    assert(syscall_read_bytes(retry_descriptors[0], &byte, 1) == -1);
    assert(byte == 0 && beskid_rt_v5_external_active_count() == 0);
    assert(beskid_rt_v5_external_sleep_until(clock_monotonic_nanos() + 30000000000) == 3);
    assert(beskid_rt_v5_external_active_count() == 0);
    return argument;
}
static void *cancel_entry(void *argument) {
    assert(fiber_cancel(reader, 9));
    return argument;
}
static void *late_completion_entry(void *argument) {
    /* The cancelled fiber slot and wait slot have both been reused. The old
     * native request may now finish, but cannot publish into the new wait. */
    assert((uint32_t)fiber_current_id() != (uint32_t)reader);
    assert(beskid_rt_v5_external_active_count() == 1);
    fixture_pipe_write(descriptors[1], "x", 1);
    fixture_pipe_write(retry_descriptors[1], "y", 1);
    post_pair();
    return argument;
}

static void *ready_entry(void *argument) {
    unsigned char byte = 0;
    assert(syscall_read_bytes(descriptors[0], &byte, 1) == 1);
    assert(byte == 'r');
    return argument;
}
static FIXTURE_THREAD_ENTRY writer_thread(void *unused) {
    (void)unused;
    fixture_lock(&barrier_lock);
    while (!publish) fixture_wait(&barrier_ready, &barrier_lock);
    fixture_unlock(&barrier_lock);
    fixture_pipe_write(descriptors[1], "r", 1);
    FIXTURE_THREAD_RETURN(NULL);
}
static void *other_runnable_entry(void *argument) {
    assert(beskid_rt_v5_external_active_count() == 1);
    /* The worker has admitted and duplicated descriptors[0]. Replace the
     * caller slot before publishing the original pipe's byte. The read below
     * must consume that original byte, never the rebound pipe. */
    fixture_pipe_open(rebound_descriptors);
    fixture_descriptor_rebind(rebound_descriptors[0], descriptors[0]);
    fixture_pipe_close(rebound_descriptors[0]);
    fixture_lock(&barrier_lock);
    publish = 1;
    fixture_signal(&barrier_ready);
    fixture_unlock(&barrier_lock);
    return argument;
}

static uintptr_t timer_tokens[12];
static size_t timer_started, timer_finished, timer_order[12];
static int64_t timer_base;
static const int offsets[] = {80, 20, 110, 40, 100, 50, 70, 10, 90, 30, 60, 120};
static void *heap_timer_entry(void *argument) {
    size_t index = timer_started++;
    uintptr_t current = beskid_rt_v5_external_wait_register(fiber_current_id(), 3, timer_base + offsets[index]);
    assert(current);
    timer_tokens[index] = current;
    uintptr_t expected = index == 0 || index == 5 || index == 7 ? 3 : 4;
    assert(beskid_rt_v5_external_wait_park(current) == expected);
    timer_order[timer_finished++] = index;
    assert(beskid_rt_v5_external_wait_release(current));
    return argument;
}
static void *heap_controller(void *argument) {
    assert(timer_started == 12 && beskid_rt_v5_external_active_count() == 12);
    const size_t canceled[] = {0, 5, 7};
    for (size_t i = 0; i < 3; ++i) {
        uintptr_t current = timer_tokens[canceled[i]];
        assert(complete_wait(current & UINT32_MAX, current >> 32, 3));
    }
    beskid_rt_v5_external_pump(timer_base + 19);
    assert(beskid_rt_v5_external_active_count() == 9);
    beskid_rt_v5_external_pump(timer_base + 20);
    assert(beskid_rt_v5_external_active_count() == 8);
    beskid_rt_v5_external_pump(timer_base + 120);
    assert(beskid_rt_v5_external_active_count() == 0);
    return argument;
}
static void timer_heap(void) {
    timer_base = clock_monotonic_nanos() + 10000000000;
    int64_t fibers[12];
    for (size_t i = 0; i < 12; ++i) fibers[i] = fiber_spawn((void *)heap_timer_entry, value);
    int64_t controller = fiber_spawn((void *)heap_controller, value);
    for (size_t i = 0; i < 12; ++i) join_success(fibers[i]);
    join_success(controller);
    const size_t expected[] = {0, 5, 7, 1, 9, 3, 10, 6, 8, 4, 2, 11};
    assert(timer_finished == 12 && memcmp(timer_order, expected, sizeof(expected)) == 0);
}

static int64_t timer_winner_handle;
static uintptr_t timer_winner_token, timer_winner_source;
static int64_t timer_winner_deadline;
static int timer_winner_registered, timer_winner_released;

static void *timer_winner_waiter_entry(void *argument) {
    timer_winner_handle = fiber_current_id();
    timer_winner_token = beskid_rt_v5_external_wait_register(
        timer_winner_handle, 9, timer_winner_deadline);
    assert(timer_winner_handle >= 0 && timer_winner_token);
    timer_winner_registered = 1;
    timer_winner_source = beskid_rt_v5_external_wait_park(timer_winner_token);
    assert(timer_winner_source == 4);
    assert(beskid_rt_v5_external_wait_release(timer_winner_token));
    timer_winner_released = 1;
    return argument;
}

static void *timer_winner_controller_entry(void *argument) {
    while (!timer_winner_registered) beskid_rt_v5_fiber_yield();
    beskid_rt_v5_external_pump(timer_winner_deadline);
    assert(beskid_rt_v5_external_active_count() == 0);
    assert(fiber_cancel(timer_winner_handle, 9));
    while (!timer_winner_released) beskid_rt_v5_fiber_yield();
    fprintf(stderr, "case=timer-winner owner=%llu generation=%llu winner=%llu active=%zu\n",
        (unsigned long long)owner, (unsigned long long)(timer_winner_token >> 32),
        (unsigned long long)timer_winner_source,
        (size_t)beskid_rt_v5_external_active_count());
    return argument;
}

static void timer_winner_before_resume(void) {
    timer_winner_handle = -1;
    timer_winner_token = timer_winner_source = 0;
    timer_winner_registered = timer_winner_released = 0;
    timer_winner_deadline = clock_monotonic_nanos() + 1000000000;
    int64_t peer = fiber_spawn((void *)timer_winner_waiter_entry, value);
    int64_t controller = fiber_spawn((void *)timer_winner_controller_entry, value);
    join_success(controller);
    assert(timer_winner_released && timer_winner_source == 4);
    assert(fiber_join_status(peer) == 1);
    assert(fiber_join_error_finish(peer));
}

static uintptr_t command_token, command_source;
static int64_t command_deadline;
static int command_registered, command_released;
static uintptr_t equal_tokens[2], equal_sources[2];
static int64_t equal_deadline;
static size_t equal_registered, equal_released, equal_completions[2];

static void *command_waiter_entry(void *argument) {
    int64_t handle = fiber_current_id();
    command_token = beskid_rt_v5_external_wait_register(handle, 9, command_deadline);
    assert(handle >= 0 && command_token);
    command_registered = 1;
    command_source = beskid_rt_v5_external_wait_park(command_token);
    assert(command_source == 1);
    assert(beskid_rt_v5_external_wait_release(command_token));
    command_released = 1;
    return argument;
}

static void *equal_deadline_waiter_entry(void *argument) {
    size_t index = equal_registered++;
    uintptr_t current = beskid_rt_v5_external_wait_register(
        fiber_current_id(), 9, equal_deadline);
    assert(index < 2 && current);
    equal_tokens[index] = current;
    equal_sources[index] = beskid_rt_v5_external_wait_park(current);
    assert(equal_sources[index] == 4);
    ++equal_completions[index];
    assert(beskid_rt_v5_external_wait_release(current));
    ++equal_released;
    return argument;
}

static void *pump_order_controller_entry(void *argument) {
    while (!command_registered) beskid_rt_v5_fiber_yield();
    assert(beskid_rt_v5_external_wait_post(owner, command_token, 1));
    beskid_rt_v5_external_pump(command_deadline);
    assert(beskid_rt_v5_external_active_count() == 0);
    while (!command_released) beskid_rt_v5_fiber_yield();
    assert(command_source == 1);
    fprintf(stderr, "case=pump-command owner=%llu generation=%llu winner=%llu active=%zu\n",
        (unsigned long long)owner, (unsigned long long)(command_token >> 32),
        (unsigned long long)command_source,
        (size_t)beskid_rt_v5_external_active_count());

    return argument;
}

static void *equal_deadline_controller_entry(void *argument) {
    while (equal_registered != 2) beskid_rt_v5_fiber_yield();
    beskid_rt_v5_external_pump(equal_deadline);
    assert(beskid_rt_v5_external_active_count() == 0);
    while (equal_released != 2) beskid_rt_v5_fiber_yield();
    assert(equal_tokens[0] != equal_tokens[1]);
    assert(equal_sources[0] == 4 && equal_sources[1] == 4);
    assert(equal_completions[0] == 1 && equal_completions[1] == 1);
    fprintf(stderr, "case=pump-equal owner=%llu generation=%llu/%llu winner=%llu/%llu active=%zu\n",
        (unsigned long long)owner, (unsigned long long)(equal_tokens[0] >> 32),
        (unsigned long long)(equal_tokens[1] >> 32),
        (unsigned long long)equal_sources[0], (unsigned long long)equal_sources[1],
        (size_t)beskid_rt_v5_external_active_count());
    return argument;
}

static void pump_order_cases(void) {
    command_token = command_source = 0;
    command_registered = command_released = 0;
    command_deadline = clock_monotonic_nanos() + 1000000000;
    int64_t command_peer = fiber_spawn((void *)command_waiter_entry, value);
    int64_t command_controller = fiber_spawn((void *)pump_order_controller_entry, value);
    join_success(command_peer);
    join_success(command_controller);

    equal_deadline = clock_monotonic_nanos() + 1000000000;
    memset(equal_tokens, 0, sizeof(equal_tokens));
    memset(equal_sources, 0, sizeof(equal_sources));
    memset(equal_completions, 0, sizeof(equal_completions));
    equal_registered = equal_released = 0;
    int64_t first_equal_peer = fiber_spawn((void *)equal_deadline_waiter_entry, value);
    int64_t second_equal_peer = fiber_spawn((void *)equal_deadline_waiter_entry, value);
    int64_t equal_controller = fiber_spawn((void *)equal_deadline_controller_entry, value);
    join_success(first_equal_peer);
    join_success(second_equal_peer);
    join_success(equal_controller);
}

static int detached_started;
static void *detached_timer_entry(void *argument) {
    (void)argument;
    detached_started = 1;
    beskid_rt_v5_external_sleep_until(clock_monotonic_nanos() + 30000000000);
    assert(!"shutdown must not resume detached timer work");
    return NULL;
}
static void *sentinel_entry(void *argument) { return argument; }
#ifdef _WIN32
#define EXTERNAL_WAIT_EXPORT __declspec(dllexport)
#else
#define EXTERNAL_WAIT_EXPORT
#endif

EXTERNAL_WAIT_EXPORT int RunExternalWaitFixture(TryComplete complete, int deadlock) {
    complete_wait = complete;
    _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
    assert(beskid_rt_v5_process_init(runtime) == runtime);
    uintptr_t descriptor[] = {24, 8, 0, 0, 0};
    uintptr_t allocation[] = {24, 8, (uintptr_t)descriptor};
    value = beskid_rt_v5_managed_object_allocate(allocation);
    assert(value && gc_register_root(&value));
    owner = beskid_rt_v5_external_owner_id();
    assert(owner);
    if (deadlock) {
        fiber_join_status(fiber_spawn((void *)deadlock_entry, value));
        return 0;
    }
    full_legal_capacity();
    race_matrix();
    timer_winner_before_resume();
    pump_order_cases();
    join_success(fiber_spawn((void *)timer_reuse_entry, value));
    join_success(fiber_spawn((void *)timer_entry, value));
    timer_heap();
    fixture_pipe_open(descriptors);
    publish = 0;
    FixtureThread writer;
    fixture_thread_start(&writer, writer_thread, NULL);
    int64_t ready_reader = fiber_spawn((void *)ready_entry, value);
    int64_t other = fiber_spawn((void *)other_runnable_entry, value);
    join_success(ready_reader); join_success(other);
    fixture_thread_join(writer);
    fprintf(stderr, "descriptor-close-rebind=preserved\n");
    fixture_pipe_close(descriptors[0]); fixture_pipe_close(descriptors[1]);
    fixture_pipe_close(rebound_descriptors[1]);
    fixture_pipe_open(descriptors);
    reader = fiber_spawn((void *)read_entry, value);
    fixture_pipe_open(retry_descriptors);
    int64_t canceler = fiber_spawn((void *)cancel_entry, value);
    assert(fiber_join_status(reader) == 1);
    assert(resumed == 1); /* cancellation must return through the owner operation */
    assert(fiber_join_error_finish(reader));
    join_success(canceler);
    phase = 1; first_source = 2; second_source = 1;
    int64_t replacement = fiber_spawn((void *)wait_entry, value);
    assert((uint32_t)replacement == (uint32_t)reader && replacement != reader);
    int64_t late = fiber_spawn((void *)late_completion_entry, value);
    join_success(replacement); join_success(late);
    assert(beskid_rt_v5_external_active_count() == 0 && resumed == 1);
    int64_t detached = fiber_spawn((void *)detached_timer_entry, value);
    fiber_detach(detached);
    join_success(fiber_spawn((void *)sentinel_entry, value));
    assert(detached_started && beskid_rt_v5_external_active_count() == 1);
    gc_unregister_root(&value);
    fixture_pipe_close(descriptors[0]); fixture_pipe_close(descriptors[1]);
    fixture_pipe_close(retry_descriptors[0]); fixture_pipe_close(retry_descriptors[1]);
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}
#ifdef EXTERNAL_WAIT_STANDALONE
int main(int argc, char **argv) {
    return RunExternalWaitFixture(beskid_rt_v5_external_try_complete,
        argc == 2 && strcmp(argv[1], "deadlock") == 0);
}
#endif
