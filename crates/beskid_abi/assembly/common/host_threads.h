/* OS thread, atomic word and host processor primitives for the canonical
 * runtime corpus (Runtime/Bootstrap/Native.bd). Policy stays in Beskid: these
 * functions start and join native threads, perform single-word atomic
 * operations, report the host's logical processor count and the kit's build
 * profile, and expose one process-wide native word block. They never touch a managed heap.
 *
 * Memory order bytes follow C11 `memory_order` without `consume`:
 * 0 relaxed, 1 acquire, 2 release, 3 acquire-release, 4 sequentially
 * consistent. Any other byte is treated as sequentially consistent. */

#ifndef BESKID_HOST_THREADS_H
#define BESKID_HOST_THREADS_H

extern void *beskid_rt_v5_intrinsic_system_allocate(size_t, size_t);
extern void beskid_rt_v5_intrinsic_system_free(void *, size_t);

typedef void *(*BeskidThreadEntry)(void *);

/* The opaque thread handle is a native cell owned by the host from a
 * successful create until the matching join. */
struct BeskidThreadCell {
#ifdef _WIN32
  HANDLE thread;
#else
  pthread_t thread;
#endif
  BeskidThreadEntry entry;
  void *argument;
};

#ifdef _WIN32
static DWORD WINAPI beskid_thread_trampoline(LPVOID raw) {
  struct BeskidThreadCell *cell = (struct BeskidThreadCell *)raw;
  (void)cell->entry(cell->argument);
  return 0;
}
#else
static void *beskid_thread_trampoline(void *raw) {
  struct BeskidThreadCell *cell = (struct BeskidThreadCell *)raw;
  return cell->entry(cell->argument);
}
#endif

int32_t beskid_rt_v5_intrinsic_thread_create(void *entry, void *argument, void **handle_out) {
  if (!entry || !handle_out) return -1;
  struct BeskidThreadCell *cell =
      (struct BeskidThreadCell *)beskid_rt_v5_intrinsic_system_allocate(sizeof(struct BeskidThreadCell), 8);
  if (!cell) return -1;
  cell->entry = (BeskidThreadEntry)entry;
  cell->argument = argument;
#ifdef _WIN32
  cell->thread = CreateThread(NULL, 0, beskid_thread_trampoline, cell, 0, NULL);
  if (!cell->thread) {
    beskid_rt_v5_intrinsic_system_free(cell, sizeof(struct BeskidThreadCell));
    return -1;
  }
#else
  if (pthread_create(&cell->thread, NULL, beskid_thread_trampoline, cell) != 0) {
    beskid_rt_v5_intrinsic_system_free(cell, sizeof(struct BeskidThreadCell));
    return -1;
  }
#endif
  *handle_out = cell;
  return 0;
}

int32_t beskid_rt_v5_intrinsic_thread_join(void *handle) {
  struct BeskidThreadCell *cell = (struct BeskidThreadCell *)handle;
  if (!cell) return -1;
#ifdef _WIN32
  if (WaitForSingleObject(cell->thread, INFINITE) != WAIT_OBJECT_0) return -1;
  (void)CloseHandle(cell->thread);
#else
  if (pthread_join(cell->thread, NULL) != 0) return -1;
#endif
  beskid_rt_v5_intrinsic_system_free(cell, sizeof(struct BeskidThreadCell));
  return 0;
}

#ifdef _WIN32
/* MSVC `/std:c11` has no <stdatomic.h> without an experimental switch. The
 * Interlocked family is a full barrier, which satisfies every order byte. */
size_t beskid_rt_v5_intrinsic_atomic_word_load(void *address, uint8_t order) {
  (void)order;
  return (size_t)InterlockedOr64((volatile LONG64 *)address, 0);
}
void beskid_rt_v5_intrinsic_atomic_word_store(void *address, size_t value, uint8_t order) {
  (void)order;
  (void)InterlockedExchange64((volatile LONG64 *)address, (LONG64)value);
}
size_t beskid_rt_v5_intrinsic_atomic_word_compare_exchange(void *address, size_t expected, size_t desired,
                                                           uint8_t order) {
  (void)order;
  return (size_t)InterlockedCompareExchange64((volatile LONG64 *)address, (LONG64)desired, (LONG64)expected);
}
size_t beskid_rt_v5_intrinsic_atomic_word_fetch_add(void *address, size_t delta, uint8_t order) {
  (void)order;
  return (size_t)InterlockedExchangeAdd64((volatile LONG64 *)address, (LONG64)delta);
}
void beskid_rt_v5_intrinsic_atomic_fence(uint8_t order) {
  (void)order;
  MemoryBarrier();
}
#else
static memory_order beskid_memory_order(uint8_t order) {
  switch (order) {
  case 0: return memory_order_relaxed;
  case 1: return memory_order_acquire;
  case 2: return memory_order_release;
  case 3: return memory_order_acq_rel;
  default: return memory_order_seq_cst;
  }
}
/* C11 forbids release and acq_rel on a plain load, and acquire and acq_rel on
 * a plain store; the nearest legal order that is at least as strong is used. */
static memory_order beskid_load_order(uint8_t order) {
  memory_order selected = beskid_memory_order(order);
  if (selected == memory_order_release) return memory_order_relaxed;
  if (selected == memory_order_acq_rel) return memory_order_acquire;
  return selected;
}
static memory_order beskid_store_order(uint8_t order) {
  memory_order selected = beskid_memory_order(order);
  if (selected == memory_order_acquire) return memory_order_relaxed;
  if (selected == memory_order_acq_rel) return memory_order_release;
  return selected;
}
/* A failed compare-exchange is a load; it may not carry release semantics. */
static memory_order beskid_failure_order(uint8_t order) {
  memory_order selected = beskid_memory_order(order);
  if (selected == memory_order_release) return memory_order_relaxed;
  if (selected == memory_order_acq_rel) return memory_order_acquire;
  return selected;
}
size_t beskid_rt_v5_intrinsic_atomic_word_load(void *address, uint8_t order) {
  return atomic_load_explicit((_Atomic size_t *)address, beskid_load_order(order));
}
void beskid_rt_v5_intrinsic_atomic_word_store(void *address, size_t value, uint8_t order) {
  atomic_store_explicit((_Atomic size_t *)address, value, beskid_store_order(order));
}
size_t beskid_rt_v5_intrinsic_atomic_word_compare_exchange(void *address, size_t expected, size_t desired,
                                                           uint8_t order) {
  size_t observed = expected;
  (void)atomic_compare_exchange_strong_explicit((_Atomic size_t *)address, &observed, desired,
                                                beskid_memory_order(order), beskid_failure_order(order));
  return observed;
}
size_t beskid_rt_v5_intrinsic_atomic_word_fetch_add(void *address, size_t delta, uint8_t order) {
  return atomic_fetch_add_explicit((_Atomic size_t *)address, delta, beskid_memory_order(order));
}
void beskid_rt_v5_intrinsic_atomic_fence(uint8_t order) {
  if (order == 0) return;
  atomic_thread_fence(beskid_memory_order(order));
}
#endif

/* The host query is a macro so that a native fixture can substitute a failing
 * query and observe the fallback. Production builds never define it. */
#ifndef BESKID_HOST_PROCESSOR_QUERY
#ifdef _WIN32
#define BESKID_HOST_PROCESSOR_QUERY() ((long long)GetActiveProcessorCount(ALL_PROCESSOR_GROUPS))
#else
#define BESKID_HOST_PROCESSOR_QUERY() ((long long)sysconf(_SC_NPROCESSORS_ONLN))
#endif
#endif
size_t beskid_rt_v5_host_processor_count(void) {
  long long count = BESKID_HOST_PROCESSOR_QUERY();
  return count < 1 ? (size_t)1 : (size_t)count;
}

/* One zero-initialized block of process-wide native words. Only the canonical
 * runtime corpus reads or writes it (Runtime/Bootstrap/Lifecycle.bd and
 * Runtime/Mem/Gc/RegionMap.bd own the word layout). */
/* The runtime kit's build profile. The kit builder compiles this object with
 * BESKID_RUNTIME_KIT_DEBUG=1 for the debug profile; any other build reports
 * release, so a kit without the definition never poisons or quarantines. */
uint8_t beskid_rt_v5_intrinsic_kit_debug_profile(void) {
#if defined(BESKID_RUNTIME_KIT_DEBUG) && BESKID_RUNTIME_KIT_DEBUG
  return 1;
#else
  return 0;
#endif
}

#define BESKID_PROCESS_STATE_WORDS 16
static size_t beskid_process_state[BESKID_PROCESS_STATE_WORDS];
void *beskid_rt_v5_intrinsic_process_state_root(void) { return beskid_process_state; }

#endif
