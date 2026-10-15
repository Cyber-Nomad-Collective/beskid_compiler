#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

/*
 * Use-after-collection fixture for the fail-fast liveness rule: a runtime service that receives a
 * managed operand which is no longer a live object traps `invalid_or_stale_handle` with a
 * diagnostic and never substitutes a null, empty, or failure value.
 *
 * `main <service>` hands that service an operand that only this C frame referenced: nothing roots
 * it, so the collection before the call frees it. Returning from the call at all is the failure
 * the rule forbids, so the fixture then prints what the service substituted and exits 0 (the test
 * requires exit status 101 and the trap diagnostic). `main live` drives the same services with
 * rooted operands across a collection and must succeed.
 */

typedef struct { _Alignas(8) unsigned char bytes[BESKID_ABI_VALUE_SIZE]; } ValueSlot;

static void *rooted_string(const char *text, size_t *handle) {
    void *value = str_new((void *)text, strlen(text));
    assert(value);
    *handle = gc_root_handle(value);
    assert(*handle);
    return value;
}

/* A string that nothing roots; the collection frees its record and payload. */
static void *collected_string(void) {
    void *value = str_new((void *)"collected", 9);
    assert(value);
    gc_collect();
    return value;
}

static int run_live(void) {
    size_t left_root = 0, right_root = 0;
    void *left = rooted_string("live", &left_root);
    void *right = rooted_string(" text", &right_root);
    gc_collect();
    void *joined = str_concat(left, right);
    assert(joined && str_len(joined) == 9);
    size_t joined_root = gc_root_handle(joined);
    assert(joined_root);
    gc_collect();
    void *slice = str_slice(joined, 0, 4);
    assert(slice && str_eq(slice, left) == 1 && str_cmp(slice, left) == 0);
    assert(str_cmp(left, joined) < 0 && str_eq(left, joined) == 0);
    gc_unroot_handle(joined_root);
    gc_unroot_handle(right_root);
    gc_unroot_handle(left_root);
    return 0;
}

static int run_dead(const char *service, const char *self_path) {
    size_t live_root = 0;
    void *live = rooted_string("live", &live_root);
    if (strcmp(service, "abi_value_initialize") == 0) {
        uintptr_t descriptor[] = {24, 8, 0, 0, 0};
        uintptr_t request[] = {24, 8, (uintptr_t)descriptor};
        void *payload = beskid_rt_v5_managed_object_allocate(request);
        assert(payload);
        gc_collect();
        ValueSlot slot = {{0}};
        printf("substituted %u\n", (unsigned)beskid_rt_v5_abi_value_initialize(&slot, 7, payload, descriptor));
        return 0;
    }
    void *dead = collected_string();
    if (strcmp(service, "str_len") == 0) {
        printf("substituted %zu\n", str_len(dead));
    } else if (strcmp(service, "str_eq") == 0) {
        printf("substituted %zu\n", str_eq(live, dead));
    } else if (strcmp(service, "str_cmp") == 0) {
        printf("substituted %lld\n", (long long)str_cmp(dead, live));
    } else if (strcmp(service, "str_concat") == 0) {
        printf("substituted %p\n", str_concat(live, dead));
    } else if (strcmp(service, "str_slice") == 0) {
        printf("substituted %p\n", str_slice(dead, 0, 3));
    } else if (strcmp(service, "gc_root_handle") == 0) {
        printf("substituted %zu\n", gc_root_handle(dead));
    } else if (strcmp(service, "gc_try_root_handle") == 0) {
        printf("substituted %zu\n", beskid_rt_v5_gc_try_root_handle(dead));
    } else if (strcmp(service, "utf8_record_adopt") == 0) {
        /* `fs_read_text` adopts the text it read into the caller's string record. */
        size_t path_root = 0;
        void *path = rooted_string(self_path, &path_root);
        printf("substituted %d\n", beskid_rt_v5_fs_read_text(path, dead));
    } else {
        fprintf(stderr, "unknown service %s\n", service);
        return 2;
    }
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        return 2;
    }
    _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
    assert(beskid_rt_v5_process_init(runtime) == runtime);
    if (strcmp(argv[1], "live") == 0) {
        int status = run_live();
        beskid_rt_v5_process_shutdown(runtime);
        return status;
    }
    /* Reached only when the service returned instead of trapping. */
    return run_dead(argv[1], argv[0]);
}
