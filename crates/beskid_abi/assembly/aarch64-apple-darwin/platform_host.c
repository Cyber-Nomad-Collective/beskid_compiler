#define _DARWIN_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#include "../../include/beskid_runtime_abi_v5.h"
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stddef.h>
#include <stdint.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

enum {
  BESKID_WORKER_READ = 1,
  BESKID_WORKER_WRITE = 2,
  BESKID_WORKER_QUEUED = 1,
  BESKID_WORKER_RUNNING = 2,
  BESKID_WORKER_COMPLETE = 3
};
struct BeskidWorkerRequest {
  void *next;
  uint64_t tag;
  uint32_t operation;
  _Atomic uint32_t state;
  uintptr_t native_handle;
  uint8_t *buffer;
  size_t length;
  intptr_t result;
  int32_t error;
  uint32_t padding;
};
#define BESKID_WORKER_MAX 4
#define BESKID_REQUEST_MAX 32
static pthread_t beskid_workers[BESKID_WORKER_MAX];
static size_t beskid_worker_count;
static _Atomic int beskid_workers_stop;
static _Atomic(struct BeskidWorkerRequest *)
    beskid_requests[BESKID_REQUEST_MAX];
static void *beskid_worker_main(void *unused) {
  (void)unused;
  while (!atomic_load_explicit(&beskid_workers_stop, memory_order_acquire)) {
    for (size_t i = 0; i < BESKID_REQUEST_MAX; ++i) {
      struct BeskidWorkerRequest *r =
          atomic_load_explicit(&beskid_requests[i], memory_order_acquire);
      uint32_t expected = BESKID_WORKER_QUEUED;
      if (!r || !atomic_compare_exchange_strong_explicit(
                    &r->state, &expected, BESKID_WORKER_RUNNING,
                    memory_order_acq_rel, memory_order_relaxed))
        continue;
      if (r->native_handle > INT_MAX) {
        r->result = -1;
        r->error = EBADF;
      } else {
        int fd = (int)r->native_handle;
        errno = 0;
        r->result = r->operation == BESKID_WORKER_READ
                        ? read(fd, r->buffer, r->length)
                        : write(fd, r->buffer, r->length);
        r->error = r->result < 0 ? errno : 0;
      }
      atomic_store_explicit(&r->state, BESKID_WORKER_COMPLETE,
                            memory_order_release);
    }
  }
  return NULL;
}
int32_t beskid_rt_v5_intrinsic_worker_pool_init(size_t count) {
  if (beskid_worker_count || count == 0 || count > BESKID_WORKER_MAX)
    return -1;
  atomic_store(&beskid_workers_stop, 0);
  for (size_t i = 0; i < count; ++i)
    if (pthread_create(&beskid_workers[i], NULL, beskid_worker_main, NULL) !=
        0) {
      atomic_store(&beskid_workers_stop, 1);
      while (i)
        pthread_join(beskid_workers[--i], NULL);
      return -1;
    }
  beskid_worker_count = count;
  return 0;
}
void beskid_rt_v5_intrinsic_worker_pool_shutdown(void) {
  atomic_store_explicit(&beskid_workers_stop, 1, memory_order_release);
  for (size_t i = 0; i < beskid_worker_count; ++i)
    pthread_join(beskid_workers[i], NULL);
  beskid_worker_count = 0;
}
int32_t beskid_rt_v5_intrinsic_worker_submit(struct BeskidWorkerRequest *r) {
  if (!r || !beskid_worker_count ||
      (r->operation != BESKID_WORKER_READ &&
       r->operation != BESKID_WORKER_WRITE))
    return -1;
  atomic_store_explicit(&r->state, BESKID_WORKER_QUEUED, memory_order_release);
  for (size_t i = 0; i < BESKID_REQUEST_MAX; ++i) {
    struct BeskidWorkerRequest *empty = NULL;
    if (atomic_compare_exchange_strong(&beskid_requests[i], &empty, r))
      return 0;
  }
  atomic_store(&r->state, 0);
  return -1;
}
int32_t beskid_rt_v5_intrinsic_worker_poll(struct BeskidWorkerRequest *r) {
  if (!r || atomic_load_explicit(&r->state, memory_order_acquire) !=
                BESKID_WORKER_COMPLETE)
    return 0;
  for (size_t i = 0; i < BESKID_REQUEST_MAX; ++i) {
    struct BeskidWorkerRequest *expected = r;
    (void)atomic_compare_exchange_strong(&beskid_requests[i], &expected, NULL);
  }
  return 1;
}

struct BeskidStr {
  const uint8_t *ptr;
  size_t len;
};
struct BeskidArgsState {
  int64_t count;
  struct BeskidStr *values;
};
static struct BeskidArgsState beskid_args;

static _Noreturn void beskid_args_trap(uint8_t code, const char *message) {
  beskid_rt_v5_trap(code, (void *)message, __builtin_strlen(message));
}

void beskid_rt_v5_args_handoff_utf8(int64_t argc, const char *const *argv) {
  if (argc < 0 || (argc != 0 && argv == NULL))
    beskid_args_trap(10, "Core.Args handoff is invalid");
  size_t headers = (size_t)argc * sizeof(struct BeskidStr);
  if (argc != 0 && headers / sizeof(struct BeskidStr) != (size_t)argc)
    beskid_args_trap(5, "Core.Args storage allocation failed");
  size_t bytes = 0;
  for (int64_t i = 0; i < argc; ++i) {
    if (argv[i] == NULL)
      beskid_args_trap(10, "Core.Args handoff is invalid");
    size_t len = __builtin_strlen(argv[i]);
    if (len > SIZE_MAX - bytes)
      beskid_args_trap(5, "Core.Args storage allocation failed");
    bytes += len;
  }
  if (headers > SIZE_MAX - bytes)
    beskid_args_trap(5, "Core.Args storage allocation failed");
  size_t total = headers + bytes;
  if (total == 0)
    total = 1;
  unsigned char *storage =
      mmap(NULL, total, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANON, -1, 0);
  if (storage == MAP_FAILED)
    beskid_args_trap(5, "Core.Args storage allocation failed");
  struct BeskidStr *values = (struct BeskidStr *)storage;
  unsigned char *cursor = storage + headers;
  for (int64_t i = 0; i < argc; ++i) {
    size_t len = __builtin_strlen(argv[i]);
    __builtin_memcpy(cursor, argv[i], len);
    values[i] = (struct BeskidStr){.ptr = cursor, .len = len};
    cursor += len;
  }
  beskid_args = (struct BeskidArgsState){.count = argc, .values = values};
}

int64_t beskid_rt_v5_args_count(void) { return beskid_args.count; }
struct BeskidStr *beskid_rt_v5_args_get(int64_t index) {
  if (index < 0 || index >= beskid_args.count)
    beskid_args_trap(2, "Core.Args argument index is out of range");
  return &beskid_args.values[index];
}

#ifndef MAP_ANON
#ifdef MAP_ANONYMOUS
#define MAP_ANON MAP_ANONYMOUS
#else
#define MAP_ANON 0x1000
#endif
#endif

#define BESKID_GUARDED_STACK_MIN (64u * 1024u)
#define BESKID_GUARDED_STACK_MAX (8u * 1024u * 1024u)
#define BESKID_GUARDED_STACK_GUARD (16u * 1024u)

static int beskid_guarded_stack_contract_valid(size_t initial_size,
                                               size_t maximum_size) {
  return initial_size == BESKID_GUARDED_STACK_MIN &&
         maximum_size == BESKID_GUARDED_STACK_MAX;
}

void *beskid_rt_v5_intrinsic_guarded_stack_allocate(size_t initial_size,
                                                    size_t maximum_size) {
  if (!beskid_guarded_stack_contract_valid(initial_size, maximum_size))
    return NULL;
  size_t total_size = maximum_size + BESKID_GUARDED_STACK_GUARD;
  void *reservation =
      mmap(NULL, total_size, PROT_NONE, MAP_PRIVATE | MAP_ANON, -1, 0);
  if (reservation == MAP_FAILED)
    return NULL;
  unsigned char *usable_base =
      (unsigned char *)reservation + BESKID_GUARDED_STACK_GUARD;
  unsigned char *initial_base = usable_base + maximum_size - initial_size;
  if (mprotect(initial_base, initial_size, PROT_READ | PROT_WRITE) != 0) {
    (void)munmap(reservation, total_size);
    return NULL;
  }
  return usable_base;
}

uint8_t beskid_rt_v5_intrinsic_guarded_stack_grow(void *usable_base,
                                                  size_t committed_size,
                                                  size_t requested_size,
                                                  size_t maximum_size) {
  if (usable_base == NULL || maximum_size != BESKID_GUARDED_STACK_MAX ||
      committed_size < BESKID_GUARDED_STACK_MIN ||
      committed_size > maximum_size || requested_size < committed_size ||
      requested_size > maximum_size ||
      committed_size % BESKID_GUARDED_STACK_GUARD != 0 ||
      requested_size % BESKID_GUARDED_STACK_GUARD != 0)
    return 0;
  if (requested_size == committed_size)
    return 1;
  unsigned char *growth_base =
      (unsigned char *)usable_base + maximum_size - requested_size;
  return mprotect(growth_base, requested_size - committed_size,
                  PROT_READ | PROT_WRITE) == 0;
}

void beskid_rt_v5_intrinsic_guarded_stack_free(void *usable_base,
                                               size_t maximum_size) {
  if (usable_base == NULL || maximum_size != BESKID_GUARDED_STACK_MAX)
    return;
  unsigned char *reservation =
      (unsigned char *)usable_base - BESKID_GUARDED_STACK_GUARD;
  (void)munmap(reservation,
               BESKID_GUARDED_STACK_GUARD + BESKID_GUARDED_STACK_MAX);
}

static int64_t beskid_clock_nanos(clockid_t clock_id) {
  struct timespec value;
  if (clock_gettime(clock_id, &value) != 0)
    return 0;
  return (int64_t)value.tv_sec * INT64_C(1000000000) + value.tv_nsec;
}

int64_t beskid_rt_v5_intrinsic_clock_monotonic_nanos(void) {
  return beskid_clock_nanos(CLOCK_MONOTONIC);
}
int64_t beskid_rt_v5_intrinsic_clock_realtime_nanos(void) {
  return beskid_clock_nanos(CLOCK_REALTIME);
}
void beskid_rt_v5_intrinsic_process_exit(int32_t code) { _exit(code); }
int32_t beskid_rt_v5_intrinsic_process_getpid(void) {
  return (int32_t)getpid();
}

enum {
  BESKID_FS_OK = 0,
  BESKID_FS_NOT_FOUND = 1,
  BESKID_FS_PERMISSION_DENIED = 2,
  BESKID_FS_IO_ERROR = 3,
  BESKID_FS_INVALID_INPUT = 4,
  BESKID_FS_ALREADY_EXISTS = 5
};
static int32_t beskid_darwin_fs_status(int e) {
  if (e == ENOENT || e == ENOTDIR)
    return 1;
  if (e == EACCES || e == EPERM || e == EROFS)
    return 2;
  if (e == EINVAL || e == ENAMETOOLONG || e == EISDIR)
    return 4;
  if (e == EEXIST)
    return 5;
  return 3;
}
static char *beskid_darwin_path(const struct BeskidStr *path,
                                size_t *size_out) {
  if (!path || (path->len && !path->ptr) || path->len == SIZE_MAX)
    return NULL;
  size_t n = path->len + 1;
  char *p =
      mmap(NULL, n, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANON, -1, 0);
  if (p == MAP_FAILED)
    return NULL;
  if (path->len != 0)
    __builtin_memcpy(p, path->ptr, path->len);
  p[path->len] = 0;
  *size_out = n;
  return p;
}
int32_t beskid_rt_v5_darwin_fs_read_text(const struct BeskidStr *path,
                                         uint8_t **bytes_out,
                                         size_t *length_out) {
  if (!bytes_out || !length_out)
    return 4;
  *bytes_out = NULL;
  *length_out = 0;
  size_t pn = 0;
  char *p = beskid_darwin_path(path, &pn);
  if (!p)
    return 4;
  int fd = open(p, O_RDONLY), e = errno;
  (void)munmap(p, pn);
  if (fd < 0)
    return beskid_darwin_fs_status(e);
  struct stat s;
  if (fstat(fd, &s) || s.st_size < 0) {
    e = errno;
    (void)close(fd);
    return beskid_darwin_fs_status(e);
  }
  size_t len = (size_t)s.st_size;
  if ((off_t)len != s.st_size) {
    (void)close(fd);
    return 3;
  }
  size_t allocation = len ? len : 1;
  uint8_t *bytes = mmap(NULL, allocation, PROT_READ | PROT_WRITE,
                        MAP_PRIVATE | MAP_ANON, -1, 0);
  if (bytes == MAP_FAILED) {
    (void)close(fd);
    return 3;
  }
  size_t at = 0;
  while (at < len) {
    ssize_t n = read(fd, bytes + at, len - at);
    if (n <= 0) {
      e = n ? errno : EIO;
      (void)close(fd);
      (void)munmap(bytes, allocation);
      return beskid_darwin_fs_status(e);
    }
    at += (size_t)n;
  }
  if (close(fd)) {
    e = errno;
    (void)munmap(bytes, allocation);
    return beskid_darwin_fs_status(e);
  }
  *bytes_out = bytes;
  *length_out = len;
  return 0;
}
void beskid_rt_v5_darwin_fs_read_text_release(uint8_t *bytes, size_t length) {
  if (bytes)
    (void)munmap(bytes, length ? length : 1);
}
int32_t beskid_rt_v5_darwin_fs_write_text(const struct BeskidStr *path,
                                          const struct BeskidStr *text) {
  if (!text || (text->len && !text->ptr))
    return 4;
  size_t pn = 0;
  char *p = beskid_darwin_path(path, &pn);
  if (!p)
    return 4;
  int fd = open(p, O_WRONLY | O_CREAT | O_TRUNC, 0666), e = errno;
  (void)munmap(p, pn);
  if (fd < 0)
    return beskid_darwin_fs_status(e);
  size_t at = 0;
  while (at < text->len) {
    ssize_t n = write(fd, text->ptr + at, text->len - at);
    if (n <= 0) {
      e = n ? errno : EIO;
      (void)close(fd);
      return beskid_darwin_fs_status(e);
    }
    at += (size_t)n;
  }
  return close(fd) == 0 ? 0 : beskid_darwin_fs_status(errno);
}
int32_t beskid_rt_v5_darwin_fs_exists(const struct BeskidStr *path) {
  size_t n = 0;
  char *p = beskid_darwin_path(path, &n);
  if (!p)
    return 4;
  struct stat s;
  int r = stat(p, &s), e = errno;
  (void)munmap(p, n);
  return r == 0 ? 0 : beskid_darwin_fs_status(e);
}
int32_t beskid_rt_v5_darwin_fs_mkdir(const struct BeskidStr *path) {
  size_t n = 0;
  char *p = beskid_darwin_path(path, &n);
  if (!p)
    return 4;
  int r = mkdir(p, 0777), e = errno;
  (void)munmap(p, n);
  return r == 0 ? 0 : beskid_darwin_fs_status(e);
}
int32_t beskid_rt_v5_darwin_fs_delete(const struct BeskidStr *path) {
  size_t n = 0;
  char *p = beskid_darwin_path(path, &n);
  if (!p)
    return 4;
  int r = unlink(p), e = errno;
  (void)munmap(p, n);
  return r == 0 ? 0 : beskid_darwin_fs_status(e);
}
