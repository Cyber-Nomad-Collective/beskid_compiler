/* Bounded port rings between isolates (v0.6.4). Native transport only: the
 * Beskid runtime (Runtime/Parallel/Ports.bd) decides every outcome and owns the
 * values, which are opaque words here (a native transit record and a class tag).
 *
 * One mutex per ring protects the slots, the closed and cancelled flags, the
 * reference count and the waiter lists. A waiter is an owner mailbox id plus an
 * external wait token: a parked sender or receiver registers itself while the
 * operation fails under the same lock, so a wake-up can never be lost. A wake-up
 * is an owner mailbox post of the waiter's token with source READY (1); the
 * owner re-tries the operation. Lock order: ring mutex, then the transport lock
 * of external_wait.h. No function here touches a managed heap.
 *
 * Status codes: 0 done, 1 closed, 2 cancelled, 3 would block (a waiter is
 * registered when `token` is nonzero), 4 the system allocator refused a waiter. */

#ifndef BESKID_PORT_RING_H
#define BESKID_PORT_RING_H

struct BeskidPortWaiter {
  struct BeskidPortWaiter *next;
  uint64_t owner;
  uint64_t token;
};

struct BeskidPortRing {
#ifdef _WIN32
  SRWLOCK lock;
#else
  pthread_mutex_t lock;
#endif
  size_t capacity;
  size_t head;
  size_t count;
  size_t references;
  uint32_t closed;
  uint32_t cancelled;
  struct BeskidPortWaiter *receivers;
  struct BeskidPortWaiter *senders;
  size_t *slots; /* two words per slot: payload, tag */
};

#ifdef _WIN32
static void BeskidPortLock(struct BeskidPortRing *ring) { AcquireSRWLockExclusive(&ring->lock); }
static void BeskidPortUnlock(struct BeskidPortRing *ring) { ReleaseSRWLockExclusive(&ring->lock); }
#else
static void BeskidPortLock(struct BeskidPortRing *ring) { pthread_mutex_lock(&ring->lock); }
static void BeskidPortUnlock(struct BeskidPortRing *ring) { pthread_mutex_unlock(&ring->lock); }
#endif

void *beskid_rt_v5_intrinsic_port_ring_create(size_t capacity) {
  if (capacity == 0 || capacity > ((size_t)1 << 40)) return NULL;
  struct BeskidPortRing *ring = beskid_rt_v5_intrinsic_system_allocate(sizeof(*ring), 8);
  if (!ring) return NULL;
  __builtin_memset(ring, 0, sizeof(*ring));
  ring->slots = beskid_rt_v5_intrinsic_system_allocate(capacity * 2 * sizeof(size_t), 8);
  if (!ring->slots) {
    beskid_rt_v5_intrinsic_system_free(ring, sizeof(*ring));
    return NULL;
  }
  /* Static initializer values keep the platform import set unchanged: a default
   * mutex needs no init or destroy call on either supported libc. */
#ifdef _WIN32
  SRWLOCK unlocked = SRWLOCK_INIT;
#else
  pthread_mutex_t unlocked = PTHREAD_MUTEX_INITIALIZER;
#endif
  ring->lock = unlocked;
  ring->capacity = capacity;
  ring->references = 1;
  return ring;
}

static void BeskidPortWake(struct BeskidPortWaiter *waiter) {
  BeskidLock();
  (void)BeskidOwnerPostLocked(waiter->owner, waiter->token, 1);
  BeskidUnlock();
  beskid_rt_v5_intrinsic_system_free(waiter, sizeof(*waiter));
}

/* Wakes the oldest waiter of `list`; the waiter is removed. */
static void BeskidPortWakeOne(struct BeskidPortWaiter **list) {
  struct BeskidPortWaiter *waiter = *list;
  if (!waiter) return;
  *list = waiter->next;
  BeskidPortWake(waiter);
}

static void BeskidPortWakeAll(struct BeskidPortWaiter **list) {
  while (*list) BeskidPortWakeOne(list);
}

/* Appends a waiter at the tail so that wake-ups follow park order. */
static int32_t BeskidPortPark(struct BeskidPortWaiter **list, uint64_t owner, uint64_t token) {
  struct BeskidPortWaiter *waiter = beskid_rt_v5_intrinsic_system_allocate(sizeof(*waiter), 8);
  if (!waiter) return 4;
  waiter->next = NULL;
  waiter->owner = owner;
  waiter->token = token;
  while (*list) list = &(*list)->next;
  *list = waiter;
  return 3;
}

int32_t beskid_rt_v5_intrinsic_port_ring_send(void *raw, size_t payload, size_t tag, uint64_t owner, uint64_t token) {
  struct BeskidPortRing *ring = (struct BeskidPortRing *)raw;
  if (!ring) return 1;
  BeskidPortLock(ring);
  int32_t status = 0;
  if (ring->cancelled) {
    status = 2;
  } else if (ring->closed) {
    status = 1;
  } else if (ring->count == ring->capacity) {
    status = token ? BeskidPortPark(&ring->senders, owner, token) : 3;
  } else {
    size_t slot = (ring->head + ring->count) % ring->capacity;
    ring->slots[slot * 2] = payload;
    ring->slots[slot * 2 + 1] = tag;
    ring->count += 1;
    BeskidPortWakeOne(&ring->receivers);
  }
  BeskidPortUnlock(ring);
  return status;
}

int32_t beskid_rt_v5_intrinsic_port_ring_receive(void *raw, size_t *out, uint64_t owner, uint64_t token) {
  struct BeskidPortRing *ring = (struct BeskidPortRing *)raw;
  if (!ring || !out) return 1;
  BeskidPortLock(ring);
  int32_t status = 0;
  if (ring->cancelled) {
    status = 2;
  } else if (ring->count != 0) {
    out[0] = ring->slots[ring->head * 2];
    out[1] = ring->slots[ring->head * 2 + 1];
    ring->slots[ring->head * 2] = 0;
    ring->slots[ring->head * 2 + 1] = 0;
    ring->head = (ring->head + 1) % ring->capacity;
    ring->count -= 1;
    BeskidPortWakeOne(&ring->senders);
  } else if (ring->closed) {
    status = 1;
  } else {
    status = token ? BeskidPortPark(&ring->receivers, owner, token) : 3;
  }
  BeskidPortUnlock(ring);
  return status;
}

/* Withdraws a waiter that resumed for any reason other than its own wake-up. */
void beskid_rt_v5_intrinsic_port_ring_unwait(void *raw, uint64_t owner, uint64_t token) {
  struct BeskidPortRing *ring = (struct BeskidPortRing *)raw;
  if (!ring) return;
  BeskidPortLock(ring);
  struct BeskidPortWaiter **lists[2] = {&ring->receivers, &ring->senders};
  for (int list = 0; list < 2; ++list) {
    struct BeskidPortWaiter **link = lists[list];
    while (*link) {
      struct BeskidPortWaiter *waiter = *link;
      if (waiter->owner == owner && waiter->token == token) {
        *link = waiter->next;
        beskid_rt_v5_intrinsic_system_free(waiter, sizeof(*waiter));
      } else {
        link = &waiter->next;
      }
    }
  }
  BeskidPortUnlock(ring);
}

/* `cancel` 0 closes (buffered values stay receivable), 1 cancels (every later
 * operation reports 2). Both are idempotent and wake every parked waiter. */
void beskid_rt_v5_intrinsic_port_ring_close(void *raw, uint8_t cancel) {
  struct BeskidPortRing *ring = (struct BeskidPortRing *)raw;
  if (!ring) return;
  BeskidPortLock(ring);
  if (cancel) ring->cancelled = 1; else ring->closed = 1;
  BeskidPortWakeAll(&ring->receivers);
  BeskidPortWakeAll(&ring->senders);
  BeskidPortUnlock(ring);
}

void beskid_rt_v5_intrinsic_port_ring_retain(void *raw) {
  struct BeskidPortRing *ring = (struct BeskidPortRing *)raw;
  if (!ring) return;
  BeskidPortLock(ring);
  ring->references += 1;
  BeskidPortUnlock(ring);
}

/* Drops one reference. The last one frees the ring; a payload still buffered is
 * a native transit record whose first word is its byte size. */
void beskid_rt_v5_intrinsic_port_ring_release(void *raw) {
  struct BeskidPortRing *ring = (struct BeskidPortRing *)raw;
  if (!ring) return;
  BeskidPortLock(ring);
  size_t remaining = --ring->references;
  BeskidPortUnlock(ring);
  if (remaining != 0) return;
  while (ring->count != 0) {
    size_t *record = (size_t *)ring->slots[ring->head * 2];
    if (record) beskid_rt_v5_intrinsic_system_free(record, record[0]);
    ring->head = (ring->head + 1) % ring->capacity;
    ring->count -= 1;
  }
  BeskidPortWakeAll(&ring->receivers);
  BeskidPortWakeAll(&ring->senders);
  beskid_rt_v5_intrinsic_system_free(ring->slots, ring->capacity * 2 * sizeof(size_t));
  beskid_rt_v5_intrinsic_system_free(ring, sizeof(*ring));
}

/* Number of buffered values (diagnostic and test view). */
size_t beskid_rt_v5_intrinsic_port_ring_count(void *raw) {
  struct BeskidPortRing *ring = (struct BeskidPortRing *)raw;
  if (!ring) return 0;
  BeskidPortLock(ring);
  size_t count = ring->count;
  BeskidPortUnlock(ring);
  return count;
}

#endif
