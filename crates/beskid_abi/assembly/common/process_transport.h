/* Canonical child OS transport. No managed pointer survives a call; readiness
 * never schedules fibers. The Beskid owner owns cancellation and deadlines. */
#ifndef BESKID_PROCESS_TRANSPORT_H
#define BESKID_PROCESS_TRANSPORT_H
#include "environment_lock.h"
#include <errno.h>
#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <wchar.h>
#ifdef _WIN32
#include <windows.h>
#else
#include <fcntl.h>
#include <pthread.h>
#include <signal.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>
#endif
extern void *beskid_rt_v5_intrinsic_system_allocate(size_t, size_t);
extern void beskid_rt_v5_intrinsic_system_free(void *, size_t);
#define CHILD_CAPACITY 128
#define CHILD_ARGUMENTS 256
#define CHILD_BYTES 1048576
struct ChildText {
  char *data;
  size_t length;
};
struct ChildTransport {
  uint64_t token;
  int state, exit_code, terminating, closing, exec_failed;
  size_t argc, envc, total;
  struct ChildText executable, cwd, args[CHILD_ARGUMENTS], env[CHILD_ARGUMENTS];
  int inherit;
#ifdef _WIN32
  HANDLE process, job, pipes[3], write_event;
  OVERLAPPED write_operation;
  uint8_t *write_bytes;
  size_t write_length;
#else
  pid_t pid;
  int pipes[3], exec_pipe;
#endif
};
static struct ChildTransport child_slots[CHILD_CAPACITY];
static uint64_t child_issuer;
#ifdef _WIN32
static SRWLOCK child_lock = SRWLOCK_INIT;
#define CHILD_LOCK() AcquireSRWLockExclusive(&child_lock)
#define CHILD_UNLOCK() ReleaseSRWLockExclusive(&child_lock)
#else
static pthread_mutex_t child_lock = PTHREAD_MUTEX_INITIALIZER;
#define CHILD_LOCK() pthread_mutex_lock(&child_lock)
#define CHILD_UNLOCK() pthread_mutex_unlock(&child_lock)
#endif
static void *ChildAllocate(size_t n) {
  void *p = beskid_rt_v5_intrinsic_system_allocate(n, 8);
  if (p)
    memset(p, 0, n);
  return p;
}
static void ChildFree(void *p, size_t n) {
  if (p)
    beskid_rt_v5_intrinsic_system_free(p, n);
}
#ifdef _WIN32
/* Only after completion, cancellation completion, or before submission. */
static void ChildReleaseWrite(struct ChildTransport *c) {
  ChildFree(c->write_bytes, c->write_length);
  c->write_bytes = NULL;
  c->write_length = 0;
  if (c->write_event)
    CloseHandle(c->write_event);
  c->write_event = NULL;
  memset(&c->write_operation, 0, sizeof c->write_operation);
}
#endif
static int ChildUtf8(const uint8_t *s, size_t n) {
  if (n && !s)
    return 0;
  for (size_t i = 0; i < n;) {
    uint32_t c = s[i++];
    if (!c)
      return 0;
    if (c < 128)
      continue;
    unsigned more;
    uint32_t minimum;
    if (c >= 0xc2 && c <= 0xdf) {
      more = 1;
      minimum = 0x80;
      c &= 31;
    } else if (c >= 0xe0 && c <= 0xef) {
      more = 2;
      minimum = 0x800;
      c &= 15;
    } else if (c >= 0xf0 && c <= 0xf4) {
      more = 3;
      minimum = 0x10000;
      c &= 7;
    } else
      return 0;
    if (more > n - i)
      return 0;
    while (more--) {
      uint8_t b = s[i++];
      if ((b & 0xc0) != 0x80)
        return 0;
      c = (c << 6) | (b & 63);
    }
    if (c < minimum || c > 0x10ffff || (c >= 0xd800 && c <= 0xdfff))
      return 0;
  }
  return 1;
}
static struct ChildTransport *ChildFind(uint64_t token) {
  if (!token)
    return NULL;
  for (size_t i = 0; i < CHILD_CAPACITY; i++)
    if (child_slots[i].token == token)
      return &child_slots[i];
  return NULL;
}
static int ChildCopy(struct ChildTransport *c, struct ChildText *out,
                     const uint8_t *bytes, size_t n) {
  if (n >= CHILD_BYTES || c->total > CHILD_BYTES - n - 1 ||
      !ChildUtf8(bytes, n))
    return -1;
  char *copy = ChildAllocate(n + 1);
  if (!copy)
    return -1;
  if (n)
    memcpy(copy, bytes, n);
  out->data = copy;
  out->length = n;
  c->total += n + 1;
  return 0;
}
static void ChildTextClear(struct ChildText *t) {
  ChildFree(t->data, t->length + 1);
  memset(t, 0, sizeof *t);
}
static void ChildConfigurationClear(struct ChildTransport *c) {
  ChildTextClear(&c->executable);
  ChildTextClear(&c->cwd);
  for (size_t i = 0; i < c->argc; i++)
    ChildTextClear(&c->args[i]);
  for (size_t i = 0; i < c->envc; i++)
    ChildTextClear(&c->env[i]);
  c->argc = c->envc = c->total = 0;
}
uint64_t beskid_rt_v5_intrinsic_child_begin(const uint8_t *exe, size_t n,
                                            const uint8_t *cwd, size_t cn,
                                            int32_t inherit) {
  if (!n || (inherit != 0 && inherit != 1))
    return 0;
  CHILD_LOCK();
  struct ChildTransport *c = NULL;
  for (size_t i = 0; i < CHILD_CAPACITY; i++)
    if (!child_slots[i].token) {
      c = &child_slots[i];
      break;
    }
  if (!c || child_issuer == UINT64_MAX) {
    CHILD_UNLOCK();
    return 0;
  }
  memset(c, 0, sizeof *c);
#ifndef _WIN32
  c->pipes[0] = c->pipes[1] = c->pipes[2] = -1;
  c->exec_pipe = -1;
#endif
  c->inherit = inherit;
  c->state = 1;
  if (ChildCopy(c, &c->executable, exe, n) || ChildCopy(c, &c->cwd, cwd, cn)) {
    ChildConfigurationClear(c);
    memset(c, 0, sizeof *c);
    CHILD_UNLOCK();
    return 0;
  }
  c->token = ++child_issuer;
  uint64_t token = c->token;
  CHILD_UNLOCK();
  return token;
}
int32_t beskid_rt_v5_intrinsic_child_argument(uint64_t token,
                                              const uint8_t *value, size_t n) {
  CHILD_LOCK();
  struct ChildTransport *c = ChildFind(token);
  int result = -1;
  if (c && c->state == 1 && c->argc < CHILD_ARGUMENTS &&
      !ChildCopy(c, &c->args[c->argc], value, n)) {
    c->argc++;
    result = 0;
  }
  CHILD_UNLOCK();
  return result;
}
#ifdef _WIN32
static wchar_t *ChildWide(const char *, size_t, size_t *);
#endif
int32_t beskid_rt_v5_intrinsic_child_environment(uint64_t token,
                                                 const uint8_t *key, size_t kn,
                                                 const uint8_t *value,
                                                 size_t vn) {
  if (!kn || kn > CHILD_BYTES || vn > CHILD_BYTES || !ChildUtf8(key, kn) ||
      !ChildUtf8(value, vn) || memchr(key, '=', kn))
    return -1;
  CHILD_LOCK();
  struct ChildTransport *c = ChildFind(token);
  int result = -1;
  if (c && c->state == 1 && c->envc < CHILD_ARGUMENTS &&
      kn + vn + 2 <= CHILD_BYTES - c->total) {
    size_t n = kn + vn + 1;
    char *p = ChildAllocate(n + 1);
    if (p) {
      memcpy(p, key, kn);
      p[kn] = '=';
      if (vn)
        memcpy(p + kn + 1, value, vn);
      /* Overrides are unique, preventing platform-dependent duplicate
       * semantics. */
      result = 0;
      for (size_t i = 0; i < c->envc; i++) {
#ifdef _WIN32
        const char *equal = memchr(c->env[i].data, '=', c->env[i].length);
        size_t old_bytes = 0, new_bytes = 0;
        wchar_t *old_key = ChildWide(
            c->env[i].data, (size_t)(equal - c->env[i].data), &old_bytes);
        wchar_t *new_key = ChildWide((const char *)key, kn, &new_bytes);
        if (!old_key || !new_key ||
            CompareStringOrdinal(old_key, -1, new_key, -1, TRUE) == CSTR_EQUAL)
          result = -1;
        ChildFree(old_key, old_bytes);
        ChildFree(new_key, new_bytes);
#else
        if (c->env[i].length > kn && c->env[i].data[kn] == '=' &&
            !memcmp(c->env[i].data, key, kn))
          result = -1;
#endif
      }
      if (!result) {
        c->env[c->envc++] = (struct ChildText){p, n};
        c->total += n + 1;
      } else
        ChildFree(p, n + 1);
    }
  }
  CHILD_UNLOCK();
  return result;
}
#ifdef _WIN32
static wchar_t *ChildWide(const char *s, size_t n, size_t *allocated) {
  if (n > INT_MAX)
    return NULL;
  int count =
      MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, s, (int)n, NULL, 0);
  if (!count && n)
    return NULL;
  *allocated = ((size_t)count + 1) * sizeof(wchar_t);
  wchar_t *w = ChildAllocate(*allocated);
  if (w && count &&
      !MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, s, (int)n, w,
                           count)) {
    ChildFree(w, *allocated);
    return NULL;
  }
  return w;
}
static int ChildWindowsPipe(HANDLE *parent, HANDLE *child, int input,
                            uint64_t token) {
  SECURITY_ATTRIBUTES security = {sizeof security, NULL, TRUE};
  if (!input) {
    HANDLE read, write;
    if (!CreatePipe(&read, &write, &security, 0))
      return -1;
    SetHandleInformation(read, HANDLE_FLAG_INHERIT, 0);
    *parent = read;
    *child = write;
    return 0;
  }
  wchar_t name[128];
  int count = swprintf(name, 128, L"\\\\.\\pipe\\beskid-child-%lu-%llu",
                       GetCurrentProcessId(), (unsigned long long)token);
  if (count <= 0)
    return -1;
  HANDLE server =
      CreateNamedPipeW(name, PIPE_ACCESS_OUTBOUND | FILE_FLAG_OVERLAPPED,
                       PIPE_TYPE_BYTE | PIPE_WAIT, 1, 65536, 65536, 0, NULL);
  if (server == INVALID_HANDLE_VALUE)
    return -1;
  HANDLE client =
      CreateFileW(name, GENERIC_READ, 0, &security, OPEN_EXISTING, 0, NULL);
  if (client == INVALID_HANDLE_VALUE) {
    CloseHandle(server);
    return -1;
  }
  *parent = server;
  *child = client;
  return 0;
}
static int ChildSpawnPlatform(struct ChildTransport *c) {
  size_t en = 0, cn = 0;
  wchar_t *exe = ChildWide(c->executable.data, c->executable.length, &en);
  wchar_t *cwd = ChildWide(c->cwd.data, c->cwd.length, &cn);
  size_t cap = (c->total * 2 + CHILD_ARGUMENTS * 4 + 8) * sizeof(wchar_t);
  wchar_t *command = ChildAllocate(cap);
  wchar_t *environment = NULL;
  size_t environment_bytes = 0;
  HANDLE child[3] = {NULL, NULL, NULL};
  HANDLE job = NULL;
  void *attributes = NULL;
  SIZE_T attribute_bytes = 0;
  int attributes_initialized = 0;
  int result = -1;
  if (!exe || !cwd || !command)
    goto done;
  size_t used = 0;
  for (size_t a = 0; a <= c->argc; a++) {
    struct ChildText *t = a ? &c->args[a - 1] : &c->executable;
    size_t bytes = 0;
    wchar_t *w = ChildWide(t->data, t->length, &bytes);
    if (!w)
      goto done;
    if (a)
      command[used++] = L' ';
    command[used++] = L'"';
    size_t slashes = 0;
    for (size_t i = 0;; i++) {
      wchar_t ch = w[i];
      if (ch == L'\\') {
        slashes++;
        continue;
      }
      if (ch == L'"' || !ch) {
        size_t copies = slashes * 2 + (ch == L'"');
        while (copies--)
          command[used++] = L'\\';
      } else
        while (slashes--)
          command[used++] = L'\\';
      slashes = 0;
      if (!ch)
        break;
      command[used++] = ch;
    }
    command[used++] = L'"';
    ChildFree(w, bytes);
  }
  command[used] = 0;
  /* Always construct an explicit environment block; overrides replace keys. */
  wchar_t *base = c->inherit ? GetEnvironmentStringsW() : NULL;
  size_t base_count = 0;
  if (base)
    for (wchar_t *p = base; *p; p += wcslen(p) + 1)
      base_count += wcslen(p) + 1;
  environment_bytes =
      (base_count + c->total + CHILD_ARGUMENTS + 2) * sizeof(wchar_t);
  environment = ChildAllocate(environment_bytes);
  if (!environment) {
    if (base)
      FreeEnvironmentStringsW(base);
    goto done;
  }
  size_t at = 0;
  if (base) {
    for (wchar_t *p = base; *p; p += wcslen(p) + 1) {
      int replace = 0;
      wchar_t *equal = wcschr(p + (*p == L'='), L'=');
      size_t key = equal ? (size_t)(equal - p) : wcslen(p);
      for (size_t i = 0; i < c->envc; i++) {
        size_t bytes = 0;
        wchar_t *w = ChildWide(c->env[i].data, c->env[i].length, &bytes);
        if (!w) {
          FreeEnvironmentStringsW(base);
          goto done;
        }
        if (wcslen(w) > key && w[key] == L'=' && !_wcsnicmp(w, p, key))
          replace = 1;
        ChildFree(w, bytes);
      }
      if (!replace) {
        size_t n = wcslen(p) + 1;
        memcpy(environment + at, p, n * sizeof(wchar_t));
        at += n;
      }
    }
    FreeEnvironmentStringsW(base);
  }
  for (size_t i = 0; i < c->envc; i++) {
    size_t bytes = 0;
    wchar_t *w = ChildWide(c->env[i].data, c->env[i].length, &bytes);
    if (!w)
      goto done;
    memcpy(environment + at, w, bytes);
    at += bytes / sizeof(wchar_t);
    ChildFree(w, bytes);
  }
  environment[at] = 0;
  for (int i = 0; i < 3; i++)
    if (ChildWindowsPipe(&c->pipes[i], &child[i], i == 0, c->token))
      goto done;
  job = CreateJobObjectW(NULL, NULL);
  if (!job)
    goto done;
  JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits;
  memset(&limits, 0, sizeof limits);
  limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
  if (!SetInformationJobObject(job, JobObjectExtendedLimitInformation, &limits,
                               sizeof limits))
    goto done;
  InitializeProcThreadAttributeList(NULL, 1, 0, &attribute_bytes);
  attributes = ChildAllocate(attribute_bytes);
  if (!attributes ||
      !InitializeProcThreadAttributeList(attributes, 1, 0, &attribute_bytes))
    goto done;
  attributes_initialized = 1;
  if (!UpdateProcThreadAttribute(attributes, 0,
                                 PROC_THREAD_ATTRIBUTE_HANDLE_LIST, child,
                                 sizeof child, NULL, NULL))
    goto done;
  STARTUPINFOEXW start;
  memset(&start, 0, sizeof start);
  start.StartupInfo.cb = sizeof start;
  start.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
  start.StartupInfo.hStdInput = child[0];
  start.StartupInfo.hStdOutput = child[1];
  start.StartupInfo.hStdError = child[2];
  start.lpAttributeList = attributes;
  PROCESS_INFORMATION process;
  memset(&process, 0, sizeof process);
  if (!CreateProcessW(exe, command, NULL, NULL, TRUE,
                      CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT |
                          EXTENDED_STARTUPINFO_PRESENT,
                      environment, c->cwd.length ? cwd : NULL,
                      &start.StartupInfo, &process)) {
    DWORD error = GetLastError();
    result = (error == ERROR_FILE_NOT_FOUND || error == ERROR_PATH_NOT_FOUND)
                 ? -7
             : error == ERROR_ACCESS_DENIED ? -8
                                            : -1;
    goto done;
  }
  if (!AssignProcessToJobObject(job, process.hProcess) ||
      ResumeThread(process.hThread) == (DWORD)-1) {
    TerminateProcess(process.hProcess, 127);
    WaitForSingleObject(process.hProcess, 5000);
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);
    goto done;
  }
  CloseHandle(process.hThread);
  c->process = process.hProcess;
  c->job = job;
  job = NULL;
  result = -2;
done:
  for (int i = 0; i < 3; i++)
    if (child[i])
      CloseHandle(child[i]);
  if (job)
    CloseHandle(job);
  if (attributes) {
    if (attributes_initialized)
      DeleteProcThreadAttributeList(attributes);
    ChildFree(attributes, attribute_bytes);
  }
  ChildFree(exe, en);
  ChildFree(cwd, cn);
  ChildFree(command, cap);
  ChildFree(environment, environment_bytes);
  return result;
}
#else
#if defined(__GLIBC__)
/* glibc's canonical environment data symbol is `__environ` (declared
   unconditionally by <unistd.h>); `environ` is only a weak alias, and a linked
   shared runtime resolves to `__environ`. Reference the canonical symbol so the
   static and shared x86_64-unknown-linux-gnu kits import exactly the data
   symbol the manifest declares. */
extern char **__environ;
#define BESKID_PROCESS_ENVIRON __environ
#else
extern char **environ;
#define BESKID_PROCESS_ENVIRON environ
#endif
static int ChildPipe(int p[2]) {
  if (pipe(p))
    return -1;
  for (int i = 0; i < 2; i++)
    if (fcntl(p[i], F_SETFD, FD_CLOEXEC) < 0) {
      close(p[0]);
      close(p[1]);
      return -1;
    }
  return 0;
}
static int ChildSpawnPlatform(struct ChildTransport *c) {
  int p[3][2] = {{-1, -1}, {-1, -1}, {-1, -1}}, error_pipe[2] = {-1, -1};
  int result = -1;
  char *args[CHILD_ARGUMENTS + 2];
  args[0] = c->executable.data;
  for (size_t i = 0; i < c->argc; i++)
    args[i + 1] = c->args[i].data;
  args[c->argc + 1] = NULL;
  struct rlimit limit;
  if (getrlimit(RLIMIT_NOFILE, &limit) || limit.rlim_cur > 1048576)
    return -1;
  size_t base = 0, inherited_bytes = 0, copied = 0;
  char **env = NULL;
  size_t bytes = 0, at = 0;
  BESKID_ENVIRONMENT_LOCK();
  if (c->inherit)
    while (BESKID_PROCESS_ENVIRON[base]) {
      if (base >= 65536)
        goto snapshot_failed;
      base++;
    }
  bytes = (base + c->envc + 1) * sizeof(char *);
  env = ChildAllocate(bytes);
  if (!env)
    goto snapshot_failed;
  for (size_t i = 0; i < base; i++) {
    int replace = 0;
    char *equal = strchr(BESKID_PROCESS_ENVIRON[i], '=');
    size_t key = equal ? (size_t)(equal - BESKID_PROCESS_ENVIRON[i]) : strlen(BESKID_PROCESS_ENVIRON[i]);
    for (size_t j = 0; j < c->envc; j++)
      if (c->env[j].length > key && c->env[j].data[key] == '=' &&
          !memcmp(BESKID_PROCESS_ENVIRON[i], c->env[j].data, key))
        replace = 1;
    if (!replace) {
      size_t length = strnlen(BESKID_PROCESS_ENVIRON[i], CHILD_BYTES);
      if (length == CHILD_BYTES || length + 1 > CHILD_BYTES - inherited_bytes)
        goto snapshot_failed;
      env[at] = ChildAllocate(length + 1);
      if (!env[at])
        goto snapshot_failed;
      memcpy(env[at++], BESKID_PROCESS_ENVIRON[i], length + 1);
      copied++;
      inherited_bytes += length + 1;
    }
  }
  BESKID_ENVIRONMENT_UNLOCK();
  for (size_t i = 0; i < c->envc; i++)
    env[at++] = c->env[i].data;
  env[at] = NULL;
  for (int i = 0; i < 3; i++)
    if (ChildPipe(p[i]))
      goto done;
  if (ChildPipe(error_pipe))
    goto done;
  pid_t pid = fork();
  if (pid < 0)
    goto done;
  if (!pid) {
    close(error_pipe[0]);
    int failure = 0;
    if (setpgid(0, 0))
      failure = errno;
    for (int i = 0; i < 3 && !failure; i++)
      if (dup2(p[i][i == 0 ? 0 : 1], i) < 0)
        failure = errno;
    for (int i = 0; i < 3; i++) {
      close(p[i][0]);
      close(p[i][1]);
    }
    if (!failure && dup2(error_pipe[1], 3) < 0)
      failure = errno;
    if (!failure && fcntl(3, F_SETFD, FD_CLOEXEC) < 0)
      failure = errno;
    for (int fd = 4; fd < (int)limit.rlim_cur; fd++)
      close(fd);
    if (!failure && c->cwd.length && chdir(c->cwd.data))
      failure = errno;
    if (!failure)
      execve(c->executable.data, args, env);
    if (!failure)
      failure = errno;
    (void)write(3, &failure, sizeof failure);
    _exit(127);
  }
  c->pid = pid;
  (void)setpgid(pid, pid);
  close(error_pipe[1]);
  error_pipe[1] = -1;
  int error_flags = fcntl(error_pipe[0], F_GETFL, 0);
  if (error_flags < 0 ||
      fcntl(error_pipe[0], F_SETFL, error_flags | O_NONBLOCK) < 0) {
    kill(-pid, SIGKILL);
    while (waitpid(pid, NULL, 0) < 0 && errno == EINTR) {
    }
    c->pid = 0;
    goto done;
  }
  c->exec_pipe = error_pipe[0];
  error_pipe[0] = -1;
  for (int i = 0; i < 3; i++) {
    int parent = i == 0 ? 1 : 0;
    close(p[i][1 - parent]);
    p[i][1 - parent] = -1;
    c->pipes[i] = p[i][parent];
    p[i][parent] = -1;
    int flags = fcntl(c->pipes[i], F_GETFL, 0);
    if (flags < 0 || fcntl(c->pipes[i], F_SETFL, flags | O_NONBLOCK) < 0) {
      kill(-pid, SIGKILL);
      while (waitpid(pid, NULL, 0) < 0 && errno == EINTR) {
      }
      c->pid = 0;
      goto done;
    }
  }
  result = -2;
done:
  for (int i = 0; i < 3; i++)
    for (int j = 0; j < 2; j++)
      if (p[i][j] >= 0)
        close(p[i][j]);
  for (int i = 0; i < 2; i++)
    if (error_pipe[i] >= 0)
      close(error_pipe[i]);
  for (size_t i = 0; i < copied; i++)
    ChildFree(env[i], strlen(env[i]) + 1);
  ChildFree(env, bytes);
  return result;
snapshot_failed:
  BESKID_ENVIRONMENT_UNLOCK();
  goto done;
}
#endif
#ifndef _WIN32
static int ChildExecProbe(struct ChildTransport *c) {
  int failure = 0;
  ssize_t count = read(c->exec_pipe, &failure, sizeof failure);
  if (count < 0 && (errno == EAGAIN || errno == EWOULDBLOCK || errno == EINTR))
    return -2;
  close(c->exec_pipe);
  c->exec_pipe = -1;
  c->state = 2;
  if (count == 0)
    return 0;
  if (count == (ssize_t)sizeof failure && failure != 0) {
    /* The private child setup branch writes this packet then immediately exits.
       No executable ran, so this path owns only the direct child, no
       descendants. */
    c->exec_failed = 1;
    c->state = 6;
    return failure == ENOENT ? -7 : failure == EACCES ? -8 : -1;
  }
  return -1;
}
#endif
int32_t beskid_rt_v5_intrinsic_child_spawn(uint64_t token) {
  CHILD_LOCK();
  struct ChildTransport *c = ChildFind(token);
  int result = -1;
  if (c && c->state == 1) {
    result = ChildSpawnPlatform(c);
    c->state = result == -2 ? 5 : result ? 4 : 2;
    ChildConfigurationClear(c);
  }
#ifndef _WIN32
  else if (c && c->state == 5)
    result = ChildExecProbe(c);
#endif
  CHILD_UNLOCK();
  return result;
}
static int64_t ChildPoll(struct ChildTransport *c) {
  if (c->state == 3)
    return c->exit_code;
  if (c->state != 2 && c->state != 5 && c->state != 6)
    return -1;
#ifdef _WIN32
  DWORD code;
  if (WaitForSingleObject(c->process, 0) == WAIT_TIMEOUT)
    return -2;
  if (!GetExitCodeProcess(c->process, &code))
    return -1;
  c->exit_code = (int)(code & INT_MAX);
#else
  int status;
  pid_t result = waitpid(c->pid, &status, WNOHANG);
  if (!result)
    return -2;
  if (result < 0)
    return errno == EINTR ? -2 : -1;
  c->exit_code =
      WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
#endif
  c->state = 3;
  return c->exit_code;
}
int64_t beskid_rt_v5_intrinsic_child_poll(uint64_t token) {
  CHILD_LOCK();
  struct ChildTransport *c = ChildFind(token);
  int64_t result = c && !c->closing ? ChildPoll(c) : -11;
  CHILD_UNLOCK();
  return result;
}
static int ChildPipeClose(struct ChildTransport *c, int stream) {
  if (stream < 0 || stream > 2)
    return -1;
#ifdef _WIN32
  if (c->pipes[stream]) {
    if (stream == 0 && c->write_bytes) {
      CancelIoEx(c->pipes[0], &c->write_operation);
      DWORD count;
      if (!GetOverlappedResult(c->pipes[0], &c->write_operation, &count,
                               FALSE) &&
          GetLastError() == ERROR_IO_INCOMPLETE)
        return -2;
      ChildReleaseWrite(c);
    }
    CloseHandle(c->pipes[stream]);
    c->pipes[stream] = NULL;
  }
#else
  if (c->pipes[stream] >= 0) {
    close(c->pipes[stream]);
    c->pipes[stream] = -1;
  }
#endif
  return 0;
}
int32_t beskid_rt_v5_intrinsic_child_close_pipe(uint64_t token,
                                                int32_t stream) {
  CHILD_LOCK();
  struct ChildTransport *c = ChildFind(token);
  int result = c ? ChildPipeClose(c, stream) : 0;
  CHILD_UNLOCK();
  return result;
}
int64_t beskid_rt_v5_intrinsic_child_read(uint64_t token, int32_t stream,
                                          uint8_t *bytes, size_t n) {
  if ((stream != 1 && stream != 2) || (!bytes && n) || n > INT_MAX)
    return -1;
  if (!n)
    return 0;
  CHILD_LOCK();
  struct ChildTransport *c = ChildFind(token);
  int64_t result = -11;
  if (c && !c->closing && (c->state == 2 || c->state == 3)) {
#ifdef _WIN32
    if (c->pipes[stream]) {
      result = -1;
      DWORD available = 0, count = 0;
      if (!PeekNamedPipe(c->pipes[stream], NULL, 0, NULL, &available, NULL))
        result = GetLastError() == ERROR_BROKEN_PIPE ? 0 : -1;
      else if (!available)
        result = -2;
      else if (ReadFile(c->pipes[stream], bytes,
                        (DWORD)(n < available ? n : available), &count, NULL))
        result = count;
    }
#else
    if (c->pipes[stream] >= 0) {
      ssize_t count = read(c->pipes[stream], bytes, n);
      result = count < 0 && (errno == EAGAIN || errno == EWOULDBLOCK ||
                             errno == EINTR)
                   ? -2
                   : count;
    }
#endif
  }
  CHILD_UNLOCK();
  return result;
}
int64_t beskid_rt_v5_intrinsic_child_write(uint64_t token, const uint8_t *bytes,
                                           size_t n) {
  if ((!bytes && n) || n > INT_MAX)
    return -1;
  if (!n)
    return 0;
  CHILD_LOCK();
  struct ChildTransport *c = ChildFind(token);
  int64_t result = -11;
  if (c && !c->closing && c->state == 2) {
#ifdef _WIN32
    if (c->pipes[0]) {
      result = -1;
      DWORD count = 0;
      if (c->write_bytes) {
        if (n < c->write_length ||
            memcmp(bytes, c->write_bytes, c->write_length)) {
          CHILD_UNLOCK();
          return -6;
        }
        if (GetOverlappedResult(c->pipes[0], &c->write_operation, &count,
                                FALSE)) {
          result = count;
          ChildReleaseWrite(c);
        } else {
          DWORD failure = GetLastError();
          result = failure == ERROR_IO_INCOMPLETE ? -2
                   : failure == ERROR_BROKEN_PIPE ? -11
                                                  : -1;
          if (result != -2)
            ChildReleaseWrite(c);
        }
      } else {
        size_t amount = n < 65536 ? n : 65536;
        c->write_length = amount;
        c->write_bytes = ChildAllocate(amount);
        c->write_event = CreateEventW(NULL, TRUE, FALSE, NULL);
        if (c->write_bytes && c->write_event) {
          memcpy(c->write_bytes, bytes, amount);
          memset(&c->write_operation, 0, sizeof c->write_operation);
          c->write_operation.hEvent = c->write_event;
          if (WriteFile(c->pipes[0], c->write_bytes, (DWORD)amount, &count,
                        &c->write_operation)) {
            result = count;
            ChildReleaseWrite(c);
          } else {
            DWORD failure = GetLastError();
            result = failure == ERROR_IO_PENDING    ? -2
                     : failure == ERROR_BROKEN_PIPE ? -11
                                                    : -1;
            if (result != -2)
              ChildReleaseWrite(c);
          }
        } else {
          ChildReleaseWrite(c);
          result = -1;
        }
      }
    }
#else
    if (c->pipes[0] >= 0) { /* Block SIGPIPE only for this write; consume our
                               new pending signal. */
      sigset_t set, old, pending;
      sigemptyset(&set);
      sigaddset(&set, SIGPIPE);
      pthread_sigmask(SIG_BLOCK, &set, &old);
      sigpending(&pending);
      int existing = sigismember(&pending, SIGPIPE);
      ssize_t count = write(c->pipes[0], bytes, n);
      int error = errno;
      if (count < 0 && error == EPIPE && !existing) {
        sigpending(&pending);
        if (sigismember(&pending, SIGPIPE)) {
          int signal_number;
          (void)sigwait(&set, &signal_number);
        }
      }
      pthread_sigmask(SIG_SETMASK, &old, NULL);
      result = count < 0 && (error == EAGAIN || error == EWOULDBLOCK ||
                             error == EINTR)
                   ? -2
               : count < 0 && error == EPIPE ? -11
                                             : count;
    }
#endif
  }
  CHILD_UNLOCK();
  return result;
}
static int ChildTerminate(struct ChildTransport *c) {
  if ((c->state != 2 && c->state != 5) || c->terminating)
    return 0;
#ifdef _WIN32
  if (!TerminateJobObject(c->job, 137))
    return -1;
  c->terminating = 1;
  return 0;
#else
  if (kill(-c->pid, SIGKILL) != 0) {
    if (errno != ESRCH)
      return -1;
    /* Setup cancellation may precede setpgid. The unreaped direct child still
       owns this PID; kill it before it can establish or escape the group. */
    if (kill(c->pid, SIGKILL) != 0 && errno != ESRCH)
      return -1;
  }
  c->terminating = 1;
  return 0;
#endif
}
int32_t beskid_rt_v5_intrinsic_child_terminate(uint64_t token) {
  CHILD_LOCK();
  struct ChildTransport *c = ChildFind(token);
  int result = c ? ChildTerminate(c) : 0;
  CHILD_UNLOCK();
  return result;
}
int32_t beskid_rt_v5_intrinsic_child_close(uint64_t token) {
  CHILD_LOCK();
  struct ChildTransport *c = ChildFind(token);
  if (!c) {
    CHILD_UNLOCK();
    return 0;
  }
  c->closing = 1;
  if (c->state == 2 || c->state == 5) {
    if (ChildTerminate(c)) {
      CHILD_UNLOCK();
      return -1;
    }
  }
  if (c->state == 2 || c->state == 5 || c->state == 6) {
    int64_t exit = ChildPoll(c);
    if (exit < 0) {
      CHILD_UNLOCK();
      return (int32_t)exit;
    }
  }
  for (int i = 0; i < 3; i++) {
    int closed = ChildPipeClose(c, i);
    if (closed) {
      CHILD_UNLOCK();
      return closed;
    }
  }
#ifdef _WIN32
  if (c->job)
    CloseHandle(c->job);
  if (c->process)
    CloseHandle(c->process);
#else
  if (c->exec_pipe >= 0)
    close(c->exec_pipe);
  /* The direct child may have exited while descendants still hold pipe ends. */
  if (c->pid > 0 && !c->exec_failed)
    (void)kill(-c->pid, SIGKILL);
#endif
  ChildConfigurationClear(c);
  memset(c, 0, sizeof *c);
  CHILD_UNLOCK();
  return 0;
}
int32_t beskid_rt_v5_intrinsic_child_shutdown(void) {
  uint64_t tokens[CHILD_CAPACITY];
  size_t count = 0;
  CHILD_LOCK();
  for (size_t i = 0; i < CHILD_CAPACITY; i++)
    if (child_slots[i].token)
      tokens[count++] = child_slots[i].token;
  CHILD_UNLOCK();
  int32_t result = 0;
  for (size_t i = 0; i < count; i++) {
    int32_t closed = beskid_rt_v5_intrinsic_child_close(tokens[i]);
    if (closed == -1)
      return -1;
    if (closed == -2)
      result = -2;
  }
  return result;
}
#endif
