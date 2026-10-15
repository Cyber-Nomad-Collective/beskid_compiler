/* Test the production checked transition without adding a reset API to the process provider. */
#define BESKID_GLUE_ISSUER_EXHAUSTION_TEST 1
#include "owner_identity_v1.c"
#include <assert.h>
int main(void) {
    _Atomic uint64_t exhausted = UINT64_MAX - 1;
    assert(next_identity(&exhausted) == UINT64_MAX);
    assert(next_identity(&exhausted) == 0);
    assert(next_identity(&exhausted) == 0);
    assert(atomic_load(&exhausted) == UINT64_MAX);
    assert(beskid_glue_v1_next_identity() == 1);
    assert(beskid_glue_v1_next_identity() == 2);
    return 0;
}
