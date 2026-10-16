#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

/*
 * GC stress, verification, poisoning and quarantine (v0.6.1 `v061-gc-stress-and-poison`).
 * The runtime reads `BESKID_GC_STRESS` and `BESKID_GC_QUARANTINE` itself in
 * `beskid_rt_v5_process_init`; the test sets them in this child's environment.
 *
 *   profile        prints `kit-profile=<0|1>`, the build profile the kit reports.
 *   missing-root   holds a string in this C frame without a root while it allocates, then passes
 *                  it to `str_concat`. With `alloc` stress the allocation collects first and
 *                  `str_concat` traps; without the variable nothing collects and it succeeds.
 *   verify-corrupt writes the poison pattern into the header word of an allocated, unrooted
 *                  string record, then collects. With `verify` the verifier traps before the collection;
 *                  without it the sweep frees the string and the fixture succeeds.
 *   quarantine     collects a string, allocates more strings of the same size class (far below
 *                  the quarantine budget), then passes the collected string to `str_len`. The
 *                  debug kit keeps the freed slot in quarantine, so `str_len` traps.
 *   init           only initializes and shuts down (malformed-variable cases trap in init).
 * Returning from a call that must trap prints what it returned and exits 0.
 */

extern uint8_t beskid_rt_v5_intrinsic_kit_debug_profile(void);
extern uint8_t gc_heap_verify(void);

#define POISON_WORD UINT64_C(0xDEADBEEFFEEDFACE)

static int missing_root(void) {
    void *unrooted = str_new((void *)"unrooted", 8);
    assert(unrooted);
    void *other = str_new((void *)" other", 6);
    assert(other);
    size_t other_root = gc_root_handle(other);
    assert(other_root);
    void *joined = str_concat(unrooted, other);
    printf("joined=%zu\n", joined ? str_len(joined) : (size_t)0);
    gc_unroot_handle(other_root);
    return 0;
}

static int verify_corrupt(void) {
    /* The record itself, not the string view (`str_new` returns the record's `data` field at
     * record + 24): the poison must land in the object's header word, where the verifier looks. */
    void *victim = beskid_rt_v5_utf8_record_construct((void *)"victim", 6);
    assert(victim);
    *(volatile uint64_t *)victim = POISON_WORD;
    gc_collect();
    printf("collected\n");
    return 0;
}

static int quarantine(void) {
    void *collected = str_new((void *)"collected", 9);
    assert(collected);
    gc_collect();
    for (int index = 0; index < 64; ++index) {
        void *fresh = str_new((void *)"refilled!", 9);
        assert(fresh);
    }
    printf("length=%zu\n", str_len(collected));
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        return 2;
    }
    _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
    if (strcmp(argv[1], "profile") == 0) {
        printf("kit-profile=%u\n", (unsigned)beskid_rt_v5_intrinsic_kit_debug_profile());
        return 0;
    }
    assert(beskid_rt_v5_process_init(runtime) == runtime);
    int status = 2;
    if (strcmp(argv[1], "init") == 0) {
        status = 0;
    } else if (strcmp(argv[1], "missing-root") == 0) {
        status = missing_root();
    } else if (strcmp(argv[1], "verify-corrupt") == 0) {
        status = verify_corrupt();
    } else if (strcmp(argv[1], "quarantine") == 0) {
        status = quarantine();
    } else {
        fprintf(stderr, "unknown mode %s\n", argv[1]);
        return 2;
    }
    if (status == 0) {
        assert(gc_heap_verify() != 0);
        beskid_rt_v5_process_shutdown(runtime);
    }
    return status;
}
