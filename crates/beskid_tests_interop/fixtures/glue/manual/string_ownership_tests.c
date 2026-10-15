/* Canonical primitive String must participate in the same GC as typed arrays. */
#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <stdint.h>
#include <stdalign.h>

static alignas(BESKID_RUNTIME_STATE_ALIGNMENT) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE];
int main(void) {
    assert(beskid_rt_v5_process_init(runtime));
    uint8_t bytes[] = {65, 0, 0xF0, 0x9F, 0x98, 0x80};
    void *value = str_new(bytes, sizeof bytes);
    assert(value && str_len(value) == sizeof bytes);
    uintptr_t retained = gc_root_handle(value);
    assert(retained != 0 && "primitive String must have a live canonical GC object");
    gc_collect();
    assert(str_len(value) == sizeof bytes);
    void *empty = str_new(NULL, 0);
    uintptr_t empty_root = gc_root_handle(empty);
    assert(empty && empty_root);
    void *copy = str_concat(value, empty);
    uintptr_t copy_root = gc_root_handle(copy);
    assert(copy && copy_root && str_eq(value, copy));
    gc_collect();
    assert(str_eq(value, copy));
    gc_unroot_handle(copy_root);
    gc_unroot_handle(empty_root);
    gc_unroot_handle(retained);
    gc_collect();
    assert(gc_heap_verify());
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}
