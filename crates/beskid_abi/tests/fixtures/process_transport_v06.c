#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <io.h>
#include <fcntl.h>
#else
#define _DARWIN_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#include <unistd.h>
#include <time.h>
#endif
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
void *beskid_rt_v5_intrinsic_system_allocate(size_t n,size_t alignment) {(void)alignment;return calloc(1,n);}
void beskid_rt_v5_intrinsic_system_free(void *p,size_t n) {(void)n;free(p);}
#include "../../assembly/common/process_transport.h"

static void pause_probe(void) {
#ifdef _WIN32
  Sleep(1);
#else
  struct timespec delay = {0, 1000000}; nanosleep(&delay, NULL);
#endif
}
static int64_t now_ms(void) {
#ifdef _WIN32
  return (int64_t)GetTickCount64();
#else
  struct timespec now; clock_gettime(CLOCK_MONOTONIC, &now);
  return (int64_t)now.tv_sec * 1000 + now.tv_nsec / 1000000;
#endif
}
static int32_t spawn_ready(uint64_t token) {
  int64_t deadline = now_ms() + 5000;
  int32_t status;
  do {
    status = beskid_rt_v5_intrinsic_child_spawn(token);
    if (status == -2) pause_probe();
  } while (status == -2 && now_ms() < deadline);
  assert(status != -2);
  return status;
}
static int32_t close_ready(uint64_t token) {
  int64_t deadline = now_ms() + 5000;
  int32_t status;
  do {
    status = beskid_rt_v5_intrinsic_child_close(token);
    if (status == -2) pause_probe();
  } while (status == -2 && now_ms() < deadline);
  if (status != 0) fprintf(stderr, "close token=%llu status=%d errno=%d\n", (unsigned long long)token, status, errno);
  assert(status != -2);
  return status;
}
static void current_directory(char *path, size_t capacity) {
#ifdef _WIN32
  wchar_t wide[4096]; DWORD length = GetCurrentDirectoryW(4096, wide);
  assert(length > 0 && length < 4096);
  assert(WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide, -1, path, (int)capacity, NULL, NULL) > 0);
#else
  assert(getcwd(path, capacity));
#endif
}
static void absolute_executable(const char *input, char *path, size_t capacity) {
#ifdef _WIN32
  (void)input;
  wchar_t wide[4096]; DWORD length = GetModuleFileNameW(NULL, wide, 4096);
  assert(length > 0 && length < 4096);
  assert(WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide, -1, path, (int)capacity, NULL, NULL) > 0);
#else
  if (input[0] == '/') { assert(strlen(input) < capacity); strcpy(path, input); }
  else { current_directory(path, capacity); size_t length = strlen(path); assert(length + strlen(input) + 2 <= capacity); path[length] = '/'; strcpy(path + length + 1, input); }
#endif
}
static void environment_and_cwd(const char *executable) {
  char directory[4096];
#ifdef _WIN32
  wchar_t temporary[4096], wide_directory[4096]; DWORD length = GetTempPathW(4096, temporary);
  assert(length > 0 && length < 4096);
  assert(GetTempFileNameW(temporary, L"bsk", 0, wide_directory));
  assert(DeleteFileW(wide_directory) && CreateDirectoryW(wide_directory, NULL));
  assert(WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide_directory, -1, directory, sizeof directory, NULL, NULL) > 0);
  assert(SetEnvironmentVariableW(L"BESKID_PROCESS_PARENT_ONLY", L"parent"));
#else
  strcpy(directory, "/tmp/beskid process.XXXXXX"); assert(mkdtemp(directory));
  char original[4096]; current_directory(original, sizeof original);
  assert(!chdir(directory)); current_directory(directory, sizeof directory); assert(!chdir(original));
  assert(!setenv("BESKID_PROCESS_PARENT_ONLY", "parent", 1));
#endif
  const char *value = "spaces and unicode \xc5\xbc";
  uint64_t token = beskid_rt_v5_intrinsic_child_begin((const uint8_t *)executable, strlen(executable), (const uint8_t *)directory, strlen(directory), 0);
  assert(token && !beskid_rt_v5_intrinsic_child_argument(token, (const uint8_t *)"--environment", 13));
  assert(!beskid_rt_v5_intrinsic_child_argument(token, (const uint8_t *)directory, strlen(directory)));
  assert(!beskid_rt_v5_intrinsic_child_environment(token, (const uint8_t *)"BESKID_PROCESS_VALUE", 20, (const uint8_t *)value, strlen(value)));
  assert(!beskid_rt_v5_intrinsic_child_environment(token, (const uint8_t *)"BESKID_PROCESS_CWD", 18, (const uint8_t *)directory, strlen(directory)));
  assert(beskid_rt_v5_intrinsic_child_environment(token, (const uint8_t *)"BESKID_PROCESS_VALUE", 20, (const uint8_t *)"duplicate", 9) < 0);
  assert(!spawn_ready(token));
  int64_t deadline = now_ms() + 5000, status;
  do { status = beskid_rt_v5_intrinsic_child_poll(token); if (status == -2) pause_probe(); } while (status == -2 && now_ms() < deadline);
  assert(status == 19 && !beskid_rt_v5_intrinsic_child_close(token));
#ifdef _WIN32
  assert(RemoveDirectoryW(wide_directory));
#else
  assert(!rmdir(directory));
#endif
}
int main(int argc, char **argv) {
  if (argc > 1 && !strcmp(argv[1], "--sleep")) { for (;;) pause_probe(); }
  if (argc > 1 && !strcmp(argv[1], "--environment")) {
    char directory[4096]; current_directory(directory, sizeof directory);
    assert(argc == 3);
#ifdef _WIN32
    char expected[4096]; const wchar_t *cwd = _wgetenv(L"BESKID_PROCESS_CWD");
    assert(cwd && WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, cwd, -1, expected, sizeof expected, NULL, NULL) > 0);
    assert(!strcmp(directory, expected));
    assert(_wgetenv(L"BESKID_PROCESS_PARENT_ONLY") == NULL);
    const wchar_t *value = _wgetenv(L"BESKID_PROCESS_VALUE");
    assert(value && !wcscmp(value, L"spaces and unicode \x017c")); return 19;
#else
    assert(!strcmp(directory, argv[2]));
    assert(getenv("BESKID_PROCESS_PARENT_ONLY") == NULL);
    const char *value = getenv("BESKID_PROCESS_VALUE");
    assert(value && !strcmp(value, "spaces and unicode \xc5\xbc")); return 19;
#endif
  }
  if (argc > 1 && !strcmp(argv[1], "--child")) {
    assert(argc == 3);
#ifdef _WIN32
    assert(wcsstr(GetCommandLineW(), L"\"spaces and unicode \x017c\""));
    _setmode(0, _O_BINARY); _setmode(1, _O_BINARY); _setmode(2, _O_BINARY);
#else
    assert(!strcmp(argv[2], "spaces and unicode \xc5\xbc"));
#endif
    uint8_t data[5]; assert(fread(data, 1, 5, stdin) == 5);
    for (size_t i = 0; i < 131072; ++i) assert(fputc('e', stderr) != EOF);
    assert(fwrite(data, 1, 5, stdout) == 5); return 7;
  }
  uint64_t child = beskid_rt_v5_intrinsic_child_begin((const uint8_t *)argv[0], strlen(argv[0]), NULL, 0, 1);
  assert(child);
  assert(!beskid_rt_v5_intrinsic_child_argument(child, (const uint8_t *)"--child", 7));
  const char *argument = "spaces and unicode \xc5\xbc";
  assert(!beskid_rt_v5_intrinsic_child_argument(child, (const uint8_t *)argument, strlen(argument)));
  assert(!spawn_ready(child));
  const uint8_t input[5] = {0, 255, 13, 10, 42};
  size_t written = 0, read_count = 0, errors = 0; uint8_t actual[5], buffer[4096];
  int64_t deadline = now_ms() + 5000, status = -2;
  while (now_ms() < deadline && (status == -2 || read_count < 5 || errors < 131072)) {
    if (written < 5) { int64_t n = beskid_rt_v5_intrinsic_child_write(child, input + written, 5-written); assert(n >= 0 || n == -2); if(n > 0) written += (size_t)n; }
    else assert(!beskid_rt_v5_intrinsic_child_close_pipe(child, 0));
    if(read_count < 5) { int64_t n = beskid_rt_v5_intrinsic_child_read(child, 1, actual+read_count, 5-read_count); assert(n >= 0 || n == -2); if(n > 0) read_count += (size_t)n; }
    int64_t n = beskid_rt_v5_intrinsic_child_read(child, 2, buffer, sizeof buffer); assert(n >= 0 || n == -2); if(n > 0) errors += (size_t)n;
    status = beskid_rt_v5_intrinsic_child_poll(child); pause_probe();
  }
  assert(status == 7 && read_count == 5 && errors == 131072 && !memcmp(input, actual, 5));
  assert(!beskid_rt_v5_intrinsic_child_close(child));
  assert(!beskid_rt_v5_intrinsic_child_close(child));
  uint64_t invalid = beskid_rt_v5_intrinsic_child_begin((const uint8_t *)"/missing/beskid-child", 21, NULL, 0, 0);
  assert(invalid && spawn_ready(invalid) == -7);
  assert(!close_ready(invalid));
  uint64_t running = beskid_rt_v5_intrinsic_child_begin((const uint8_t *)argv[0], strlen(argv[0]), NULL, 0, 1);
  assert(running && !beskid_rt_v5_intrinsic_child_argument(running, (const uint8_t *)"--sleep", 7));
  assert(!spawn_ready(running));
  int64_t close_started = now_ms();
  assert(!beskid_rt_v5_intrinsic_child_terminate(running));
  int close_status;
  do { close_status = beskid_rt_v5_intrinsic_child_close(running); if (close_status == -2) pause_probe(); } while(close_status == -2 && now_ms()-close_started < 5000);
  if(close_status != 0 || now_ms()-close_started >= 5000) fprintf(stderr,"close status=%d elapsed=%lld errno=%d\n",close_status,(long long)(now_ms()-close_started),errno);
  assert(close_status == 0 && now_ms()-close_started < 5000);
  assert(!beskid_rt_v5_intrinsic_child_close(running));
  uint8_t discarded;
  assert(beskid_rt_v5_intrinsic_child_poll(running) == -11);
  assert(beskid_rt_v5_intrinsic_child_read(running, 1, &discarded, 1) == -11);
  assert(beskid_rt_v5_intrinsic_child_write(running, input, 1) == -11);
  char executable[4096]; absolute_executable(argv[0], executable, sizeof executable);
  environment_and_cwd(executable);
  uint64_t replacement = beskid_rt_v5_intrinsic_child_begin((const uint8_t *)executable, strlen(executable), NULL, 0, 1);
  assert(replacement && replacement != running);
  assert(beskid_rt_v5_intrinsic_child_argument(running, (const uint8_t *)"stale", 5) < 0);
  assert(beskid_rt_v5_intrinsic_child_argument(UINT64_MAX, (const uint8_t *)"forged", 6) < 0);
  assert(!beskid_rt_v5_intrinsic_child_close(running));
  assert(!beskid_rt_v5_intrinsic_child_argument(replacement, (const uint8_t *)"--sleep", 7));
  assert(!spawn_ready(replacement));
  deadline = now_ms() + 5000; int shutdown;
  do { shutdown = beskid_rt_v5_intrinsic_child_shutdown(); if (shutdown == -2) pause_probe(); } while (shutdown == -2 && now_ms() < deadline);
  assert(!shutdown && now_ms() < deadline);
  assert(beskid_rt_v5_intrinsic_child_poll(replacement) < 0);
  assert(!beskid_rt_v5_intrinsic_child_shutdown());
  uint64_t setup = beskid_rt_v5_intrinsic_child_begin((const uint8_t *)executable, strlen(executable), NULL, 0, 1);
  assert(setup && !beskid_rt_v5_intrinsic_child_argument(setup, (const uint8_t *)"--sleep", 7));
  int32_t setup_status = beskid_rt_v5_intrinsic_child_spawn(setup);
  assert(setup_status == 0 || setup_status == -2);
  assert(!close_ready(setup));
  assert(beskid_rt_v5_intrinsic_child_poll(setup) == -11);
  puts("binary=exact argv=exact stderr=drained reap=bounded environment=isolated cwd=exact stale=denied shutdown=bounded"); return 0;
}
