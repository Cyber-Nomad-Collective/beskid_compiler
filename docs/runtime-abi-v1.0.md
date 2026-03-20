# Runtime ABI v1.0

Source of truth: `crates/beskid_abi/src/symbols.rs`.

## Exported symbols
- `beskid_runtime_abi_version`
- `alloc`
- `str_new`
- `str_concat`
- `str_len`
- `array_new`
- `panic`
- `panic_str`
- `sys_print`
- `sys_println`
- `gc_write_barrier`
- `gc_root_handle`
- `gc_unroot_handle`
- `gc_register_root`
- `gc_unregister_root`
- `event_subscribe`
- `event_unsubscribe_first`
- `event_len`
- `event_get_handler`
- `interop_dispatch_unit`
- `interop_dispatch_ptr`
- `interop_dispatch_usize`
- `test_bytes_ptr`
- `test_bytes_len`

## Versioning
- ABI version value is defined by `BESKID_RUNTIME_ABI_VERSION`.
- Any breaking symbol/signature change requires version bump and snapshot update.
