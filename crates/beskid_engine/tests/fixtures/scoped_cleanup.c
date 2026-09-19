#include "beskid_runtime_abi_v5.h"
#include <assert.h>
extern int64_t RunCleanupFixture(void) __asm__(CLEANUP_FIXTURE_SYMBOL);
int main(void) {
    _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
    assert(beskid_rt_v5_process_init(runtime) == runtime);
    assert(RunCleanupFixture() == 42);
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}
