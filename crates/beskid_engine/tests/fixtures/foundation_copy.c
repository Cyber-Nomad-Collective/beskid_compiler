#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <unistd.h>
#include <string.h>
struct ArrayView { unsigned char *data; size_t length; };
static struct ArrayView *observed;
static const unsigned char original[5] = {1, 2, 3, 4, 5};
void Remember(struct ArrayView *buffer) { observed = buffer; }
int64_t Case(void) { return COPY_CASE; }
void beskid_rt_v5_intrinsic_trap(uint8_t code, void *message, size_t length) {
    (void)message; (void)length;
    if (code != 2 || observed == NULL || observed->length != 5 || memcmp(observed->data, original, 5) != 0) _exit(102);
    static const char evidence[] = "unchanged-bounds\n";
    (void)write(STDOUT_FILENO, evidence, sizeof(evidence) - 1);
    _exit(101);
}
extern int64_t RunFoundationFixture(void) __asm__(FOUNDATION_FIXTURE_SYMBOL);
int main(void) {
    _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
    assert(beskid_rt_v5_process_init(runtime) == runtime);
    assert(RunFoundationFixture() == 42);
    assert(COPY_CASE == 6);
    assert(memcmp(observed->data, original, 5) == 0);
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}
