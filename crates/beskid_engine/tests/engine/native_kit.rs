//! Engine suites that consume the shared canonical native runtime kit.
//!
//! Publishing the canonical ABI-v5 runtime kit lowers the embedded runtime corpus, which costs
//! tens of seconds in every process that does it. These suites only consume a kit, so they use
//! the kit from `support::shared_kit`, whose lease keeps two engines from initializing the same
//! loaded runtime at once. Each module keeps its own platform gate.

mod artifact_runtime_reset;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
mod bulk_array_of_native;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
mod contract_dispatch_native;
mod corelib_repeat_jit;
mod descriptor_smoke;
mod enum_return_across_call;
#[cfg(unix)]
mod gc_stress_native;
mod generic_field_match_native;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
mod generic_for_values_native;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
mod generic_function_values_native;
mod generic_try_specialization_native;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
mod heap_growth_native;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
mod incremental_marking_native;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
mod iterator_for_native;
mod jit_pipeline_observer;
mod mod_host;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
mod parallel_counts_native;
mod repeat_loop_probe;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
mod scoped_cleanup_native;
mod spawn_scheduler;
mod struct_literal_enum_field_native;
#[cfg(unix)]
mod traced_abi_value_native;
mod unary_field_write_native;
#[cfg(unix)]
mod use_after_collection_native;
