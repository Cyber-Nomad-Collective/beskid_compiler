/* @generated from runtime_manifest.bsol; do not edit. */
#ifndef BESKID_RUNTIME_ABI_V5_H
#define BESKID_RUNTIME_ABI_V5_H
#include <stddef.h>
#include <stdint.h>
#define BESKID_RUNTIME_ABI_VERSION 5
#define BESKID_TRAP_EXIT_STATUS 101
#define BESKID_TRAP_DIAGNOSTIC "beskid runtime trap v5"
struct BeskidStr;
#define BESKID_ALLOCATION_REQUEST_SIZE 24
#define BESKID_ALLOCATION_REQUEST_ALIGNMENT 8
#define BESKID_ALLOCATION_REQUEST_SIZE_OFFSET 0
#define BESKID_ALLOCATION_REQUEST_ALIGNMENT_OFFSET 8
#define BESKID_ALLOCATION_REQUEST_DESCRIPTOR_OFFSET 16
#define BESKID_ARRAY_ALLOCATION_REQUEST_SIZE 32
#define BESKID_ARRAY_ALLOCATION_REQUEST_ALIGNMENT 8
#define BESKID_ARRAY_ALLOCATION_REQUEST_ELEMENT_OFFSET 0
#define BESKID_ARRAY_ALLOCATION_REQUEST_LENGTH_OFFSET 8
#define BESKID_ARRAY_ALLOCATION_REQUEST_DESCRIPTOR_OFFSET 16
#define BESKID_ARRAY_ALLOCATION_REQUEST_FLAGS_OFFSET 24
#define BESKID_ARRAY_ALLOCATION_REQUEST_RESERVED_OFFSET 28
#define BESKID_ARRAY_ELEMENT_DESCRIPTOR_SIZE 32
#define BESKID_ARRAY_ELEMENT_DESCRIPTOR_ALIGNMENT 8
#define BESKID_ARRAY_ELEMENT_DESCRIPTOR_STRIDE_OFFSET 0
#define BESKID_ARRAY_ELEMENT_DESCRIPTOR_ALIGNMENT_OFFSET 8
#define BESKID_ARRAY_ELEMENT_DESCRIPTOR_POINTER_MAP_OFFSET 16
#define BESKID_ARRAY_ELEMENT_DESCRIPTOR_POINTER_COUNT_OFFSET 24
#define BESKID_CALLBACK_ENTRY_SIZE 16
#define BESKID_CALLBACK_ENTRY_ALIGNMENT 8
#define BESKID_CALLBACK_ENTRY_IDENTITY_OFFSET 0
#define BESKID_CALLBACK_ENTRY_TARGET_OFFSET 8
#define BESKID_CALLBACK_REGISTRY_SIZE 1056
#define BESKID_CALLBACK_REGISTRY_ALIGNMENT 8
#define BESKID_CALLBACK_REGISTRY_OWNER_RUNTIME_OFFSET 0
#define BESKID_CALLBACK_REGISTRY_CALLBACK_COUNT_OFFSET 8
#define BESKID_CALLBACK_REGISTRY_HANDLER_COUNT_OFFSET 16
#define BESKID_CALLBACK_REGISTRY_COMPOSITION_CONTAINER_OFFSET 24
#define BESKID_CALLBACK_REGISTRY_CALLBACKS_OFFSET 32
#define BESKID_CALLBACK_REGISTRY_HANDLERS_OFFSET 544
#define BESKID_COMPOSITION_CONTAINER_SIZE 64
#define BESKID_COMPOSITION_CONTAINER_ALIGNMENT 8
#define BESKID_COMPOSITION_CONTAINER_OWNER_RUNTIME_OFFSET 0
#define BESKID_COMPOSITION_CONTAINER_STATUS_OFFSET 8
#define BESKID_COMPOSITION_CONTAINER_REGISTRATION_COUNT_OFFSET 16
#define BESKID_COMPOSITION_CONTAINER_REGISTRATIONS_OFFSET 24
#define BESKID_COMPOSITION_CONTAINER_PLURAL_COUNT_OFFSET 32
#define BESKID_COMPOSITION_CONTAINER_PLURAL_BINDINGS_OFFSET 40
#define BESKID_COMPOSITION_CONTAINER_ACTIVATED_COUNT_OFFSET 48
#define BESKID_COMPOSITION_CONTAINER_OPEN_SCOPE_COUNT_OFFSET 56
#define BESKID_COMPOSITION_SCOPE_SIZE 32
#define BESKID_COMPOSITION_SCOPE_ALIGNMENT 8
#define BESKID_COMPOSITION_SCOPE_OWNER_CONTAINER_OFFSET 0
#define BESKID_COMPOSITION_SCOPE_PARENT_SCOPE_OFFSET 8
#define BESKID_COMPOSITION_SCOPE_OWNED_COUNT_OFFSET 16
#define BESKID_COMPOSITION_SCOPE_OWNED_VALUES_OFFSET 24
#define BESKID_FIBER_RECORD_SIZE 128
#define BESKID_FIBER_RECORD_ALIGNMENT 8
#define BESKID_FIBER_RECORD_STATE_OFFSET 0
#define BESKID_FIBER_RECORD_ENTRY_OFFSET 8
#define BESKID_FIBER_RECORD_ARGUMENT_OFFSET 16
#define BESKID_FIBER_RECORD_CANCEL_SLOT_OFFSET 24
#define BESKID_FIBER_RECORD_PARENT_OFFSET 32
#define BESKID_FIBER_RECORD_DETACHED_OFFSET 40
#define BESKID_FIBER_RECORD_CANCELLED_OFFSET 41
#define BESKID_FIBER_RECORD_GENERATION_OFFSET 44
#define BESKID_FIBER_RECORD_OUTCOME_KIND_OFFSET 48
#define BESKID_FIBER_RECORD_OUTCOME_VALUE_OFFSET 56
#define BESKID_FIBER_RECORD_COMPOSITION_SCOPE_OFFSET 64
#define BESKID_FIBER_RECORD_COMPOSITION_DEPTH_OFFSET 72
#define BESKID_FIBER_RECORD_ROOT_FRAME_OFFSET 80
#define BESKID_FIBER_RECORD_RESERVED_OFFSET 88
#define BESKID_FIBER_RECORD_STACK_BASE_OFFSET 96
#define BESKID_FIBER_RECORD_ARCH_CONTEXT_OFFSET 104
#define BESKID_FIBER_RECORD_ARCH_CONTEXT_SIZE_OFFSET 112
#define BESKID_FIBER_RECORD_STACK_USABLE_SIZE_OFFSET 120
#define BESKID_GC_HANDLE_SLOT_SIZE 16
#define BESKID_GC_HANDLE_SLOT_ALIGNMENT 8
#define BESKID_GC_HANDLE_SLOT_VALUE_OFFSET 0
#define BESKID_GC_HANDLE_SLOT_GENERATION_OFFSET 8
#define BESKID_HANDLE_SIZE 16
#define BESKID_HANDLE_ALIGNMENT 8
#define BESKID_HANDLE_SLOT_OFFSET 0
#define BESKID_HANDLE_GENERATION_OFFSET 4
#define BESKID_HANDLE_OWNER_COOKIE_OFFSET 8
#define BESKID_HEAP_STATE_SIZE 768
#define BESKID_HEAP_STATE_ALIGNMENT 8
#define BESKID_HEAP_STATE_REGION_START_OFFSET 0
#define BESKID_HEAP_STATE_REGION_SIZE_OFFSET 8
#define BESKID_HEAP_STATE_BUMP_OFFSET 16
#define BESKID_HEAP_STATE_LIMIT_OFFSET 24
#define BESKID_HEAP_STATE_LIVE_BYTES_OFFSET 32
#define BESKID_HEAP_STATE_LIVE_COUNT_OFFSET 40
#define BESKID_HEAP_STATE_COLLECTION_COUNT_OFFSET 48
#define BESKID_HEAP_STATE_COLLECTION_THRESHOLD_OFFSET 56
#define BESKID_HEAP_STATE_GRAY_COUNT_OFFSET 64
#define BESKID_HEAP_STATE_GRAY_ENTRIES_OFFSET 72
#define BESKID_HEAP_STATE_EXTERNAL_ROOT_COUNT_OFFSET 120
#define BESKID_HEAP_STATE_EXTERNAL_ROOTS_OFFSET 128
#define BESKID_HEAP_STATE_HANDLE_COUNT_OFFSET 632
#define BESKID_HEAP_STATE_HANDLES_OFFSET 640
#define BESKID_OBJECT_HEADER_SIZE 16
#define BESKID_OBJECT_HEADER_ALIGNMENT 8
#define BESKID_OBJECT_HEADER_DESCRIPTOR_OFFSET 0
#define BESKID_OBJECT_HEADER_GC_WORD_OFFSET 8
#define BESKID_PENDING_SPAWN_SIZE 40
#define BESKID_PENDING_SPAWN_ALIGNMENT 8
#define BESKID_PENDING_SPAWN_ENTRY_OFFSET 0
#define BESKID_PENDING_SPAWN_ARGUMENT_OFFSET 8
#define BESKID_PENDING_SPAWN_CANCEL_SLOT_OFFSET 16
#define BESKID_PENDING_SPAWN_PARENT_OFFSET 24
#define BESKID_PENDING_SPAWN_RESERVED_OFFSET 32
#define BESKID_POLL_LINK_SIZE 32
#define BESKID_POLL_LINK_ALIGNMENT 8
#define BESKID_POLL_LINK_GENERATION_OFFSET 0
#define BESKID_POLL_LINK_TASK_OFFSET 8
#define BESKID_POLL_LINK_LIVE_OFFSET 16
#define BESKID_POLL_LINK_RESERVED_OFFSET 24
#define BESKID_POLL_MONITOR_SIZE 24
#define BESKID_POLL_MONITOR_ALIGNMENT 8
#define BESKID_POLL_MONITOR_GENERATION_OFFSET 0
#define BESKID_POLL_MONITOR_TASK_OFFSET 8
#define BESKID_POLL_MONITOR_LIVE_OFFSET 16
#define BESKID_POLL_STATE_SIZE 6144
#define BESKID_POLL_STATE_ALIGNMENT 8
#define BESKID_POLL_STATE_TASKS_OFFSET 0
#define BESKID_POLL_STATE_MONITORS_OFFSET 2560
#define BESKID_POLL_STATE_LINKS_OFFSET 4096
#define BESKID_POLL_STATE_READY_QUEUE_OFFSET 5376
#define BESKID_POLL_STATE_READY_HEAD_OFFSET 5632
#define BESKID_POLL_STATE_READY_TAIL_OFFSET 5640
#define BESKID_POLL_STATE_TASK_COUNT_OFFSET 5648
#define BESKID_POLL_STATE_MONITOR_COUNT_OFFSET 5656
#define BESKID_POLL_STATE_LINK_COUNT_OFFSET 5664
#define BESKID_POLL_TASK_SIZE 80
#define BESKID_POLL_TASK_ALIGNMENT 8
#define BESKID_POLL_TASK_GENERATION_OFFSET 0
#define BESKID_POLL_TASK_STATE_OFFSET 8
#define BESKID_POLL_TASK_QUEUED_OFFSET 16
#define BESKID_POLL_TASK_DETACHED_OFFSET 17
#define BESKID_POLL_TASK_POLL_ENTRY_OFFSET 24
#define BESKID_POLL_TASK_TASK_STATE_OFFSET 32
#define BESKID_POLL_TASK_RESULT_SLOT_OFFSET 40
#define BESKID_POLL_TASK_CANCEL_SLOT_OFFSET 48
#define BESKID_POLL_TASK_OUTCOME_OFFSET 56
#define BESKID_POLL_TASK_RESULT_OFFSET 64
#define BESKID_POLL_TASK_LIVE_LINKS_OFFSET 72
#define BESKID_ROOT_FRAME_SIZE 24
#define BESKID_ROOT_FRAME_ALIGNMENT 8
#define BESKID_ROOT_FRAME_PREVIOUS_OFFSET 0
#define BESKID_ROOT_FRAME_SLOTS_OFFSET 8
#define BESKID_ROOT_FRAME_SLOT_COUNT_OFFSET 16
#define BESKID_ROOT_SLOT_SIZE 8
#define BESKID_ROOT_SLOT_ALIGNMENT 8
#define BESKID_ROOT_SLOT_VALUE_OFFSET 0
#define BESKID_RUNTIME_STATE_SIZE 64
#define BESKID_RUNTIME_STATE_ALIGNMENT 8
#define BESKID_RUNTIME_STATE_ABI_VERSION_OFFSET 0
#define BESKID_RUNTIME_STATE_FLAGS_OFFSET 4
#define BESKID_RUNTIME_STATE_CURRENT_THREAD_OFFSET 8
#define BESKID_RUNTIME_STATE_HEAP_OFFSET 16
#define BESKID_RUNTIME_STATE_CALLBACK_REGISTRY_OFFSET 24
#define BESKID_RUNTIME_STATE_SCHEDULER_OFFSET 32
#define BESKID_RUNTIME_STATE_ROOT_FRAME_OFFSET 40
#define BESKID_RUNTIME_STATE_TLS_KEY_OFFSET 48
#define BESKID_RUNTIME_STATE_LIFECYCLE_STATE_OFFSET 56
#define BESKID_RUNTIME_STATE_LIFECYCLE_GENERATION_OFFSET 60
#define BESKID_SCHEDULER_STATE_SIZE 3512
#define BESKID_SCHEDULER_STATE_ALIGNMENT 8
#define BESKID_SCHEDULER_STATE_FIBER_COUNT_OFFSET 0
#define BESKID_SCHEDULER_STATE_MAIN_FIBER_OFFSET 8
#define BESKID_SCHEDULER_STATE_RUN_QUEUE_HEAD_OFFSET 16
#define BESKID_SCHEDULER_STATE_RUN_QUEUE_TAIL_OFFSET 24
#define BESKID_SCHEDULER_STATE_RUN_QUEUE_OFFSET 32
#define BESKID_SCHEDULER_STATE_PENDING_SPAWNS_OFFSET 288
#define BESKID_SCHEDULER_STATE_PENDING_SPAWN_COUNT_OFFSET 608
#define BESKID_SCHEDULER_STATE_PENDING_CANCELS_OFFSET 616
#define BESKID_SCHEDULER_STATE_PENDING_CANCEL_COUNT_OFFSET 872
#define BESKID_SCHEDULER_STATE_PENDING_DETACHES_OFFSET 880
#define BESKID_SCHEDULER_STATE_PENDING_DETACH_COUNT_OFFSET 1136
#define BESKID_SCHEDULER_STATE_PENDING_WAKES_OFFSET 1144
#define BESKID_SCHEDULER_STATE_PENDING_WAKE_COUNT_OFFSET 1400
#define BESKID_SCHEDULER_STATE_FIBERS_OFFSET 1408
#define BESKID_SCHEDULER_STATE_CHANNEL_TABLE_OFFSET 3456
#define BESKID_SCHEDULER_STATE_MUTEX_TABLE_OFFSET 3464
#define BESKID_SCHEDULER_STATE_WAITGROUP_TABLE_OFFSET 3472
#define BESKID_SCHEDULER_STATE_CURRENT_FIBER_OFFSET 3480
#define BESKID_SCHEDULER_STATE_HUB_TABLE_OFFSET 3488
#define BESKID_SCHEDULER_STATE_SCHEDULER_CONTEXT_OFFSET 3496
#define BESKID_SCHEDULER_STATE_POLL_STATE_OFFSET 3504
#define BESKID_TLS_STATE_SIZE 48
#define BESKID_TLS_STATE_ALIGNMENT 8
#define BESKID_TLS_STATE_RUNTIME_OFFSET 0
#define BESKID_TLS_STATE_ROOT_FRAME_OFFSET 8
#define BESKID_TLS_STATE_RESERVED_OFFSET 16
#define BESKID_TLS_STATE_ATTACH_DEPTH_OFFSET 24
#define BESKID_TLS_STATE_COMPOSITION_SCOPE_OFFSET 32
#define BESKID_TLS_STATE_COMPOSITION_DEPTH_OFFSET 40
#define BESKID_TYPE_DESCRIPTOR_SIZE 40
#define BESKID_TYPE_DESCRIPTOR_ALIGNMENT 8
#define BESKID_TYPE_DESCRIPTOR_SIZE_OFFSET 0
#define BESKID_TYPE_DESCRIPTOR_ALIGNMENT_OFFSET 8
#define BESKID_TYPE_DESCRIPTOR_POINTER_MAP_OFFSET 16
#define BESKID_TYPE_DESCRIPTOR_POINTER_COUNT_OFFSET 24
#define BESKID_TYPE_DESCRIPTOR_FLAGS_OFFSET 32
#define BESKID_TYPE_DESCRIPTOR_RESERVED_OFFSET 36
#define BESKID_WORKER_REQUEST_SIZE 64
#define BESKID_WORKER_REQUEST_ALIGNMENT 8
#define BESKID_WORKER_REQUEST_NEXT_OFFSET 0
#define BESKID_WORKER_REQUEST_TAG_OFFSET 8
#define BESKID_WORKER_REQUEST_OPERATION_OFFSET 16
#define BESKID_WORKER_REQUEST_STATE_OFFSET 20
#define BESKID_WORKER_REQUEST_NATIVE_HANDLE_OFFSET 24
#define BESKID_WORKER_REQUEST_BUFFER_OFFSET 32
#define BESKID_WORKER_REQUEST_LENGTH_OFFSET 40
#define BESKID_WORKER_REQUEST_RESULT_OFFSET 48
#define BESKID_WORKER_REQUEST_ERROR_OFFSET 56
#define BESKID_WORKER_REQUEST_PADDING_OFFSET 60
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_SIZE 176
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_ALIGNMENT 16
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_X19_OFFSET 0
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_X20_OFFSET 8
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_X21_OFFSET 16
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_X22_OFFSET 24
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_X23_OFFSET 32
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_X24_OFFSET 40
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_X25_OFFSET 48
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_X26_OFFSET 56
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_X27_OFFSET 64
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_X28_OFFSET 72
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_X29_OFFSET 80
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_X30_OFFSET 88
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_SP_OFFSET 96
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_PC_OFFSET 104
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_D8_OFFSET 112
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_D9_OFFSET 120
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_D10_OFFSET 128
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_D11_OFFSET 136
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_D12_OFFSET 144
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_D13_OFFSET 152
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_D14_OFFSET 160
#define BESKID_ARCH_CONTEXT_AARCH64_DARWIN_D15_OFFSET 168
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_SIZE 240
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_ALIGNMENT 16
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_RBX_OFFSET 0
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_RBP_OFFSET 8
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_RDI_OFFSET 16
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_RSI_OFFSET 24
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_R12_OFFSET 32
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_R13_OFFSET 40
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_R14_OFFSET 48
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_R15_OFFSET 56
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_RSP_OFFSET 64
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_RIP_OFFSET 72
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_XMM6_OFFSET 80
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_XMM7_OFFSET 96
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_XMM8_OFFSET 112
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_XMM9_OFFSET 128
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_XMM10_OFFSET 144
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_XMM11_OFFSET 160
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_XMM12_OFFSET 176
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_XMM13_OFFSET 192
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_XMM14_OFFSET 208
#define BESKID_ARCH_CONTEXT_X86_64_WINDOWS_XMM15_OFFSET 224
#define BESKID_ARCH_CONTEXT_X86_64_SYS_V_SIZE 64
#define BESKID_ARCH_CONTEXT_X86_64_SYS_V_ALIGNMENT 16
#define BESKID_ARCH_CONTEXT_X86_64_SYS_V_RBX_OFFSET 0
#define BESKID_ARCH_CONTEXT_X86_64_SYS_V_RBP_OFFSET 8
#define BESKID_ARCH_CONTEXT_X86_64_SYS_V_R12_OFFSET 16
#define BESKID_ARCH_CONTEXT_X86_64_SYS_V_R13_OFFSET 24
#define BESKID_ARCH_CONTEXT_X86_64_SYS_V_R14_OFFSET 32
#define BESKID_ARCH_CONTEXT_X86_64_SYS_V_R15_OFFSET 40
#define BESKID_ARCH_CONTEXT_X86_64_SYS_V_RSP_OFFSET 48
#define BESKID_ARCH_CONTEXT_X86_64_SYS_V_RIP_OFFSET 56
int32_t beskid_library_attach_v5(void * runtime);
void beskid_library_detach_v5(void * runtime);
uint32_t beskid_rt_v5_abi_version(void);
void * beskid_rt_v5_array_allocate_rooted(void * request, void * root_handle_out);
uint8_t beskid_rt_v5_array_construction_finish(void * root_handle);
void * beskid_rt_v5_array_grow_rooted(void * array, size_t minimum_capacity, void * root_handle_out);
uint8_t beskid_rt_v5_array_write_barrier(void * array, void * value);
uint8_t beskid_rt_v5_closure_capture_store(void * environment, void * descriptor, size_t map_index, void * value);
void * beskid_rt_v5_closure_environment_allocate(void * request);
uint8_t beskid_rt_v5_closure_environment_root(void * tls_state, size_t slot_index, void * environment);
uint8_t beskid_rt_v5_closure_environment_root_current(size_t slot_index, void * environment);
int64_t beskid_rt_v5_fiber_spawn_with_cancel_slot(void * entry, void * environment, void * cancelled_slot);
void beskid_rt_v5_fiber_yield(void);
void * beskid_rt_v5_managed_object_allocate(void * request);
int32_t beskid_rt_v5_poll_executor_run_once(void);
int64_t beskid_rt_v5_poll_executor_spawn(void * poll_entry, void * task_state, void * result_slot, void * cancel_slot);
int32_t beskid_rt_v5_poll_executor_wake(int64_t task);
int64_t beskid_rt_v5_poll_link_clone(int64_t link);
void beskid_rt_v5_poll_link_drop(int64_t link);
int64_t beskid_rt_v5_poll_link_new(int64_t task);
int32_t beskid_rt_v5_poll_link_poll(int64_t link, void * result_slot);
void beskid_rt_v5_poll_monitor_drop(int64_t monitor);
int64_t beskid_rt_v5_poll_monitor_new(int64_t task);
int32_t beskid_rt_v5_poll_monitor_poll(int64_t monitor, void * result_slot);
void * beskid_rt_v5_process_init(void * config);
void beskid_rt_v5_process_shutdown(void * runtime);
void * beskid_rt_v5_thread_attach(void * runtime);
void beskid_rt_v5_thread_detach(void * thread);
_Noreturn void beskid_rt_v5_trap(uint8_t code, void * message, size_t message_len);
void beskid_arch_v5_context_init(void * context, void * stack_top, void * entry, void * argument, void * return_trampoline);
void beskid_arch_v5_context_switch(void * from, void * to);
void * alloc(size_t size, void * descriptorPtr);
int64_t beskid_rt_v5_args_count(void);
struct BeskidStr * beskid_rt_v5_args_get(int64_t index);
size_t array_len(void * array);
void * array_new(size_t elementSize, size_t count);
void beskid_register_callbacks(void * entries, size_t count);
void beskid_register_handlers(void * entries, size_t count);
int32_t bytes_compare(void * left, size_t leftLen, void * right, size_t rightLen);
void bytes_copy(void * destination, void * source, size_t count);
void * bytes_from_str(void * value);
uint8_t bytes_get(void * bytes, size_t index);
void bytes_set(void * bytes, size_t index, uint8_t value);
void channel_close(int64_t id);
int64_t channel_create(int64_t capacity, int64_t flags);
void * channel_receive_ptr(int64_t id);
int64_t channel_receive_status(int64_t id);
int64_t channel_receive_value(int64_t id);
int64_t channel_send(int64_t id, int64_t value);
int64_t channel_send_ptr(int64_t id, void * valuePtr);
uint8_t channel_try_receive(int64_t id);
void * channel_try_receive_ptr(int64_t id);
int64_t channel_try_send(int64_t id, int64_t value);
int64_t channel_try_send_ptr(int64_t id, void * valuePtr);
int64_t clock_monotonic_nanos(void);
int64_t clock_realtime_nanos(void);
void * composition_container_create(size_t slot_count);
void composition_container_drop(void * container);
uint8_t composition_launch(void * container);
int32_t composition_scope_depth(void);
void composition_scope_enter(void * container);
void composition_scope_leave(void);
void composition_shutdown(void * container);
uint8_t composition_slot_store(void * container, size_t slot, void * service);
int32_t dynamic_cast_checked(void * cell, int32_t expectedShapeId);
void * dynamic_cell_create(void * value, void * descriptor);
void * dynamic_cell_wrap(void * value, void * descriptor);
void * dynamic_map_aot(void * cell, void * mapping);
void * dynamic_map_fallback(void * cell, void * mapping);
void * dynamic_object_alloc(void * descriptor);
void * env_get(void * key);
void * env_getcwd(void);
int32_t env_set(void * key, void * value);
void * event_get_handler(void * event, uint32_t index);
size_t event_len(void * event);
size_t event_subscribe(void * eventSlot, void * handler, size_t capacity);
size_t event_unsubscribe_first(void * eventSlot, void * handler);
uint8_t fiber_cancel(int64_t fiberId);
int64_t fiber_current_id(void);
void fiber_detach(int64_t fiberId);
int32_t fiber_join_status(int64_t fiberId);
int64_t fiber_join_value(int64_t fiberId);
int64_t fiber_now_millis(void);
size_t fiber_processor_count(void);
int64_t fiber_spawn(void * entry, void * argument);
int32_t beskid_rt_v5_fs_delete(void * path);
int32_t beskid_rt_v5_fs_exists(void * path);
int32_t beskid_rt_v5_fs_mkdir(void * path);
int32_t beskid_rt_v5_fs_read_text(void * path, void * text_out);
int32_t beskid_rt_v5_fs_write_text(void * path, void * text);
size_t gc_bytes_allocated(void);
size_t gc_collect(void);
size_t gc_collect_if_needed(void);
size_t gc_external_root_count(void);
size_t gc_object_count(void);
size_t gc_phase(void);
uint8_t gc_register_root(void * ptrAddr);
size_t gc_root_handle(void * valuePtr);
void gc_unregister_root(void * ptrAddr);
void gc_unroot_handle(size_t handle);
void gc_write_barrier(void * parent, void * child);
int64_t hub_create(void);
int64_t hub_register(int64_t hubId, int64_t index, int64_t channelId);
int64_t hub_unregister(int64_t hubId, int64_t index);
int64_t hub_wait_receive_index(int64_t hubId);
int64_t hub_wait_receive_status(int64_t hubId);
int64_t hub_wait_receive_value(int64_t hubId);
void * mutex_create(void);
int32_t mutex_lock(void * id);
int32_t mutex_try_lock(void * id);
void mutex_unlock(void * id);
void panic(int64_t code);
void panic_str(void * message, size_t len);
void process_exit(int32_t code);
int32_t process_getpid(void);
void runtime_preempt_check(void);
void * str_concat(void * left, void * right);
size_t str_eq(void * left, void * right);
void * str_from_bytes_utf8(void * bytes, size_t len);
void * str_from_i64(int64_t value);
size_t str_len(void * value);
void * str_new(void * data, size_t length);
void * str_slice(void * value, size_t start, size_t count);
int64_t syscall_read(int32_t fd, void * buffer, size_t len);
int64_t syscall_read_bytes(int32_t fd, void * data, size_t len);
int64_t syscall_write(int32_t fd, void * buffer, size_t len);
int64_t syscall_write_bytes(int32_t fd, void * data, size_t len);
void * tty_winsize(void);
void wait_group_add(void * id, int64_t delta);
void * wait_group_create(void);
void wait_group_done(void * id);
int32_t wait_group_wait(void * id);
#endif
