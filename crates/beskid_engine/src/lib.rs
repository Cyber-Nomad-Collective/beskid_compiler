//! JIT execution host: compile a [`beskid_codegen::CodegenArtifact`] with Cranelift-JIT and run entrypoints under the GC arena.

mod engine;
mod jit_callable;
mod jit_module;
pub mod link_libraries;
pub mod services;

pub use engine::Engine;
pub use services::run_resolved_entrypoint_with_pipeline;
#[cfg(feature = "extern_dlopen")]
pub use engine::resolve_for_tests;
#[cfg(feature = "extern_dlopen")]
pub use engine::set_security_policies_for_tests;
pub use jit_module::{BeskidJitModule, JitError};
