#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <unistd.h>
extern int64_t RunFoundationFixture(void) __asm__(FOUNDATION_FIXTURE_SYMBOL);
int main(void) {
    _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
    assert(beskid_rt_v5_process_init(runtime) == runtime);
#if SYSCALL_INPUT
    int descriptors[2];
    const unsigned char payload[3] = {0, 255, 42};
    assert(pipe(descriptors) == 0);
    assert(write(descriptors[1], payload, 3) == 3);
    assert(dup2(descriptors[0], 198) == 198);
    close(descriptors[0]);
    close(descriptors[1]);
#endif
    assert(RunFoundationFixture() == 42);
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}
