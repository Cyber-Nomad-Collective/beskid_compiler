#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <stdlib.h>

/*
 * Self-contained C host, following `fiber_value_transfer.c`'s pattern (a hand-rolled `main`,
 * not the standard `executable_bootstrap.c` path, so `BESKID_HEAP_MAX_BYTES`'s decimal/K/M/G
 * string parsing is NOT exercised here — that parser lives in
 * `crates/beskid_abi/assembly/common/executable_bootstrap.c` and is unchanged by this design;
 * this host tests the cap-enforcement CONTRACT (a request over the cap traps diagnosably,
 * never crashes) by calling `beskid_rt_v5_heap_set_cap` directly with a pre-parsed decimal byte
 * count from `BESKID_TEST_HEAP_CAP_BYTES`, when present.
 */
extern int64_t HEAP_GROWTH_FIXTURE_ENTRY(void) __asm__(HEAP_GROWTH_FIXTURE_SYMBOL);
extern uint8_t gc_heap_verify(void);
extern void gc_heap_force_root_stack_failure(size_t enabled);
extern void gc_heap_set_stress_interval(size_t n);

int main(void) {
    _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
    assert(beskid_rt_v5_process_init(runtime) == runtime);
    const char *cap_text = getenv("BESKID_TEST_HEAP_CAP_BYTES");
    if (cap_text != NULL && cap_text[0] != '\0') {
        long long cap_value = atoll(cap_text);
        if (cap_value > 0) {
            (void)beskid_rt_v5_heap_set_cap((size_t)cap_value);
        }
    }
    if (getenv("BESKID_TEST_FORCE_ROOT_STACK_FAILURE") != NULL) {
        gc_heap_force_root_stack_failure(1);
    }
    const char *stress_text = getenv("BESKID_TEST_STRESS_INTERVAL");
    if (stress_text != NULL && stress_text[0] != '\0') {
        long long stress_value = atoll(stress_text);
        if (stress_value > 0) {
            gc_heap_set_stress_interval((size_t)stress_value);
        }
    }
    int64_t result = HEAP_GROWTH_FIXTURE_ENTRY();
    assert(result >= 0);
    assert(gc_heap_verify() != 0);
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}
