#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>

/* Host transport records, not Beskid object representations. Every object
 * offset and discriminant is supplied by the retained semantic queries. */
typedef struct {
    size_t size, tag_offset, ok_tag, error_tag, ok_offset, error_offset;
} TimerResultLayout;

typedef struct {
    size_t pointer_bytes, word_bytes, i64_bytes, tag_bytes;
    TimerResultLayout deadline, status;
    size_t error_size, error_tag_offset, unavailable_tag, overflow_tag, cancelled_tag;
    void *(*from_nanoseconds)(int64_t);
    void *(*deadline_from_sample)(void *, int64_t);
    void *(*result_from_status)(size_t);
} TimerValidationMetadata;

static uint32_t read_tag(const void *value, size_t offset) {
    uint32_t tag;
    assert(value);
    memcpy(&tag, (const unsigned char *)value + offset, sizeof(tag));
    return tag;
}

static void check_result_layout(const TimerResultLayout *layout, int has_deadline) {
    assert(layout->size >= sizeof(uint32_t));
    assert(layout->tag_offset <= layout->size - sizeof(uint32_t));
    assert(layout->ok_tag != layout->error_tag);
    assert(layout->size >= sizeof(void *));
    assert(layout->error_offset <= layout->size - sizeof(void *));
    if (has_deadline) {
        assert(layout->size >= sizeof(int64_t));
        assert(layout->ok_offset <= layout->size - sizeof(int64_t));
    } else {
        assert(layout->ok_offset == SIZE_MAX); /* query's unit field: no storage */
    }
}

static void observe(const TimerValidationMetadata *metadata, const TimerResultLayout *layout,
                    void *result, int ok, int64_t deadline, size_t error_tag) {
    assert(read_tag(result, layout->tag_offset) == (ok ? layout->ok_tag : layout->error_tag));
    if (!ok) {
        void *error;
        memcpy(&error, (unsigned char *)result + layout->error_offset, sizeof(error));
        assert(read_tag(error, metadata->error_tag_offset) == error_tag);
    } else if (layout->ok_offset != SIZE_MAX) {
        int64_t actual;
        memcpy(&actual, (unsigned char *)result + layout->ok_offset, sizeof(actual));
        assert(actual == deadline);
    }
}

size_t RunTimerValidation(const TimerValidationMetadata *metadata) {
    assert(metadata->pointer_bytes == sizeof(void *));
    assert(metadata->word_bytes == sizeof(size_t));
    assert(metadata->i64_bytes == sizeof(int64_t));
    assert(metadata->tag_bytes == sizeof(uint32_t));
    assert(metadata->from_nanoseconds && metadata->deadline_from_sample && metadata->result_from_status);
    check_result_layout(&metadata->deadline, 1);
    check_result_layout(&metadata->status, 0);
    assert(metadata->error_size >= sizeof(uint32_t));
    assert(metadata->error_tag_offset <= metadata->error_size - sizeof(uint32_t));
    const struct { const char *name; int64_t now, duration; int ok; size_t error_tag; } deadlines[] = {
        {"negative-one", -1, 0, 0, metadata->unavailable_tag},
        {"negative-min", INT64_MIN, 0, 0, metadata->unavailable_tag},
        {"exact-boundary", 17, INT64_MAX - 17, 1, 0},
        {"overflow-boundary", 17, INT64_MAX - 16, 0, metadata->overflow_tag},
        {"maximum-zero", INT64_MAX, 0, 1, 0},
        {"maximum-one", INT64_MAX, 1, 0, metadata->overflow_tag},
    };
    const struct { size_t status; int ok; size_t error_tag; } statuses[] = {
        {0, 0, metadata->unavailable_tag}, {3, 0, metadata->cancelled_tag}, {4, 1, 0},
    };
    const size_t roots_before = gc_external_root_count();
    void *duration = NULL, *result = NULL;
    assert(gc_register_root(&duration));
    assert(gc_register_root(&result));
    assert(gc_external_root_count() == roots_before + 2);
    size_t executed = 0;
    for (size_t i = 0; i < sizeof(deadlines) / sizeof(*deadlines); ++i) {
        duration = metadata->from_nanoseconds(deadlines[i].duration);
        assert(duration);
        gc_collect(); /* Duration stays rooted across the allocating helper. */
        result = metadata->deadline_from_sample(duration, deadlines[i].now);
        assert(result);
        gc_collect(); /* Also exercise the Result's managed TimerError payload. */
        observe(metadata, &metadata->deadline, result, deadlines[i].ok, INT64_MAX, deadlines[i].error_tag);
        ++executed;
        fprintf(stderr, "timer deadline %s passed\n", deadlines[i].name);
        duration = result = NULL;
    }
    for (size_t i = 0; i < sizeof(statuses) / sizeof(*statuses); ++i) {
        result = metadata->result_from_status(statuses[i].status);
        assert(result);
        gc_collect();
        observe(metadata, &metadata->status, result, statuses[i].ok, 0, statuses[i].error_tag);
        ++executed;
        fprintf(stderr, "timer status %zu passed\n", statuses[i].status);
        result = NULL;
    }
    gc_unregister_root(&result);
    gc_unregister_root(&duration);
    assert(gc_external_root_count() == roots_before);
    return executed;
}

#ifdef TIMER_VALIDATION_STANDALONE
extern void *TimerFromNanoseconds(int64_t) __asm__(TIMER_FROM_NANOSECONDS_SYMBOL);
extern void *TimerDeadlineFromSample(void *, int64_t) __asm__(TIMER_DEADLINE_SYMBOL);
extern void *TimerResultFromStatus(size_t) __asm__(TIMER_STATUS_SYMBOL);
int main(void) {
    TimerValidationMetadata metadata = TIMER_VALIDATION_METADATA;
    metadata.from_nanoseconds = TimerFromNanoseconds;
    metadata.deadline_from_sample = TimerDeadlineFromSample;
    metadata.result_from_status = TimerResultFromStatus;
    _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
    assert(beskid_rt_v5_process_init(runtime) == runtime);
    size_t executed = RunTimerValidation(&metadata);
    beskid_rt_v5_process_shutdown(runtime);
    printf("%zu\n", executed);
    return 0;
}
#endif
