//! Ahead-of-time compilation: Cranelift object emission, optional Beskid runtime linking, and
//! host linker invocation for executables, static libraries, and shared objects.
//!
//! Consumers typically call [`api::build`] with an [`api::AotBuildRequest`] built from a
//! [`beskid_codegen::CodegenArtifact`].

pub mod api;
pub mod bundled;
pub mod error;
pub mod linker;
pub mod object_module;
pub mod runtime;
pub mod target;

pub use api::{
    AotBuildRequest, AotBuildResult, BuildOutputKind, BuildProfile, ExportPolicy, LinkMode,
    ProjectTargetKind, RuntimeStrategy, build, default_output_kind, emit_object_only,
    resolve_entrypoint,
};
pub use bundled::{default_runtime_strategy, resolve_bundled_runtime_archive};
pub use beskid_abi::BESKID_RUNTIME_ABI_VERSION;
pub use beskid_pipeline::SharedPipelineObserver;
pub use error::{AotError, AotResult};
