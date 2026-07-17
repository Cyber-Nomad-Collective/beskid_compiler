//! JIT execution host for interactive snippet evaluation and the interim `beskid test` runner.
//!
//! Production `beskid run` and integration tests use AOT via [`beskid_aot`]. This crate keeps
//! Cranelift-JIT for low-latency REPL sessions ([`beskid_repl`]) and in-process test discovery
//! until a multi-entrypoint AOT test runner lands in phase 2.

mod engine;
mod jit_callable;
mod jit_module;
pub mod link_libraries;
mod runtime_kit;
pub mod services;

pub use engine::Engine;
pub use engine::host_runtime_target;
#[cfg(feature = "extern_dlopen")]
pub use engine::resolve_for_tests;
#[cfg(feature = "extern_dlopen")]
pub use engine::set_security_policies_for_tests;
pub use jit_module::{BeskidJitModule, JitError};
pub use runtime_kit::JitRuntimeKit;
