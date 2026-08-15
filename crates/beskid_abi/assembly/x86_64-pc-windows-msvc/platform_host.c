#include "../../include/beskid_runtime_abi_v5.h"
#include "../common/args_utf16.h"
#include <stddef.h>
#include <stdint.h>
#include <windows.h>

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
  volatile LONG state;
  uintptr_t native_handle;
  uint8_t *buffer;
  size_t length;
  intptr_t result;
  int32_t error;
  uint32_t padding;
};
#define BESKID_WORKER_MAX 4
#define BESKID_REQUEST_MAX 32
static HANDLE beskid_workers[BESKID_WORKER_MAX];
static size_t beskid_worker_count;
static volatile LONG beskid_workers_stop;
static struct BeskidWorkerRequest *volatile beskid_requests[BESKID_REQUEST_MAX];
static DWORD WINAPI beskid_worker_main(LPVOID unused) {
  (void)unused;
  while (InterlockedCompareExchange(&beskid_workers_stop, 0, 0) == 0) {
    for (size_t i = 0; i < BESKID_REQUEST_MAX; ++i) {
      struct BeskidWorkerRequest *r = InterlockedCompareExchangePointer(
          (PVOID volatile *)&beskid_requests[i], NULL, NULL);
      if (!r || InterlockedCompareExchange(&r->state, BESKID_WORKER_RUNNING,
                                           BESKID_WORKER_QUEUED) !=
                    BESKID_WORKER_QUEUED)
        continue;
      DWORD transferred = 0;
      HANDLE h = (HANDLE)r->native_handle;
      BOOL ok =
          r->operation == BESKID_WORKER_READ
              ? ReadFile(h, r->buffer, (DWORD)r->length, &transferred, NULL)
          : r->operation == BESKID_WORKER_WRITE
              ? WriteFile(h, r->buffer, (DWORD)r->length, &transferred, NULL)
              : FALSE;
      r->result = ok ? (intptr_t)transferred : -1;
      r->error = ok ? 0 : (int32_t)GetLastError();
      InterlockedExchange(&r->state, BESKID_WORKER_COMPLETE);
    }
  }
  return 0;
}
int32_t beskid_rt_v5_intrinsic_worker_pool_init(size_t count) {
  if (beskid_worker_count || !count || count > BESKID_WORKER_MAX)
    return -1;
  InterlockedExchange(&beskid_workers_stop, 0);
  for (size_t i = 0; i < count; ++i) {
    beskid_workers[i] =
        CreateThread(NULL, 0, beskid_worker_main, NULL, 0, NULL);
    if (!beskid_workers[i]) {
      InterlockedExchange(&beskid_workers_stop, 1);
      while (i) {
        --i;
        WaitForSingleObject(beskid_workers[i], INFINITE);
        CloseHandle(beskid_workers[i]);
      }
      return -1;
    }
  }
  beskid_worker_count = count;
  return 0;
}
void beskid_rt_v5_intrinsic_worker_pool_shutdown(void) {
  InterlockedExchange(&beskid_workers_stop, 1);
  for (size_t i = 0; i < beskid_worker_count; ++i) {
    WaitForSingleObject(beskid_workers[i], INFINITE);
    CloseHandle(beskid_workers[i]);
  }
  beskid_worker_count = 0;
}
int32_t beskid_rt_v5_intrinsic_worker_submit(struct BeskidWorkerRequest *r) {
  if (!r)
    return -1;
  if (r->operation != BESKID_WORKER_READ &&
      r->operation != BESKID_WORKER_WRITE) {
    r->result = -1;
    r->error = ERROR_INVALID_FUNCTION;
    InterlockedExchange(&r->state, 0);
    return -1;
  }
  if (!beskid_worker_count || r->length > UINT32_MAX)
    return -1;
  InterlockedExchange(&r->state, BESKID_WORKER_QUEUED);
  for (size_t i = 0; i < BESKID_REQUEST_MAX; ++i)
    if (InterlockedCompareExchangePointer((PVOID volatile *)&beskid_requests[i],
                                          r, NULL) == NULL)
      return 0;
  InterlockedExchange(&r->state, 0);
  return -1;
}
int32_t beskid_rt_v5_intrinsic_worker_poll(struct BeskidWorkerRequest *r) {
  if (!r ||
      InterlockedCompareExchange(&r->state, 0, 0) != BESKID_WORKER_COMPLETE)
    return 0;
  for (size_t i = 0; i < BESKID_REQUEST_MAX; ++i)
    (void)InterlockedCompareExchangePointer(
        (PVOID volatile *)&beskid_requests[i], NULL, r);
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

static __declspec(noreturn) void beskid_args_trap(uint8_t code,
                                                  const char *message,
                                                  size_t message_len) {
  beskid_rt_v5_trap(code, (void *)message, message_len);
}

#define BESKID_ARGS_TRAP(code, message) \
  beskid_args_trap((code), (message), sizeof(message) - 1)

void beskid_rt_v5_args_handoff_utf16(int64_t argc,
                                      const uint16_t *const *argv) {
  if (argc < 0 || (argc != 0 && argv == NULL))
    BESKID_ARGS_TRAP(10, "Core.Args handoff is invalid");
  size_t headers = (size_t)argc * sizeof(struct BeskidStr), bytes = 0;
  if (argc != 0 && headers / sizeof(struct BeskidStr) != (size_t)argc)
    BESKID_ARGS_TRAP(5, "Core.Args storage allocation failed");
  for (int64_t i = 0; i < argc; ++i) {
    if (argv[i] == NULL)
      BESKID_ARGS_TRAP(10, "Core.Args handoff is invalid");
    size_t len = beskid_args_utf8_length(argv[i]);
    if (len > SIZE_MAX - bytes)
      BESKID_ARGS_TRAP(5, "Core.Args storage allocation failed");
    bytes += len;
  }
  if (headers > SIZE_MAX - bytes)
    BESKID_ARGS_TRAP(5, "Core.Args storage allocation failed");
  size_t total = headers + bytes;
  if (total == 0)
    total = 1;
  unsigned char *storage =
      VirtualAlloc(NULL, total, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
  if (storage == NULL)
    BESKID_ARGS_TRAP(5, "Core.Args storage allocation failed");
  struct BeskidStr *values = (struct BeskidStr *)storage;
  unsigned char *cursor = storage + headers;
  for (int64_t i = 0; i < argc; ++i) {
    unsigned char *start = cursor;
    cursor = beskid_args_write_utf8(cursor, argv[i]);
    values[i] =
        (struct BeskidStr){.ptr = start, .len = (size_t)(cursor - start)};
  }
  beskid_args = (struct BeskidArgsState){.count = argc, .values = values};
}

int64_t beskid_rt_v5_args_count(void) { return beskid_args.count; }
struct BeskidStr *beskid_rt_v5_args_get(int64_t index) {
  if (index < 0 || index >= beskid_args.count)
    BESKID_ARGS_TRAP(2, "Core.Args argument index is out of range");
  return &beskid_args.values[index];
}

#define BESKID_GUARDED_STACK_MIN (64u * 1024u)
#define BESKID_GUARDED_STACK_MAX (8u * 1024u * 1024u)
#define BESKID_GUARDED_STACK_GUARD (64u * 1024u)

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
  unsigned char *reservation = (unsigned char *)VirtualAlloc(
      NULL, total_size, MEM_RESERVE, PAGE_NOACCESS);
  if (reservation == NULL)
    return NULL;
  unsigned char *usable_base = reservation + BESKID_GUARDED_STACK_GUARD;
  unsigned char *initial_base = usable_base + maximum_size - initial_size;
  if (VirtualAlloc(initial_base, initial_size, MEM_COMMIT, PAGE_READWRITE) ==
      NULL) {
    (void)VirtualFree(reservation, 0, MEM_RELEASE);
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
  return VirtualAlloc(growth_base, requested_size - committed_size, MEM_COMMIT,
                      PAGE_READWRITE) != NULL;
}

void beskid_rt_v5_intrinsic_guarded_stack_free(void *usable_base,
                                               size_t maximum_size) {
  if (usable_base == NULL || maximum_size != BESKID_GUARDED_STACK_MAX)
    return;
  unsigned char *reservation =
      (unsigned char *)usable_base - BESKID_GUARDED_STACK_GUARD;
  (void)VirtualFree(reservation, 0, MEM_RELEASE);
}

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
  return (int64_t)((ticks.QuadPart - UINT64_C(116444736000000000)) *
                   UINT64_C(100));
}

void beskid_rt_v5_intrinsic_process_exit(int32_t code) {
  ExitProcess((UINT)code);
}
int32_t beskid_rt_v5_intrinsic_process_getpid(void) {
  return (int32_t)GetCurrentProcessId();
}

enum {
  BESKID_FS_OK = 0,
  BESKID_FS_NOT_FOUND = 1,
  BESKID_FS_PERMISSION_DENIED = 2,
  BESKID_FS_IO_ERROR = 3,
  BESKID_FS_INVALID_INPUT = 4,
  BESKID_FS_ALREADY_EXISTS = 5
};
static int32_t beskid_windows_fs_status(DWORD e) {
  if (e == ERROR_FILE_NOT_FOUND || e == ERROR_PATH_NOT_FOUND)
    return 1;
  if (e == ERROR_ACCESS_DENIED || e == ERROR_SHARING_VIOLATION ||
      e == ERROR_WRITE_PROTECT)
    return 2;
  if (e == ERROR_INVALID_NAME || e == ERROR_BAD_PATHNAME ||
      e == ERROR_INVALID_PARAMETER)
    return 4;
  if (e == ERROR_FILE_EXISTS || e == ERROR_ALREADY_EXISTS)
    return 5;
  return 3;
}
static WCHAR *beskid_windows_path(const struct BeskidStr *path) {
  if (!path || (path->len && !path->ptr) || path->len > INT_MAX)
    return NULL;
  int chars =
      MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS,
                          (const char *)path->ptr, (int)path->len, NULL, 0);
  if (chars <= 0 && path->len != 0)
    return NULL;
  size_t bytes = ((size_t)chars + 1) * sizeof(WCHAR);
  WCHAR *wide =
      VirtualAlloc(NULL, bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
  if (!wide)
    return NULL;
  if (chars && MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS,
                                   (const char *)path->ptr, (int)path->len,
                                   wide, chars) != chars) {
    (void)VirtualFree(wide, 0, MEM_RELEASE);
    return NULL;
  }
  wide[chars] = 0;
  return wide;
}
int32_t beskid_rt_v5_windows_fs_read_text(const struct BeskidStr *path,
                                          uint8_t **bytes_out,
                                          size_t *length_out) {
  if (!bytes_out || !length_out)
    return 4;
  *bytes_out = NULL;
  *length_out = 0;
  WCHAR *p = beskid_windows_path(path);
  if (!p)
    return 4;
  HANDLE f = CreateFileW(p, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING,
                         FILE_ATTRIBUTE_NORMAL, NULL);
  DWORD e = GetLastError();
  (void)VirtualFree(p, 0, MEM_RELEASE);
  if (f == INVALID_HANDLE_VALUE)
    return beskid_windows_fs_status(e);
  LARGE_INTEGER size;
  if (!GetFileSizeEx(f, &size) || size.QuadPart < 0 ||
      (uint64_t)size.QuadPart > SIZE_MAX) {
    e = GetLastError();
    CloseHandle(f);
    return beskid_windows_fs_status(e);
  }
  size_t len = (size_t)size.QuadPart, allocation = len ? len : 1;
  uint8_t *bytes =
      VirtualAlloc(NULL, allocation, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
  if (!bytes) {
    CloseHandle(f);
    return 3;
  }
  size_t at = 0;
  while (at < len) {
    DWORD chunk = (DWORD)((len - at) > UINT32_MAX ? UINT32_MAX : (len - at)),
          got = 0;
    if (!ReadFile(f, bytes + at, chunk, &got, NULL) || got == 0) {
      e = GetLastError();
      CloseHandle(f);
      VirtualFree(bytes, 0, MEM_RELEASE);
      return beskid_windows_fs_status(e);
    }
    at += got;
  }
  if (!CloseHandle(f)) {
    e = GetLastError();
    VirtualFree(bytes, 0, MEM_RELEASE);
    return beskid_windows_fs_status(e);
  }
  *bytes_out = bytes;
  *length_out = len;
  return 0;
}
void beskid_rt_v5_windows_fs_read_text_release(uint8_t *bytes, size_t length) {
  (void)length;
  if (bytes)
    (void)VirtualFree(bytes, 0, MEM_RELEASE);
}
int32_t beskid_rt_v5_windows_fs_write_text(const struct BeskidStr *path,
                                           const struct BeskidStr *text) {
  if (!text || (text->len && !text->ptr))
    return 4;
  WCHAR *p = beskid_windows_path(path);
  if (!p)
    return 4;
  HANDLE f = CreateFileW(p, GENERIC_WRITE, 0, NULL, CREATE_ALWAYS,
                         FILE_ATTRIBUTE_NORMAL, NULL);
  DWORD e = GetLastError();
  VirtualFree(p, 0, MEM_RELEASE);
  if (f == INVALID_HANDLE_VALUE)
    return beskid_windows_fs_status(e);
  size_t at = 0;
  while (at < text->len) {
    DWORD chunk = (DWORD)((text->len - at) > UINT32_MAX ? UINT32_MAX
                                                        : (text->len - at)),
          put = 0;
    if (!WriteFile(f, text->ptr + at, chunk, &put, NULL) || put == 0) {
      e = GetLastError();
      CloseHandle(f);
      return beskid_windows_fs_status(e);
    }
    at += put;
  }
  return CloseHandle(f) ? 0 : beskid_windows_fs_status(GetLastError());
}
int32_t beskid_rt_v5_windows_fs_exists(const struct BeskidStr *path) {
  WCHAR *p = beskid_windows_path(path);
  if (!p)
    return 4;
  DWORD a = GetFileAttributesW(p), e = GetLastError();
  VirtualFree(p, 0, MEM_RELEASE);
  return a == INVALID_FILE_ATTRIBUTES ? beskid_windows_fs_status(e) : 0;
}
int32_t beskid_rt_v5_windows_fs_mkdir(const struct BeskidStr *path) {
  WCHAR *p = beskid_windows_path(path);
  if (!p)
    return 4;
  BOOL ok = CreateDirectoryW(p, NULL);
  DWORD e = GetLastError();
  VirtualFree(p, 0, MEM_RELEASE);
  return ok ? 0 : beskid_windows_fs_status(e);
}
int32_t beskid_rt_v5_windows_fs_delete(const struct BeskidStr *path) {
  WCHAR *p = beskid_windows_path(path);
  if (!p)
    return 4;
  BOOL ok = DeleteFileW(p);
  DWORD e = GetLastError();
  VirtualFree(p, 0, MEM_RELEASE);
  return ok ? 0 : beskid_windows_fs_status(e);
}
