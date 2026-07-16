//! Ahead-of-time compilation: Cranelift object emission, optional Beskid runtime linking, and
//! host linker invocation for executables, static libraries, and shared objects.
//!
//! Consumers typically call [`api::build`] with an [`api::AotBuildRequest`] built from a
//! [`beskid_codegen::CodegenArtifact`].

pub mod api;
pub mod bundled;
pub mod error;
pub mod export_table;
pub mod linker;
pub mod mod_artifact;
pub mod object_module;
pub mod prepared_syntax;
pub mod run;
pub mod runtime;
pub mod target;

pub use api::{
    AotBuildRequest, AotBuildResult, BuildOutputKind, BuildProfile, ExportPolicy, LinkMode, NativeLibraryPair, emit_library_pair,
    ProjectTargetKind, RuntimeKitRequest, build, default_output_kind, emit_object_only,
    resolve_entrypoint,
};
pub use beskid_abi::BESKID_RUNTIME_ABI_VERSION;
pub use beskid_pipeline::SharedPipelineObserver;
pub use bundled::{
    default_runtime_strategy, installed_runtime_strategy, resolve_installed_runtime_archive,
};
pub use error::{AotError, AotResult};
pub use export_table::{ExportTable, ExportTableEntry};
pub use mod_artifact::{
    ContractRegistration, ModArtifactBuildRequest, ModArtifactDescriptor, build_mod_artifact,
    compute_mod_artifact_key, mod_artifact_dir,
};
pub use prepared_syntax::lower_prepared_syntax_entrypoint;
pub use run::{AotRunRequest, AotRunResult, build_and_run, run_linked_executable};
