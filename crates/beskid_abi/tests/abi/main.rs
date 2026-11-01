//! Integration tests for `beskid_abi`: one module per topic. Platform-specific modules are
//! compiled only on the hosts that can run them.

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod aarch64_darwin_context_assembly;
mod abi_v5_contract;
mod canonical_array_growth_sources;
mod canonical_callback_sources;
mod canonical_composition_sources;
mod canonical_event_sources;
mod canonical_hub_sources;
mod canonical_runtime_sources;
mod canonical_waitgroup_sources;
mod composition_runtime_imports;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod core_args_native_behavior;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
mod external_owner_transport;
mod filesystem_vertical_seam;
mod glue_owner_issuer_contract;
mod glue_provider_publication;
mod glue_shared_provider;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod guarded_stack_adapter;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
mod host_threads_native;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
mod network_iocp_settle;
mod platform_intrinsic_registration;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
mod process_transport;
mod runtime_bootstrap_contract;
mod runtime_import_catalog;
mod runtime_kit_build;
mod runtime_kit_resolution;
mod runtime_provenance_audit;
mod runtime_provenance_cli;
mod scheduler_sources;
#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), all(target_os = "macos", target_arch = "aarch64")))]
mod x86_64_linux_context_assembly;
#[cfg(any(target_os = "macos", target_os = "linux", all(target_os = "windows", target_env = "msvc")))]
mod x86_64_windows_context_assembly;
