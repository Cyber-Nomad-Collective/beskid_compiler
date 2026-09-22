/* Native transport only. The Beskid owner decides every terminal wait outcome.
 * One lock protects mailbox publication and worker request lifetime. No foreign
 * thread touches the managed heap, a fiber record, or a scheduler run queue. */

#ifndef _WIN32
#include <fcntl.h>
#endif

extern void *beskid_rt_v5_intrinsic_system_allocate(size_t, size_t);
extern void beskid_rt_v5_intrinsic_system_free(void *, size_t);
extern int64_t beskid_rt_v5_intrinsic_clock_monotonic_nanos(void);
void beskid_rt_v5_intrinsic_worker_pool_shutdown(void);

#ifdef _WIN32
typedef CONDITION_VARIABLE BeskidCondition;
static SRWLOCK beskid_transport_lock = SRWLOCK_INIT;
static BeskidCondition beskid_work_ready = CONDITION_VARIABLE_INIT;
static void BeskidLock(void) { AcquireSRWLockExclusive(&beskid_transport_lock); }
static void BeskidUnlock(void) { ReleaseSRWLockExclusive(&beskid_transport_lock); }
static int BeskidConditionInit(BeskidCondition *c) { InitializeConditionVariable(c); return 0; }
static void BeskidConditionDestroy(BeskidCondition *c) { (void)c; }
static void BeskidConditionSignal(BeskidCondition *c) { WakeAllConditionVariable(c); }
static void BeskidConditionWait(BeskidCondition *c, int64_t deadline) {
  DWORD delay = INFINITE;
  if (deadline >= 0) {
    int64_t remaining = deadline - beskid_rt_v5_intrinsic_clock_monotonic_nanos();
    if (remaining <= 0) return;
    uint64_t millis = ((uint64_t)remaining + 999999) / 1000000;
    delay = millis >= INFINITE ? INFINITE - 1 : (DWORD)millis;
  }
  SleepConditionVariableSRW(c, &beskid_transport_lock, delay, 0);
}
#else
typedef pthread_cond_t BeskidCondition;
static pthread_mutex_t beskid_transport_lock = PTHREAD_MUTEX_INITIALIZER;
static BeskidCondition beskid_work_ready = PTHREAD_COND_INITIALIZER;
static void BeskidLock(void) { pthread_mutex_lock(&beskid_transport_lock); }
static void BeskidUnlock(void) { pthread_mutex_unlock(&beskid_transport_lock); }
static int BeskidConditionInit(BeskidCondition *c) {
#ifdef __APPLE__
  return pthread_cond_init(c, NULL);
#else
  pthread_condattr_t attributes;
  if (pthread_condattr_init(&attributes)) return -1;
  int result = pthread_condattr_setclock(&attributes, CLOCK_MONOTONIC);
  if (!result) result = pthread_cond_init(c, &attributes);
  pthread_condattr_destroy(&attributes);
  return result;
#endif
}
static void BeskidConditionDestroy(BeskidCondition *c) { pthread_cond_destroy(c); }
static void BeskidConditionSignal(BeskidCondition *c) { pthread_cond_broadcast(c); }
static void BeskidConditionWait(BeskidCondition *c, int64_t deadline) {
  if (deadline < 0) { pthread_cond_wait(c, &beskid_transport_lock); return; }
#ifdef __APPLE__
  int64_t remaining = deadline - beskid_rt_v5_intrinsic_clock_monotonic_nanos();
  if (remaining <= 0) return;
  struct timespec time = {remaining / 1000000000, remaining % 1000000000};
  pthread_cond_timedwait_relative_np(c, &beskid_transport_lock, &time);
#else
  struct timespec time = {deadline / 1000000000, deadline % 1000000000};
  pthread_cond_timedwait(c, &beskid_transport_lock, &time);
#endif
}
#endif

struct BeskidCommand { uint64_t wait, source; };
struct BeskidOwner {
  struct BeskidOwner *next;
  uint64_t id;
  size_t head, count;
  BeskidCondition wake;
  struct BeskidCommand commands[256];
};
static struct BeskidOwner *beskid_owners;
static uint64_t beskid_next_owner;
static struct BeskidOwner *BeskidOwnerFind(uint64_t id) {
  for (struct BeskidOwner *owner = beskid_owners; owner; owner = owner->next)
    if (owner->id == id) return owner;
  return NULL;
}
uint64_t beskid_rt_v5_intrinsic_owner_create(void) {
  struct BeskidOwner *owner = beskid_rt_v5_intrinsic_system_allocate(sizeof(*owner), 8);
  if (!owner) return 0;
  __builtin_memset(owner, 0, sizeof(*owner));
  if (BeskidConditionInit(&owner->wake)) {
    beskid_rt_v5_intrinsic_system_free(owner, sizeof(*owner)); return 0;
  }
  BeskidLock();
  if (beskid_next_owner == UINT64_MAX) beskid_rt_v5_trap(10, NULL, 0);
  owner->id = ++beskid_next_owner;
  owner->next = beskid_owners;
  beskid_owners = owner;
  BeskidUnlock();
  return owner->id;
}
void beskid_rt_v5_intrinsic_owner_destroy(uint64_t id) {
  BeskidLock();
  struct BeskidOwner **link = &beskid_owners;
  while (*link && (*link)->id != id) link = &(*link)->next;
  struct BeskidOwner *owner = *link;
  if (owner) *link = owner->next;
  BeskidUnlock();
  if (owner) {
    BeskidConditionDestroy(&owner->wake);
    beskid_rt_v5_intrinsic_system_free(owner, sizeof(*owner));
  }
}
static int32_t BeskidOwnerPostLocked(uint64_t id, uint64_t wait, uint64_t source) {
  struct BeskidOwner *owner = BeskidOwnerFind(id);
  if (!owner || !wait || source < 1 || source > 4) return 0;
  if (owner->count == 256) beskid_rt_v5_trap(10, NULL, 0);
  owner->commands[(owner->head + owner->count++) % 256] = (struct BeskidCommand){wait, source};
  BeskidConditionSignal(&owner->wake);
  return 1;
}
int32_t beskid_rt_v5_intrinsic_owner_post(uint64_t id, uint64_t wait, uint64_t source) {
  BeskidLock();
  int32_t result = BeskidOwnerPostLocked(id, wait, source);
  BeskidUnlock();
  return result;
}
int32_t beskid_rt_v5_intrinsic_owner_pop(uint64_t id, struct BeskidCommand *command) {
  BeskidLock();
  struct BeskidOwner *owner = BeskidOwnerFind(id);
  int32_t found = owner && owner->count && command;
  if (found) {
    *command = owner->commands[owner->head];
    owner->head = (owner->head + 1) % 256;
    --owner->count;
  }
  BeskidUnlock();
  return found;
}
void beskid_rt_v5_intrinsic_owner_wait(uint64_t id, int64_t deadline) {
  BeskidLock();
  struct BeskidOwner *owner = BeskidOwnerFind(id);
  /* Testing the queue under the same lock as publication closes the
   * empty-queue / sleeping-owner race, including a signal before this call. */
  while (owner && !owner->count) {
    if (deadline >= 0 && beskid_rt_v5_intrinsic_clock_monotonic_nanos() >= deadline) break;
    BeskidConditionWait(&owner->wake, deadline);
  }
  BeskidUnlock();
}
int32_t beskid_rt_v5_intrinsic_wait_claim(uint64_t *state, uint64_t source) {
#ifdef _WIN32
  return InterlockedCompareExchange64((volatile LONG64 *)state, source, 0) == 0;
#else
  uint64_t expected = 0;
  return __atomic_compare_exchange_n(state, &expected, source, 0, __ATOMIC_ACQ_REL, __ATOMIC_ACQUIRE);
#endif
}

enum { BESKID_WORKER_READ = 1, BESKID_WORKER_WRITE = 2,
       BESKID_WORKER_QUEUED = 1, BESKID_WORKER_RUNNING = 2, BESKID_WORKER_COMPLETE = 3 };
enum {
  /* The low bit is the existing cancellation protocol; the high bit is private
   * ownership state. They cannot collide because abandonment has always been
   * written as zero or one. An owned duplicate may itself be descriptor zero. */
  BESKID_WORKER_ABANDONED = 1u,
  BESKID_WORKER_OWNS_DESCRIPTOR = UINT32_C(0x80000000)
};
struct BeskidWorkerRequest {
  struct BeskidWorkerRequest *next;
  uint64_t tag;
  uint32_t operation, state;
  uintptr_t native_handle;
  uint8_t *buffer;
  size_t length;
  intptr_t result;
  int32_t error;
  uint32_t abandoned;
  uint64_t owner;
};
#ifdef BESKID_NETWORK_TRANSPORT
static void BeskidNetworkDnsPerform(struct BeskidWorkerRequest *request);
static void BeskidNetworkDnsFree(struct BeskidWorkerRequest *request);
#endif
_Static_assert(sizeof(struct BeskidWorkerRequest) == BESKID_WORKER_REQUEST_SIZE, "manifest worker request size");
_Static_assert(offsetof(struct BeskidWorkerRequest, owner) == BESKID_WORKER_REQUEST_OWNER_SCHEDULER_ID_OFFSET, "manifest owner route offset");
_Static_assert(offsetof(struct BeskidWorkerRequest, state) == BESKID_WORKER_REQUEST_STATE_OFFSET, "manifest request state offset");
static int BeskidWorkerIsAbandoned(const struct BeskidWorkerRequest *request) {
  return (request->abandoned & BESKID_WORKER_ABANDONED) != 0;
}
static int BeskidWorkerOwnsDescriptor(const struct BeskidWorkerRequest *request) {
  return (request->abandoned & BESKID_WORKER_OWNS_DESCRIPTOR) != 0;
}
#ifdef _WIN32
static void BeskidWorkerInvalidParameter(const wchar_t *expression,
                                         const wchar_t *function,
                                         const wchar_t *file,
                                         unsigned int line,
                                         uintptr_t reserved) {
  (void)expression; (void)function; (void)file; (void)line; (void)reserved;
}
static void BeskidWorkerCloseDescriptor(struct BeskidWorkerRequest *request) {
  if (!BeskidWorkerOwnsDescriptor(request)) return;
  _invalid_parameter_handler previous =
      _set_thread_local_invalid_parameter_handler(BeskidWorkerInvalidParameter);
  (void)_close((int)request->native_handle);
  _set_thread_local_invalid_parameter_handler(previous);
  request->abandoned &= ~BESKID_WORKER_OWNS_DESCRIPTOR;
}
static int BeskidWorkerAcquireDescriptor(struct BeskidWorkerRequest *request) {
  if (request->native_handle > INT_MAX) { request->result = -1; request->error = EBADF; return 0; }
  _invalid_parameter_handler previous =
      _set_thread_local_invalid_parameter_handler(BeskidWorkerInvalidParameter);
  errno = 0;
  int descriptor = _dup((int)request->native_handle);
  int error = errno;
  if (descriptor >= 0) {
    errno = 0;
    if (_setmode(descriptor, _O_BINARY) < 0) {
      error = errno;
      (void)_close(descriptor);
      descriptor = -1;
    }
  }
  _set_thread_local_invalid_parameter_handler(previous);
  if (descriptor < 0) { request->result = -1; request->error = error; return 0; }
  request->native_handle = (uintptr_t)descriptor;
  request->abandoned |= BESKID_WORKER_OWNS_DESCRIPTOR;
  return 1;
}
#else
static void BeskidWorkerCloseDescriptor(struct BeskidWorkerRequest *request) {
  if (!BeskidWorkerOwnsDescriptor(request)) return;
  (void)close((int)request->native_handle);
  request->abandoned &= ~BESKID_WORKER_OWNS_DESCRIPTOR;
}
static int BeskidWorkerAcquireDescriptor(struct BeskidWorkerRequest *request) {
  if (request->native_handle > INT_MAX) { request->result = -1; request->error = EBADF; return 0; }
  int descriptor = fcntl((int)request->native_handle, F_DUPFD_CLOEXEC, 0);
  if (descriptor < 0) { request->result = -1; request->error = errno; return 0; }
  request->native_handle = (uintptr_t)descriptor;
  request->abandoned |= BESKID_WORKER_OWNS_DESCRIPTOR;
  return 1;
}
#endif
static void BeskidWorkerFree(struct BeskidWorkerRequest *request) {
#ifdef BESKID_NETWORK_TRANSPORT
  if (request->operation == 3) BeskidNetworkDnsFree(request);
#endif
  BeskidWorkerCloseDescriptor(request);
  beskid_rt_v5_intrinsic_system_free(request->buffer, request->length ? request->length : 1);
  beskid_rt_v5_intrinsic_system_free(request, sizeof(*request));
}
#define BESKID_WORKER_MAX 4
#define BESKID_REQUEST_MAX 32
#ifdef _WIN32
static HANDLE beskid_workers[BESKID_WORKER_MAX];
#else
static pthread_t beskid_workers[BESKID_WORKER_MAX];
#endif
static size_t beskid_worker_count, beskid_request_count;
static int beskid_workers_stop;
static struct BeskidWorkerRequest *beskid_work_head, *beskid_work_tail;
static void BeskidWorkerPerform(struct BeskidWorkerRequest *r) {
#ifdef BESKID_NETWORK_TRANSPORT
  if (r->operation == 3) { BeskidNetworkDnsPerform(r); return; }
#endif
#ifdef _WIN32
  _invalid_parameter_handler previous =
      _set_thread_local_invalid_parameter_handler(BeskidWorkerInvalidParameter);
  errno = 0;
  r->result = r->operation == BESKID_WORKER_READ
      ? _read((int)r->native_handle, r->buffer, (unsigned int)r->length)
      : _write((int)r->native_handle, r->buffer, (unsigned int)r->length);
  int error = errno;
  _set_thread_local_invalid_parameter_handler(previous);
  r->error = r->result < 0 ? error : 0;
#else
  errno = 0;
  r->result = r->operation == BESKID_WORKER_READ
      ? read((int)r->native_handle, r->buffer, r->length)
      : write((int)r->native_handle, r->buffer, r->length);
  r->error = r->result < 0 ? errno : 0;
#endif
}
#ifdef _WIN32
static DWORD WINAPI beskid_worker_main(LPVOID unused) {
#else
static void *beskid_worker_main(void *unused) {
#endif
  (void)unused;
  BeskidLock();
  for (;;) {
    while (!beskid_work_head && !beskid_workers_stop) BeskidConditionWait(&beskid_work_ready, -1);
    if (!beskid_work_head && beskid_workers_stop) break;
    struct BeskidWorkerRequest *request = beskid_work_head;
    beskid_work_head = request->next;
    if (!beskid_work_head) beskid_work_tail = NULL;
    request->next = NULL;
    if (BeskidWorkerIsAbandoned(request)) { --beskid_request_count; BeskidWorkerFree(request); continue; }
    request->state = BESKID_WORKER_RUNNING;
    BeskidUnlock();
    BeskidWorkerPerform(request);
    BeskidLock();
    if (BeskidWorkerIsAbandoned(request)) { --beskid_request_count; BeskidWorkerFree(request); }
    else {
      request->state = BESKID_WORKER_COMPLETE;
      /* This is transport publication, never terminal wait completion. */
      BeskidOwnerPostLocked(request->owner, request->tag, 1);
    }
  }
  BeskidUnlock();
  return 0;
}
int32_t beskid_rt_v5_intrinsic_worker_pool_init(size_t count) {
  if (beskid_worker_count || !count || count > BESKID_WORKER_MAX) return -1;
  BeskidLock(); beskid_workers_stop = 0; BeskidUnlock();
  size_t created = 0;
  for (; created < count; ++created) {
#ifdef _WIN32
    beskid_workers[created] = CreateThread(NULL, 0, beskid_worker_main, NULL, 0, NULL);
    if (!beskid_workers[created]) break;
#else
    if (pthread_create(&beskid_workers[created], NULL, beskid_worker_main, NULL)) break;
#endif
  }
  beskid_worker_count = created;
  if (created == count) return 0;
  beskid_rt_v5_intrinsic_worker_pool_shutdown();
  return -1;
}
void beskid_rt_v5_intrinsic_worker_pool_shutdown(void) {
  BeskidLock(); beskid_workers_stop = 1; BeskidConditionSignal(&beskid_work_ready); BeskidUnlock();
  for (size_t i = 0; i < beskid_worker_count; ++i) {
#ifdef _WIN32
    WaitForSingleObject(beskid_workers[i], INFINITE); CloseHandle(beskid_workers[i]);
#else
    pthread_join(beskid_workers[i], NULL);
#endif
  }
  beskid_worker_count = 0;
}
int32_t beskid_rt_v5_intrinsic_worker_submit(struct BeskidWorkerRequest *r) {
  if (!r || !beskid_worker_count) return -1;
#ifdef BESKID_NETWORK_TRANSPORT
  if (r->operation != BESKID_WORKER_READ && r->operation != BESKID_WORKER_WRITE && r->operation != 3) return -1;
#else
  if (r->operation != BESKID_WORKER_READ && r->operation != BESKID_WORKER_WRITE) return -1;
#endif
#ifdef _WIN32
  if (r->length > INT_MAX) return -1;
#endif
  if (r->operation != 3 && !BeskidWorkerAcquireDescriptor(r)) return -1;
  BeskidLock();
  if (beskid_workers_stop || beskid_request_count >= BESKID_REQUEST_MAX) {
    BeskidWorkerCloseDescriptor(r);
    BeskidUnlock();
    return -1;
  }
  r->state = BESKID_WORKER_QUEUED;
  r->next = NULL;
  ++beskid_request_count;
  if (beskid_work_tail) beskid_work_tail->next = r; else beskid_work_head = r;
  beskid_work_tail = r;
  BeskidConditionSignal(&beskid_work_ready);
  BeskidUnlock();
  return 0;
}
int32_t beskid_rt_v5_intrinsic_worker_poll(struct BeskidWorkerRequest *r) {
  BeskidLock(); int32_t ready = r && r->state == BESKID_WORKER_COMPLETE; BeskidUnlock();
  return ready;
}
void beskid_rt_v5_intrinsic_worker_release(struct BeskidWorkerRequest *r) {
  if (!r) return;
  BeskidLock();
  if (r->state == 0 || r->state == BESKID_WORKER_COMPLETE) {
    if (r->state != 0) --beskid_request_count;
    BeskidWorkerFree(r);
  } else {
    /* Cancellation unregisters the source. The worker retains its native
     * storage until its OS call returns; it cannot publish another command. */
    r->abandoned |= BESKID_WORKER_ABANDONED;
    r->owner = 0;
  }
  BeskidUnlock();
}
