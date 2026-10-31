//! Integration tests for `beskid_engine`, in one test process.
//!
//! Suites that re-run this executable as a child process name their child by its full test path
//! (`external_wait_native::...`, `fiber_value_transfer::...`). Release scripts select the
//! staged-kit proofs by their full test path in this binary (`--test engine`).

mod support;

#[cfg(unix)]
mod abi_v5_jit_runtime_kit;
mod extern_frontend_validation;
#[cfg(target_os = "linux")]
mod extern_tests;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
mod external_wait_native;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
mod fiber_value_transfer;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]
mod foundation_io_native;
mod native_kit;
mod native_runtime_kit_smoke;
