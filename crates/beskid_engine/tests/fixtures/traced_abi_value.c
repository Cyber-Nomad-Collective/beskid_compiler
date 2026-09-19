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
static void *fiber_wait_fixture(void *argument) { gc_collect(); return argument; }
static void *fiber_panic_fixture(void *argument) { assert(gc_register_root(&argument)); beskid_trap_code(19); return 0; }
static void *fiber_panic_message_fixture(void *argument) {
    (void)argument;
    uintptr_t message[] = {(uintptr_t)"child panic", 11};
    beskid_trap_message(message);
    return 0;
}
static void *fiber_forever_fixture(void *argument) { (void)argument; assert(!"detached work must not delay shutdown"); return 0; }
static void *fiber_spawn_detached_fixture(void *argument) {
    (void)argument;
    int64_t detached = fiber_spawn((void *)fiber_forever_fixture, 0);
    assert(detached >= 0);
    fiber_detach(detached);
    static uintptr_t descriptor[] = {24, 8, 0, 0, 0};
    uintptr_t request[] = {24, 8, (uintptr_t)descriptor};
    return beskid_rt_v5_managed_object_allocate(request);
}

static void transfer_via_fiber(ValueSlot *owner) {
    void *expected = payload(owner);
    int64_t child = fiber_spawn((void *)fiber_wait_fixture, expected);
    assert(child >= 0);
    assert(beskid_rt_v5_abi_value_clear(owner));
    gc_collect(); /* only the scheduler-owned capture retains the payload */
    assert(fiber_join_status(child) == 0);
    gc_collect(); /* only the terminal result retains it now */
    assert(fiber_join_value(child, owner));
    assert(!fiber_join_value(child, owner));
    assert(payload(owner) == expected);
    gc_collect();
}

int RunAbiValueFixture(MoveValue move) {
    _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
    assert(beskid_rt_v5_process_init(runtime) == runtime);
    int64_t children[16];
    uintptr_t fiber_descriptor[] = {24, 8, 0, 0, 0};
    uintptr_t fiber_request[] = {24, 8, (uintptr_t)fiber_descriptor};
    uintptr_t *fiber_payload = beskid_rt_v5_managed_object_allocate(fiber_request);
    assert(fiber_payload);
    fiber_payload[2] = 42;
    for (size_t i = 0; i < 16; ++i) {
        children[i] = fiber_spawn((void *)fiber_wait_fixture, fiber_payload);
        assert(children[i] >= 0);
        assert(fiber_current_id() == -1); /* Record growth must not overlap scheduler state. */
    }
    for (size_t i = 0; i < 16; ++i) {
        assert(fiber_join_status(children[i]) == 0); /* Join drives child completion. */
        ValueSlot result = {0};
        assert(fiber_join_value(children[i], &result));
        assert(payload(&result) == fiber_payload && fiber_payload[2] == 42);
        assert(!fiber_join_value(children[i], &result));
        assert(beskid_rt_v5_abi_value_clear(&result));
    }
    gc_collect();
    assert(gc_external_root_count() == 0 && gc_object_count() == 0);
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
    transfer_via_fiber(&sender);
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
    transfer_via_fiber(&sender);
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
    transfer_via_fiber(&sender);
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
    /* A generic root registration cannot confer ABI ownership on copied bytes. */
    ValueSlot registered_copy = {0};
    assert(gc_register_root(field(&registered_copy, BESKID_ABI_VALUE_PAYLOAD_OFFSET)));
    memcpy(&registered_copy, &receiver, sizeof(receiver));
    ValueSlot receiver_before = receiver, registered_before = registered_copy;
    assert(!move(&registered_copy, &sender));
    assert(!move(&receiver, &registered_copy));
    assert(!beskid_rt_v5_abi_value_clear(&registered_copy));
    assert(!beskid_rt_v5_abi_value_initialize(&registered_copy, 13, owned_resource, resource_descriptor));
    assert(!beskid_rt_v5_abi_value_replace_with_barrier(&registered_copy, &receiver));
    assert(!beskid_rt_v5_abi_value_replace_with_barrier(&receiver, &registered_copy));
    assert(vacant(&sender) && memcmp(&receiver, &receiver_before, sizeof(receiver)) == 0);
    assert(memcmp(&registered_copy, &registered_before, sizeof(registered_copy)) == 0);
    gc_unregister_root(field(&registered_copy, BESKID_ABI_VALUE_PAYLOAD_OFFSET));

    /* Nor can overwriting an already live, address-rooted ABI destination. */
    ValueSlot live_copy = {0};
    uintptr_t *previous_payload = beskid_rt_v5_managed_object_allocate(resource_request);
    assert(previous_payload);
    previous_payload[2] = 7; /* ordinary boxed scalar owned by the original slot */
    assert(beskid_rt_v5_abi_value_initialize(&live_copy, 14, previous_payload, resource_descriptor));
    ValueSlot live_before = live_copy;
    memcpy(&live_copy, &receiver, sizeof(receiver));
    assert(!move(&live_copy, &sender));
    assert(!move(&receiver, &live_copy));
    assert(!beskid_rt_v5_abi_value_clear(&live_copy));
    assert(!beskid_rt_v5_abi_value_initialize(&live_copy, 13, owned_resource, resource_descriptor));
    assert(!beskid_rt_v5_abi_value_replace_with_barrier(&live_copy, &receiver));
    assert(!beskid_rt_v5_abi_value_replace_with_barrier(&receiver, &live_copy));
    assert(vacant(&sender) && memcmp(&receiver, &receiver_before, sizeof(receiver)) == 0);
    assert(memcmp(&live_copy, &receiver_before, sizeof(live_copy)) == 0);
    /* Restore this test-corrupted slot so its legitimate root can be released. */
    memcpy(&live_copy, &live_before, sizeof(live_copy));
    assert(beskid_rt_v5_abi_value_clear(&live_copy));
    assert(gc_external_root_count() == 1);
    uintptr_t saved_heap = *field(&receiver, BESKID_ABI_VALUE_OWNER_HEAP_OFFSET);
    *field(&receiver, BESKID_ABI_VALUE_OWNER_HEAP_OFFSET) = 1;
    assert(!beskid_rt_v5_abi_value_clear(&receiver));
    *field(&receiver, BESKID_ABI_VALUE_OWNER_HEAP_OFFSET) = saved_heap;
    owned_resource[2] = 0; /* exactly one explicit resource cleanup */
    assert(beskid_rt_v5_abi_value_clear(&receiver));
    gc_collect();
    assert(vacant(&receiver) && gc_object_count() == 0 && gc_external_root_count() == 0);
    int64_t cancelled = fiber_spawn((void *)fiber_wait_fixture, 0);
    assert(fiber_cancel(cancelled, 17));
    assert(fiber_cancel(cancelled, 29));
    uintptr_t *scheduler = (uintptr_t *)*(uintptr_t *)(runtime + BESKID_RUNTIME_STATE_SCHEDULER_OFFSET);
    unsigned char *cancelled_record = (unsigned char *)scheduler + BESKID_SCHEDULER_STATE_FIBERS_OFFSET
        + ((uint64_t)cancelled & UINT32_MAX) * BESKID_FIBER_RECORD_SIZE;
    assert(*(uintptr_t *)(cancelled_record + BESKID_FIBER_RECORD_OUTCOME_REASON_OFFSET) == 17);
    assert(fiber_join_status(cancelled) == 1);
    fiber_detach(cancelled);
    int64_t panicked = fiber_spawn((void *)fiber_panic_fixture, 0);
    assert(fiber_join_status(panicked) == 2); /* child panic is a result, not process abort */
    assert(gc_external_root_count() == 0); /* aborted child frames must not retain stale stack roots */
    assert(fiber_join_detail(panicked, 0) == 2);
    assert(fiber_join_error_finish(panicked));
    assert(!fiber_join_error_finish(panicked));
    int64_t detached = fiber_spawn((void *)fiber_forever_fixture, 0);
    fiber_detach(detached);
    assert(fiber_spawn((void *)fiber_spawn_detached_fixture, 0) >= 0);
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}

#ifdef ABI_VALUE_STANDALONE
int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "detached-panic") == 0) {
        _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
        assert(beskid_rt_v5_process_init(runtime) == runtime);
        int64_t child = fiber_spawn((void *)fiber_panic_message_fixture, 0);
        fiber_detach(child);
        int64_t sentinel = fiber_spawn((void *)fiber_wait_fixture, 0);
        fiber_join_status(sentinel); /* Running detached panic must abort the process. */
        return 0;
    }
    return RunAbiValueFixture(beskid_rt_v5_abi_value_move_out);
}
#endif
