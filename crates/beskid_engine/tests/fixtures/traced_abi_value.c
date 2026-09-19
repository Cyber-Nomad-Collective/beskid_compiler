/* Behavioral ABI fixture; the same compiled Beskid runtime serves static AOT,
 * native-kit dynamic linking, and a JIT-generated move call. */
#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <string.h>

typedef uint8_t (*MoveValue)(void *, void *);
typedef struct { _Alignas(8) unsigned char bytes[BESKID_ABI_VALUE_SIZE]; } ValueSlot;
static uintptr_t *field(ValueSlot *slot, size_t offset) { return (uintptr_t *)(slot->bytes + offset); }
static void *payload(ValueSlot *slot) { return (void *)*field(slot, BESKID_ABI_VALUE_PAYLOAD_OFFSET); }
static int vacant(ValueSlot *slot) { ValueSlot zero = {0}; return memcmp(slot, &zero, sizeof(zero)) == 0; }

int RunAbiValueFixture(MoveValue move) {
    _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
    assert(beskid_rt_v5_process_init(runtime) == runtime);
    ValueSlot sender = {0}, receiver = {0};

    /* u8[]: construction handle -> sender root -> receiver root. */
    uintptr_t byte_element[] = {1, 1, 0, 0};
    uintptr_t array_descriptor[] = {48, 8, 0, 0, 1};
    uintptr_t array_request[] = {(uintptr_t)byte_element, 3, (uintptr_t)array_descriptor, 0};
    uintptr_t construction = 0;
    uintptr_t *array = beskid_rt_v5_array_allocate_rooted(array_request, &construction);
    assert(array && construction);
    memcpy((void *)array[0], "abc", 3);
    assert(!beskid_rt_v5_abi_value_initialize(&sender, 0, array, array_descriptor));
    assert(beskid_rt_v5_abi_value_initialize(&sender, 11, array, array_descriptor));
    assert(beskid_rt_v5_array_construction_finish((void *)construction));
    assert(gc_external_root_count() == 1);
    gc_collect();
    assert(gc_object_count() == 1);
    assert(move(&sender, &receiver));
    assert(vacant(&sender));
    assert(!move(&sender, &receiver));
    assert(!move(&receiver, &receiver));
    assert(!move(&receiver, (void *)((char *)&receiver + 8)));
    gc_collect();
    assert(payload(&receiver) == array && memcmp((void *)array[0], "abc", 3) == 0);

    /* Aggregate: its ordinary pointer map keeps the nested array alive. */
    uintptr_t map[] = {16};
    uintptr_t aggregate_descriptor[] = {32, 8, (uintptr_t)map, 1, 0};
    uintptr_t aggregate_request[] = {32, 8, (uintptr_t)aggregate_descriptor};
    uintptr_t *aggregate = beskid_rt_v5_managed_object_allocate(aggregate_request);
    assert(aggregate);
    aggregate[2] = (uintptr_t)array;
    aggregate[3] = 42;
    assert(!beskid_rt_v5_abi_value_initialize(&sender, 12, aggregate, array_descriptor));
    assert(vacant(&sender));
    assert(beskid_rt_v5_abi_value_initialize(&sender, 12, aggregate, aggregate_descriptor));
    assert(beskid_rt_v5_abi_value_replace_with_barrier(&receiver, &sender));
    assert(vacant(&sender) && gc_external_root_count() == 1);
    gc_collect();
    assert(gc_object_count() == 2 && payload(&receiver) == aggregate);
    assert(aggregate[3] == 42 && memcmp((void *)array[0], "abc", 3) == 0);
    assert(beskid_rt_v5_abi_value_clear(&receiver));
    assert(beskid_rt_v5_abi_value_clear(&receiver));
    gc_collect();
    assert(gc_object_count() == 0 && gc_external_root_count() == 0);

    /* Foundation-local opaque OwnedResource: boxed token, no pointer-side channel.
     * Transport does not dispose; explicit fixture cleanup owns the token. */
    uintptr_t resource_descriptor[] = {24, 8, 0, 0, 0};
    uintptr_t resource_request[] = {24, 8, (uintptr_t)resource_descriptor};
    uintptr_t *owned_resource = beskid_rt_v5_managed_object_allocate(resource_request);
    assert(owned_resource);
    owned_resource[2] = UINT64_C(0xfedcba9876543210);
    assert(beskid_rt_v5_abi_value_initialize(&sender, 13, owned_resource, resource_descriptor));
    uintptr_t empty_roots[62] = {0};
    for (size_t i = 0; i < 62; ++i) assert(gc_register_root(&empty_roots[i]));
    assert(!move(&sender, &receiver)); /* Full root registry: retain sender ownership. */
    assert(vacant(&receiver) && payload(&sender) == owned_resource);
    gc_collect();
    assert(gc_object_count() == 1);
    for (size_t i = 0; i < 62; ++i) gc_unregister_root(&empty_roots[i]);
    assert(move(&sender, &receiver));
    assert(vacant(&sender) && !move(&sender, &receiver));
    gc_collect();
    assert(payload(&receiver) == owned_resource && owned_resource[2] == UINT64_C(0xfedcba9876543210));
    ValueSlot illegal_copy = receiver;
    assert(!move(&illegal_copy, &sender)); /* Copying bytes must not mint ownership. */
    assert(!beskid_rt_v5_abi_value_clear(&illegal_copy));
    uintptr_t saved_heap = *field(&receiver, BESKID_ABI_VALUE_OWNER_HEAP_OFFSET);
    *field(&receiver, BESKID_ABI_VALUE_OWNER_HEAP_OFFSET) = 1;
    assert(!beskid_rt_v5_abi_value_clear(&receiver));
    *field(&receiver, BESKID_ABI_VALUE_OWNER_HEAP_OFFSET) = saved_heap;
    owned_resource[2] = 0; /* exactly one explicit resource cleanup */
    assert(beskid_rt_v5_abi_value_clear(&receiver));
    gc_collect();
    assert(vacant(&receiver) && gc_object_count() == 0 && gc_external_root_count() == 0);
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}

#ifdef ABI_VALUE_STANDALONE
int main(void) { return RunAbiValueFixture(beskid_rt_v5_abi_value_move_out); }
#endif
