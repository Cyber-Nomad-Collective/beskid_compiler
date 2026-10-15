/* ABI caller control fixture, not production descriptor issuance. */
#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <stdint.h>
#include <stdalign.h>
#include <string.h>

static alignas(BESKID_RUNTIME_STATE_ALIGNMENT) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE];
static void word_store(void *storage, size_t offset, uintptr_t value) {
    memcpy((unsigned char *)storage + offset, &value, sizeof value);
}
static uintptr_t word_load(void *storage, size_t offset) {
    uintptr_t value;
    memcpy(&value, (unsigned char *)storage + offset, sizeof value);
    return value;
}

int main(void) {
    alignas(BESKID_TYPE_DESCRIPTOR_ALIGNMENT) unsigned char descriptor[BESKID_TYPE_DESCRIPTOR_SIZE] = {0};
    alignas(BESKID_ARRAY_ELEMENT_DESCRIPTOR_ALIGNMENT) unsigned char element[BESKID_ARRAY_ELEMENT_DESCRIPTOR_SIZE] = {0};
    alignas(BESKID_ARRAY_ALLOCATION_REQUEST_ALIGNMENT) unsigned char request[BESKID_ARRAY_ALLOCATION_REQUEST_SIZE] = {0};
    word_store(descriptor, BESKID_TYPE_DESCRIPTOR_SIZE_OFFSET, 48);
    word_store(descriptor, BESKID_TYPE_DESCRIPTOR_ALIGNMENT_OFFSET, 8);
    uint32_t array_flag = 1;
    memcpy(descriptor + BESKID_TYPE_DESCRIPTOR_FLAGS_OFFSET, &array_flag, sizeof array_flag);
    word_store(element, BESKID_ARRAY_ELEMENT_DESCRIPTOR_STRIDE_OFFSET, 1);
    word_store(element, BESKID_ARRAY_ELEMENT_DESCRIPTOR_ALIGNMENT_OFFSET, 1);
    word_store(request, BESKID_ARRAY_ALLOCATION_REQUEST_ELEMENT_OFFSET, (uintptr_t)element);
    word_store(request, BESKID_ARRAY_ALLOCATION_REQUEST_DESCRIPTOR_OFFSET, (uintptr_t)descriptor);
    assert(beskid_rt_v5_process_init(runtime));
    uintptr_t root = 0;
    void *array = beskid_rt_v5_array_allocate_rooted(request, &root);
    assert(array && root);
    size_t replacements = 0;
    for (uintptr_t index = 0; index < 4096; ++index) {
        uintptr_t next_root = 0;
        void *replacement = beskid_rt_v5_array_grow_rooted(array, index + 1, &next_root);
        assert(replacement && next_root);
        if (replacement != array) ++replacements;
        assert(word_load(replacement, 8) == index);
        assert(word_load(replacement, 16) >= index + 1);
        assert(beskid_rt_v5_array_construction_finish((void *)root));
        array = replacement;
        root = next_root;
        unsigned char *data = (unsigned char *)word_load(array, 0);
        data[index] = (unsigned char)(index % 251);
        word_store(array, 8, index + 1);
        if (index % 127 == 0) gc_collect();
    }
    unsigned char *data = (unsigned char *)word_load(array, 0);
    for (uintptr_t index = 0; index < 4096; ++index) assert(data[index] == index % 251);
    assert(replacements <= 16 && "successive Append must grow geometrically rather than copy every prefix");
    assert(beskid_rt_v5_array_construction_finish((void *)root));
    gc_collect();
    assert(gc_heap_verify());
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}
