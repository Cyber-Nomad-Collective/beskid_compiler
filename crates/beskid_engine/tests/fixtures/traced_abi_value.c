/* Behavioral ABI fixture; the same compiled Beskid runtime serves static AOT,
 * native-kit dynamic linking, and a JIT-generated move call. */
#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <string.h>
#include <stdlib.h>

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

static void channel_close_retains_committed_value(ValueSlot *owner) {
    void *expected = payload(owner);
    int64_t channel = channel_create(1, 0);
    assert(channel >= 0);
    assert(channel_try_send(channel, owner) == 0);
    assert(vacant(owner));
    channel_close(channel);
    gc_collect();
    assert(channel_try_receive(channel) == 0); /* Closed queues must drain. */
    assert(channel_receive_value(channel, owner));
    assert(payload(owner) == expected);
    ValueSlot duplicate = {0};
    assert(!channel_receive_value(channel, &duplicate));
    assert(channel_try_receive(channel) == 1);
    assert(channel_try_send(channel, owner) == 1);
    assert(payload(owner) == expected); /* close cannot consume an uncommitted send */
}

static void channel_dynamic_storage(void) {
    int64_t channel = channel_create(0, 0);
    uintptr_t descriptor[] = {24, 8, 0, 0, 0};
    uintptr_t request[] = {24, 8, (uintptr_t)descriptor};
    for (uintptr_t i = 0; i < 24; ++i) {
        uintptr_t *boxed = beskid_rt_v5_managed_object_allocate(request);
        assert(boxed);
        boxed[2] = i;
        ValueSlot sender = {0};
        assert(beskid_rt_v5_abi_value_initialize(&sender, 30, boxed, descriptor));
        assert(channel_try_send(channel, &sender) == 0);
        assert(vacant(&sender));
        gc_collect();
    }
    channel_close(channel);
    /* Each move validates slot-address ownership. Growing beyond the old ring
     * cannot byte-copy/relocate initialized cells or alter FIFO order. */
    for (uintptr_t i = 0; i < 24; ++i) {
        ValueSlot received = {0};
        assert(channel_try_receive(channel) == 0);
        gc_collect(); /* exclusive receipt must remain rooted */
        assert(channel_receive_value(channel, &received));
        assert(((uintptr_t *)payload(&received))[2] == i);
        assert(beskid_rt_v5_abi_value_clear(&received));
    }
    assert(channel_try_receive(channel) == 1);
    gc_collect();
    assert(gc_external_root_count() == 0 && gc_object_count() == 0);
}

static int64_t receipt_channel;
static int64_t receipt_second_channel;
static uintptr_t *receipt_descriptor;
static ValueSlot *receipt_destination;
static int receipt_resumed;
static void *channel_receipt_thief(void *argument) {
    ValueSlot stolen = {0};
    assert(!channel_receive_value(receipt_channel, &stolen));
    assert(vacant(&stolen));
    return argument;
}
static void *channel_cancelled_receiver(void *argument) {
    assert(channel_try_receive(receipt_channel) == 0);
    assert(channel_try_receive(receipt_second_channel) == 0);
    ValueSlot nested = {0};
    assert(beskid_rt_v5_abi_value_initialize(&nested, 31, argument, receipt_descriptor));
    assert(channel_send(receipt_second_channel, &nested) == 0);
    assert(vacant(&nested)); /* nested send phase must not clear either receipt */
    assert(fiber_cancel(fiber_current_id(), 9));
    beskid_rt_v5_fiber_yield(); /* pending cancellation must retain the claimant */
    beskid_rt_v5_fiber_yield(); /* already-applied cancellation must also resume */
    assert(channel_receive_value(receipt_second_channel, &nested));
    assert(!channel_receive_value(receipt_channel, &nested)); /* failed move retains its count */
    assert(beskid_rt_v5_abi_value_clear(&nested));
    beskid_rt_v5_fiber_yield(); /* resolving one receipt cannot abandon the other */
    channel_close(receipt_channel);
    gc_collect();
    /* Cancellation after claim must not discard an already-committed receipt. */
    assert(channel_receive_value(receipt_channel, receipt_destination));
    assert(payload(receipt_destination) == argument);
    ValueSlot duplicate = {0};
    assert(!channel_receive_value(receipt_channel, &duplicate));
    assert(channel_try_receive(receipt_channel) == 2); /* cancellation only after ownership resolved */
    receipt_resumed = 1;
    return argument;
}
static void channel_receipt_ownership(ValueSlot *owner) {
    void *expected = payload(owner);
    receipt_descriptor = (uintptr_t *)*field(owner, BESKID_ABI_VALUE_DESCRIPTOR_OFFSET);
    receipt_channel = channel_create(1, 0);
    receipt_second_channel = channel_create(1, 0);
    assert(channel_try_send(receipt_channel, owner) == 0);
    assert(channel_try_receive(receipt_channel) == 0);
    gc_collect();
    int64_t thief = fiber_spawn((void *)channel_receipt_thief, expected);
    assert(fiber_join_status(thief) == 0);
    ValueSlot child = {0};
    assert(fiber_join_value(thief, &child));
    assert(beskid_rt_v5_abi_value_clear(&child));
    assert(channel_receive_value(receipt_channel, owner));
    assert(payload(owner) == expected);
    assert(channel_try_send(receipt_channel, owner) == 0);
    ValueSlot second = {0};
    assert(beskid_rt_v5_abi_value_initialize(&second, 32, expected, receipt_descriptor));
    assert(channel_try_send(receipt_second_channel, &second) == 0);
    receipt_destination = owner;
    receipt_resumed = 0;
    int64_t receiver = fiber_spawn((void *)channel_cancelled_receiver, expected);
    assert(fiber_join_status(receiver) == 1);
    fiber_join_error_finish(receiver);
    channel_close(receipt_channel);
    assert(receipt_resumed == 1); /* cannot terminalize/clean up an unresolved receipt */
    gc_collect();
    assert(payload(owner) == expected);
    assert(channel_try_receive(receipt_channel) == 1);
    channel_close(receipt_second_channel);
    assert(channel_try_receive(receipt_second_channel) == 0);
    assert(channel_receive_value(receipt_second_channel, &second));
    assert(payload(&second) == expected);
    assert(beskid_rt_v5_abi_value_clear(&second));
    assert(channel_try_receive(receipt_second_channel) == 1);
}

static int receipt_abort_mode;
static void *channel_abandoned_receiver(void *argument) {
    assert(channel_try_receive(receipt_channel) == 0);
    static uintptr_t tail_descriptor[] = {24, 8, 0, 0, 0};
    uintptr_t tail_request[] = {24, 8, (uintptr_t)tail_descriptor};
    uintptr_t *tail = beskid_rt_v5_managed_object_allocate(tail_request);
    assert(tail);
    tail[2] = 99;
    ValueSlot newer = {0};
    assert(beskid_rt_v5_abi_value_initialize(&newer, 34, tail, tail_descriptor));
    assert(channel_try_send(receipt_channel, &newer) == 0);
    beskid_rt_v5_fiber_yield(); /* let another receiver park behind the exclusive receipt */
    if (receipt_abort_mode == 1) beskid_trap_code(19);
    if (receipt_abort_mode == 2) {
        assert(fiber_cancel(fiber_current_id(), 9));
        beskid_rt_v5_fiber_yield();
        beskid_rt_v5_fiber_yield();
    }
    return argument; /* explicit return/panic/cancel teardown returns receipt to queue */
}
static void channel_receipt_abort(ValueSlot *owner, int mode) {
    void *expected = payload(owner);
    receipt_channel = channel_create(1, 0);
    receipt_abort_mode = mode;
    assert(channel_try_send(receipt_channel, owner) == 0);
    int64_t claimant = fiber_spawn((void *)channel_abandoned_receiver, expected);
    assert(fiber_join_status(claimant) == (mode == 0 ? 0 : mode == 1 ? 2 : 1));
    ValueSlot result = {0};
    if (mode == 0) {
        assert(fiber_join_value(claimant, &result));
        assert(beskid_rt_v5_abi_value_clear(&result));
    } else assert(fiber_join_error_finish(claimant));
    /* Repeated stale cleanup must neither retract nor duplicate the restored cell. */
    assert(!fiber_join_error_finish(claimant));
    int64_t next_generation = fiber_spawn((void *)fiber_wait_fixture, expected);
    assert(next_generation != claimant);
    assert(!fiber_join_error_finish(claimant));
    assert(!fiber_cancel(claimant, 7));
    assert(fiber_join_status(next_generation) == 0);
    assert(fiber_join_value(next_generation, &result));
    assert(beskid_rt_v5_abi_value_clear(&result));
    channel_close(receipt_channel);
    gc_collect();
    assert(channel_try_receive(receipt_channel) == 0);
    assert(channel_receive_value(receipt_channel, owner));
    assert(payload(owner) == expected);
    assert(!channel_receive_value(receipt_channel, &result));
    assert(channel_try_receive(receipt_channel) == 0); /* restored receipt precedes later commit */
    assert(channel_receive_value(receipt_channel, &result));
    assert(((uintptr_t *)payload(&result))[2] == 99);
    assert(beskid_rt_v5_abi_value_clear(&result));
    assert(channel_try_receive(receipt_channel) == 1);
}

static int64_t receipt_wait_hub;
static void *channel_receipt_waiter(void *argument) {
    ValueSlot received = {0};
    if (receipt_wait_hub >= 0) {
        assert(hub_wait_receive_status(receipt_wait_hub) == 0);
        assert(hub_wait_receive_value(receipt_wait_hub, &received));
    } else {
        assert(channel_receive_status(receipt_channel) == 0);
        assert(channel_receive_value(receipt_channel, &received));
    }
    assert(payload(&received) == argument);
    assert(beskid_rt_v5_abi_value_clear(&received));
    return argument;
}
static void channel_receipt_cleanup_wakes(ValueSlot *owner, int use_hub) {
    void *expected = payload(owner);
    receipt_channel = channel_create(1, 0);
    receipt_abort_mode = 0;
    receipt_wait_hub = use_hub ? hub_create() : -1;
    if (use_hub) assert(hub_register(receipt_wait_hub, 7, receipt_channel) == 0);
    assert(channel_try_send(receipt_channel, owner) == 0);
    int64_t claimant = fiber_spawn((void *)channel_abandoned_receiver, expected);
    int64_t waiter = fiber_spawn((void *)channel_receipt_waiter, expected);
    assert(fiber_join_status(claimant) == 0);
    ValueSlot result = {0};
    assert(fiber_join_value(claimant, &result)); /* cleanup wakes the parked receiver */
    assert(beskid_rt_v5_abi_value_clear(&result));
    assert(fiber_join_status(waiter) == 0);
    assert(fiber_join_value(waiter, owner));
    assert(payload(owner) == expected);
    channel_close(receipt_channel);
    assert(channel_try_receive(receipt_channel) == 0);
    assert(channel_receive_value(receipt_channel, &result));
    assert(((uintptr_t *)payload(&result))[2] == 99);
    assert(beskid_rt_v5_abi_value_clear(&result));
    assert(channel_try_receive(receipt_channel) == 1);
}

static int64_t race_channel, race_sender, race_status;
static int race_mode, race_sender_owned;
static void *channel_sender_fixture(void *argument) {
    ValueSlot pending = {0};
    /* Normalize arrays through their object header's descriptor in the caller. */
    extern uintptr_t *race_descriptor;
    assert(beskid_rt_v5_abi_value_initialize(&pending, 21, argument, race_descriptor));
    race_status = channel_send(race_channel, &pending);
    race_sender_owned = !vacant(&pending);
    assert(beskid_rt_v5_abi_value_clear(&pending));
    return argument;
}
uintptr_t *race_descriptor;
static void *channel_controller_fixture(void *argument) {
    gc_collect(); /* pending sender or committed queue is the transfer owner */
    if (race_mode == 4) {
        ValueSlot sender = {0};
        assert(beskid_rt_v5_abi_value_initialize(&sender, 23, argument, race_descriptor));
        assert(channel_try_send(race_channel, &sender) == 0);
    } else if (race_mode == 1) {
        ValueSlot first = {0};
        assert(channel_try_receive(race_channel) == 0);
        assert(channel_receive_value(race_channel, &first));
        assert(beskid_rt_v5_abi_value_clear(&first));
    } else if (race_mode == 3 || race_mode == 6) {
        channel_close(race_channel);
    } else {
        assert(fiber_cancel(race_sender, 7));
    }
    return argument;
}
static void *channel_waiting_receiver(void *argument) {
    ValueSlot receiver = {0};
    race_status = channel_receive_status(race_channel);
    if (race_status == 0) {
        assert(channel_receive_value(race_channel, &receiver));
        assert(payload(&receiver) == argument);
        assert(beskid_rt_v5_abi_value_clear(&receiver));
    } else assert(vacant(&receiver));
    return argument;
}
static void channel_receiver_wake(ValueSlot *owner, int mode) {
    void *expected = payload(owner);
    race_descriptor = (uintptr_t *)*field(owner, BESKID_ABI_VALUE_DESCRIPTOR_OFFSET);
    race_channel = channel_create(1, 0);
    race_mode = mode;
    race_status = -100;
    race_sender = fiber_spawn((void *)channel_waiting_receiver, expected);
    int64_t controller = fiber_spawn((void *)channel_controller_fixture, expected);
    assert(beskid_rt_v5_abi_value_clear(owner));
    int64_t outcome = fiber_join_status(race_sender);
    assert(outcome == (mode == 5 ? 1 : 0));
    if (outcome == 0) {
        ValueSlot joined = {0};
        assert(fiber_join_value(race_sender, &joined));
        assert(beskid_rt_v5_abi_value_clear(&joined));
    } else fiber_join_error_finish(race_sender);
    assert(fiber_join_status(controller) == 0);
    assert(fiber_join_value(controller, owner));
    assert(race_status == (mode == 4 ? 0 : mode == 5 ? 2 : 1));
    channel_close(race_channel);
    assert(channel_try_receive(race_channel) == 1);
}
static void channel_race(ValueSlot *owner, int mode) {
    void *expected = payload(owner);
    race_descriptor = (uintptr_t *)*field(owner, BESKID_ABI_VALUE_DESCRIPTOR_OFFSET);
    race_channel = channel_create(1, 0);
    race_mode = mode;
    race_status = -100;
    race_sender_owned = -100;
    if (mode != 2) {
        uintptr_t descriptor[] = {24, 8, 0, 0, 0};
        uintptr_t request[] = {24, 8, (uintptr_t)descriptor};
        void *first = beskid_rt_v5_managed_object_allocate(request);
        ValueSlot queued = {0};
        assert(beskid_rt_v5_abi_value_initialize(&queued, 22, first, descriptor));
        assert(channel_try_send(race_channel, &queued) == 0);
        /* Descriptor must outlive all queued cells: drain this initial cell before return. */
        race_sender = fiber_spawn((void *)channel_sender_fixture, expected);
        int64_t controller = fiber_spawn((void *)channel_controller_fixture, expected);
        assert(race_sender >= 0 && controller >= 0);
        assert(beskid_rt_v5_abi_value_clear(owner));
        int64_t status = fiber_join_status(race_sender);
        assert(status == (mode == 0 ? 1 : 0));
        if (status == 0) { ValueSlot joined = {0}; assert(fiber_join_value(race_sender, &joined)); assert(beskid_rt_v5_abi_value_clear(&joined)); }
        else fiber_join_error_finish(race_sender);
        assert(fiber_join_status(controller) == 0);
        assert(fiber_join_value(controller, owner));
        assert(race_status == (mode == 0 ? 2 : mode == 3 ? 1 : 0));
        assert(race_sender_owned == (mode == 1 ? 0 : 1));
        ValueSlot received = {0};
        assert(channel_try_receive(race_channel) == 0);
        assert(channel_receive_value(race_channel, &received));
        if (mode == 1) assert(payload(&received) == expected);
        assert(beskid_rt_v5_abi_value_clear(&received));
    } else {
        race_sender = fiber_spawn((void *)channel_sender_fixture, expected);
        int64_t controller = fiber_spawn((void *)channel_controller_fixture, expected);
        assert(beskid_rt_v5_abi_value_clear(owner));
        assert(fiber_join_status(race_sender) == 1);
        fiber_join_error_finish(race_sender);
        assert(fiber_join_status(controller) == 0);
        assert(fiber_join_value(controller, owner));
        assert(race_status == 2 && race_sender_owned == 0);
        ValueSlot received = {0};
        assert(channel_try_receive(race_channel) == 0);
        assert(channel_receive_value(race_channel, &received));
        assert(payload(&received) == expected);
        assert(beskid_rt_v5_abi_value_clear(&received));
    }
    assert(payload(owner) == expected);
    channel_close(race_channel);
    assert(channel_try_receive(race_channel) == 1);
    gc_collect();
}

int RunAbiValueFixture(MoveValue move) {
    _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
    assert(beskid_rt_v5_process_init(runtime) == runtime);
    channel_dynamic_storage();
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
    channel_close_retains_committed_value(&sender);
    channel_race(&sender, 0);
    channel_race(&sender, 1);
    channel_race(&sender, 2);
    channel_race(&sender, 3);
    channel_receipt_ownership(&sender);
    for (int mode = 0; mode < 3; ++mode) channel_receipt_abort(&sender, mode);
    for (int hub = 0; hub < 2; ++hub) channel_receipt_cleanup_wakes(&sender, hub);
    for (int mode = 4; mode < 7; ++mode) channel_receiver_wake(&sender, mode);
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
    channel_close_retains_committed_value(&sender);
    for (int mode = 0; mode < 4; ++mode) channel_race(&sender, mode);
    channel_receipt_ownership(&sender);
    for (int mode = 0; mode < 3; ++mode) channel_receipt_abort(&sender, mode);
    for (int hub = 0; hub < 2; ++hub) channel_receipt_cleanup_wakes(&sender, hub);
    for (int mode = 4; mode < 7; ++mode) channel_receiver_wake(&sender, mode);
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
    channel_close_retains_committed_value(&sender);
    for (int mode = 0; mode < 4; ++mode) channel_race(&sender, mode);
    channel_receipt_ownership(&sender);
    for (int mode = 0; mode < 3; ++mode) channel_receipt_abort(&sender, mode);
    for (int hub = 0; hub < 2; ++hub) channel_receipt_cleanup_wakes(&sender, hub);
    for (int mode = 4; mode < 7; ++mode) channel_receiver_wake(&sender, mode);
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
static unsigned char *receipt_runtime;
static int receipt_count_underflow;
static void *channel_receipt_count_corruption(void *argument) {
    uintptr_t scheduler = *(uintptr_t *)(receipt_runtime + BESKID_RUNTIME_STATE_SCHEDULER_OFFSET);
    uintptr_t index = (uintptr_t)fiber_current_id() & UINT32_MAX;
    unsigned char *operation = (unsigned char *)scheduler + BESKID_SCHEDULER_STATE_FIBERS_OFFSET
        + index * BESKID_FIBER_RECORD_SIZE + BESKID_FIBER_RECORD_CHANNEL_OPERATION_OFFSET;
    if (receipt_count_underflow) {
        assert(channel_try_receive(receipt_channel) == 0);
        *operation = 0; /* fault injection: resolving must reject count underflow */
        ValueSlot received = {0};
        channel_receive_value(receipt_channel, &received);
    } else {
        *operation = 252; /* fault injection: acquiring must reject 63 -> 64 */
        channel_try_receive(receipt_channel);
    }
    (void)argument;
    _Exit(0); /* only a trap at the operation boundary can satisfy the control */
}
int main(int argc, char **argv) {
    if (argc == 2 && (strcmp(argv[1], "receipt-count-overflow") == 0
        || strcmp(argv[1], "receipt-count-underflow") == 0)) {
        _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
        assert(beskid_rt_v5_process_init(runtime) == runtime);
        receipt_runtime = runtime;
        receipt_count_underflow = strcmp(argv[1], "receipt-count-underflow") == 0;
        uintptr_t descriptor[] = {24, 8, 0, 0, 0};
        uintptr_t request[] = {24, 8, (uintptr_t)descriptor};
        void *value = beskid_rt_v5_managed_object_allocate(request);
        ValueSlot owner = {0};
        assert(beskid_rt_v5_abi_value_initialize(&owner, 33, value, descriptor));
        receipt_channel = channel_create(1, 0);
        assert(channel_try_send(receipt_channel, &owner) == 0);
        int64_t child = fiber_spawn((void *)channel_receipt_count_corruption, value);
        fiber_join_status(child);
        return 0;
    }
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
