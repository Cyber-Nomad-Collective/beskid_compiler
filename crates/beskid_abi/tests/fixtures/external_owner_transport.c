#define _DARWIN_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <errno.h>
#include <limits.h>
#include <pthread.h>
#include <stdint.h>
#include <stdatomic.h>
#include <sched.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>

/* Test-only rendezvous at the actual OS wait boundary. The production mailbox,
 * lock, condition variable, and queue are included unchanged below. */
static pthread_mutex_t gate = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t gate_changed = PTHREAD_COND_INITIALIZER;
static int entered, proceed, attempted;
static int intercept_wait = 1;
static _Atomic size_t live_allocations;
static int CheckedCondWait(pthread_cond_t *condition, pthread_mutex_t *mutex) {
    if (!intercept_wait) return pthread_cond_wait(condition, mutex);
    pthread_mutex_lock(&gate);
    entered = 1;
    pthread_cond_broadcast(&gate_changed);
    while (!proceed) pthread_cond_wait(&gate_changed, &gate);
    pthread_mutex_unlock(&gate);
    return pthread_cond_wait(condition, mutex);
}
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
    struct timespec time;
    assert(clock_gettime(CLOCK_MONOTONIC, &time) == 0);
    return time.tv_sec * INT64_C(1000000000) + time.tv_nsec;
}
_Noreturn void beskid_rt_v5_trap(uint8_t code, void *message, size_t length) {
    (void)code; (void)message; (void)length; abort();
}
#define pthread_cond_wait CheckedCondWait
#include "../../assembly/common/external_wait.h"
#undef pthread_cond_wait

static uint64_t owner;
static void *waiter(void *unused) {
    (void)unused;
    beskid_rt_v5_intrinsic_owner_wait(owner, -1);
    return NULL;
}
static void *poster(void *unused) {
    (void)unused;
    pthread_mutex_lock(&gate);
    attempted = 1;
    pthread_cond_broadcast(&gate_changed);
    pthread_mutex_unlock(&gate);
    assert(beskid_rt_v5_intrinsic_owner_post(owner, 123, 1));
    return NULL;
}
static void await_entry(void) {
    pthread_mutex_lock(&gate);
    while (!entered) pthread_cond_wait(&gate_changed, &gate);
    pthread_mutex_unlock(&gate);
}
static void release_entry(void) {
    pthread_mutex_lock(&gate);
    proceed = 1;
    pthread_cond_broadcast(&gate_changed);
    pthread_mutex_unlock(&gate);
}
static void consume(void) {
    struct BeskidCommand command;
    assert(beskid_rt_v5_intrinsic_owner_pop(owner, &command));
    assert(command.wait == 123 && command.source == 1);
    assert(!beskid_rt_v5_intrinsic_owner_pop(owner, &command));
}
static struct BeskidWorkerRequest *request_for(int fd, uint64_t id, uint64_t token) {
    struct BeskidWorkerRequest *request = beskid_rt_v5_intrinsic_system_allocate(sizeof(*request), 8);
    *request = (struct BeskidWorkerRequest){0};
    request->operation = BESKID_WORKER_READ;
    request->native_handle = fd;
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
        sched_yield();
    }
}
int main(void) {
    alarm(10);
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
        pthread_t waiting, posting;
        assert(pthread_create(&waiting, NULL, waiter, NULL) == 0);
        await_entry();
        assert(pthread_create(&posting, NULL, poster, NULL) == 0);
        pthread_mutex_lock(&gate);
        while (!attempted) pthread_cond_wait(&gate_changed, &gate);
        pthread_mutex_unlock(&gate);
        release_entry();
        assert(pthread_join(posting, NULL) == 0 && pthread_join(waiting, NULL) == 0);
        consume();
        /* After: acquiring the same transport lock proves the waiter has
         * released it into the OS condition wait before we publish. */
        entered = proceed = 0;
        assert(pthread_create(&waiting, NULL, waiter, NULL) == 0);
        await_entry(); release_entry();
        BeskidLock();
        assert(BeskidOwnerPostLocked(owner, 123, 1));
        BeskidUnlock();
        assert(pthread_join(waiting, NULL) == 0);
        consume();
    }
    intercept_wait = 0;
    assert(beskid_rt_v5_intrinsic_worker_pool_init(2) == 0);
    int descriptors[2];
    assert(pipe(descriptors) == 0);
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
    assert(write(descriptors[1], "x", 1) == 1);
    for (;;) {
        BeskidLock(); size_t pending = beskid_request_count; BeskidUnlock();
        if (!pending) break;
        sched_yield();
    }
    assert(atomic_load(&live_allocations) == retained - 2);
    assert(!beskid_rt_v5_intrinsic_owner_pop(replacement, &command));
    struct BeskidWorkerRequest *current = request_for(descriptors[0], replacement, 456);
    assert(beskid_rt_v5_intrinsic_worker_submit(current) == 0);
    await_running(current);
    assert(write(descriptors[1], "y", 1) == 1);
    beskid_rt_v5_intrinsic_owner_wait(replacement, -1);
    assert(beskid_rt_v5_intrinsic_owner_pop(replacement, &command));
    assert(command.wait == 456 && command.source == 1);
    assert(beskid_rt_v5_intrinsic_worker_poll(current) && current->result == 1 && current->buffer[0] == 'y');
    beskid_rt_v5_intrinsic_worker_release(current);
    beskid_rt_v5_intrinsic_worker_pool_shutdown();
    close(descriptors[0]); close(descriptors[1]);
    beskid_rt_v5_intrinsic_owner_destroy(replacement);
    beskid_rt_v5_intrinsic_owner_destroy(other);
    assert(atomic_load(&live_allocations) == 0);
    alarm(0);
    return 0;
}
