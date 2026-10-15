//! Engine suites that consume the canonical native runtime kit, in one test process.
//!
//! Publishing the canonical ABI-v5 runtime kit lowers the embedded runtime corpus, which costs
//! tens of seconds in every process that does it. These suites only consume a kit, so they share
//! one binary and the kit from `shared_kit`, whose lease keeps two engines from initializing the
//! same loaded runtime at once. Each module keeps its own platform gate.
//!
//! Suites that re-run their own executable as a child process (`fiber_value_transfer`,
//! `external_wait_native`), or that release scripts call by binary name (`foundation_io_native`,
//! `native_runtime_kit_smoke`), stay separate binaries.

#[path = "../support/runtime_prefix.rs"]
mod runtime_prefix;
#[path = "../support/shared_kit.rs"]
mod shared_kit;

mod artifact_runtime_reset;
mod bulk_array_of_native;
mod corelib_repeat_jit;
mod cyb169_enum_return_sigill;
mod descriptor_smoke;
mod gc_stress_native;
mod generic_field_match_native;
mod generic_function_values_native;
mod generic_try_specialization_native;
mod heap_growth_native;
mod incremental_marking_native;
mod iterator_for_native;
mod jit_pipeline_observer;
mod mod_host;
mod repeat_loop_probe;
mod scoped_cleanup_native;
mod spawn_scheduler;
mod struct_literal_enum_field_native;
mod traced_abi_value_native;
mod unary_field_write_native;
mod use_after_collection_native;
