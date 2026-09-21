#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <errno.h>
#include <inttypes.h>
#include <stdio.h>
#include <string.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#include <stdlib.h>
#define FIXTURE_EXPORT __declspec(dllexport)
#define FixtureClose _close
#define FixtureRead _read
#define FixtureWrite _write
static void FixtureInvalidParameter(const wchar_t *expression, const wchar_t *function,
                                    const wchar_t *file, unsigned int line, uintptr_t reserved) {
    (void)expression; (void)function; (void)file; (void)line; (void)reserved;
}
static int FixtureSave(int fd) {
    /* Restore the host handler after this producer-only probe, before source execution. */
    _invalid_parameter_handler previous = _set_thread_local_invalid_parameter_handler(FixtureInvalidParameter);
    int saved = _dup(fd);
    int error = errno;
    _set_thread_local_invalid_parameter_handler(previous);
    assert(saved >= 0 || error == EBADF);
    return saved;
}
static void FixturePipe(int descriptors[2]) {
    assert(_pipe(descriptors, 4096, _O_BINARY) == 0);
}
static void FixtureDuplicate(int source, int target) {
    assert(_dup2(source, target) == 0);
}
#else
#include <unistd.h>
#define FIXTURE_EXPORT
#define FixtureClose close
#define FixtureRead read
#define FixtureWrite write
static int FixtureSave(int fd) {
    int saved = dup(fd);
    assert(saved >= 0 || errno == EBADF);
    return saved;
}
static void FixturePipe(int descriptors[2]) { assert(pipe(descriptors) == 0); }
static void FixtureDuplicate(int source, int target) { assert(dup2(source, target) == target); }
#endif

/* Shared producer mechanics, not transfer policy: 1 = short input,
   2 = binary corpus with text-mode caller descriptors, 3 = closed input. */
FIXTURE_EXPORT int64_t RunWithFoundationInput(int64_t (*run)(void), int mode) {
    if (mode == 0) return run();
    const unsigned char prefix[] = {0, 255, 42};
    const unsigned char corpus[] = {13, 10, 10, 26, 0, 255, 192, 175, 42};
    int savedInput = FixtureSave(198);
    int savedOutput = mode == 2 ? FixtureSave(199) : -1;
    int output[2] = {-1, -1};
    if (mode == 3) {
        if (savedInput >= 0) assert(FixtureClose(198) == 0);
    } else {
        int input[2];
        FixturePipe(input);
        assert(input[0] != 198 && input[1] != 198);
        const unsigned char *payload = mode == 2 ? corpus : prefix;
        unsigned int length = mode == 2 ? sizeof(corpus) : sizeof(prefix);
        assert(FixtureWrite(input[1], payload, length) == (int)length);
        FixtureDuplicate(input[0], 198);
        assert(FixtureClose(input[0]) == 0);
        assert(FixtureClose(input[1]) == 0);
        if (mode == 2) {
            FixturePipe(output);
            assert(output[0] != 199 && output[1] != 199);
            FixtureDuplicate(output[1], 199);
            assert(FixtureClose(output[1]) == 0);
#ifdef _WIN32
            assert(_setmode(198, _O_TEXT) == _O_BINARY);
            assert(_setmode(199, _O_TEXT) == _O_BINARY);
#endif
        }
    }
    int64_t result = run();
    if (mode == 2) {
#ifdef _WIN32
        int inputMode = _setmode(198, _O_BINARY);
        int outputMode = _setmode(199, _O_BINARY);
        if (inputMode != _O_TEXT || outputMode != _O_TEXT) result = -901;
#endif
        assert(FixtureClose(199) == 0);
        unsigned char actual[32] = {0};
        int count = (int)FixtureRead(output[0], actual, sizeof(actual));
        if (result == 42 && (count != (int)sizeof(corpus) || memcmp(actual, corpus, sizeof(corpus)) != 0)) result = -902;
        assert(FixtureClose(output[0]) == 0);
        if (savedOutput >= 0) {
            FixtureDuplicate(savedOutput, 199);
            assert(FixtureClose(savedOutput) == 0);
        }
    }
    if (mode != 3) assert(FixtureClose(198) == 0);
    if (savedInput >= 0) {
        FixtureDuplicate(savedInput, 198);
        assert(FixtureClose(savedInput) == 0);
    }
    return result;
}

#ifndef FOUNDATION_PRODUCER_ONLY
extern int64_t RunFoundationFixture(void) __asm__(FOUNDATION_FIXTURE_SYMBOL);
int main(void) {
    _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
    assert(beskid_rt_v5_process_init(runtime) == runtime);
    int64_t result = RunWithFoundationInput(RunFoundationFixture, SYSCALL_INPUT);
    fprintf(stderr, "foundation source result=%" PRId64 " expected=%" PRId64 "\n", result, (int64_t)FOUNDATION_EXPECTED);
    beskid_rt_v5_process_shutdown(runtime);
    return result == (int64_t)FOUNDATION_EXPECTED ? 0 : 1;
}
#endif
