#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <unistd.h>

#if !CHILD_PANIC
// Observe the final OS boundary, whose production diagnostic deliberately omits code.
void beskid_rt_v5_intrinsic_trap(uint8_t code, void *message, size_t length) {
    (void)message;
    (void)length;
    static const char observed[] = "observed-trap\n";
    (void)write(STDOUT_FILENO, observed, sizeof(observed) - 1);
    _exit(code == PANIC_CODE ? 101 : 102);
}
#else
static void panic_child(void *unused) {
    (void)unused;
    beskid_trap_code(PANIC_CODE);
}
#endif
int main(void) {
    _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
    assert(beskid_rt_v5_process_init(runtime) == runtime);
#if CHILD_PANIC
    int64_t child = fiber_spawn((void *)panic_child, NULL);
    assert(child >= 0);
    assert(fiber_join_status(child) == 2);
    // FiberError::Panicked has mandated code 2, independently of process trap codes.
    assert(fiber_join_detail(child, 0) == 2);
    assert(fiber_join_error_finish(child));
    assert(!fiber_join_error_finish(child));
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
#else
    beskid_trap_code(PANIC_CODE);
#endif
}
