//! Public AOT API: build requests, output kinds, and the [`build`] orchestration entry point.

mod host_emit;
mod model;
mod object_stage;
mod pipeline;
mod platform_objects;
mod validation;

pub use host_emit::{
    emit_host_context_library_pair, emit_host_platform_library_pair, require_canonical_host_emit_authority,
};
pub use model::{
    AotBuildRequest, AotBuildResult, BuildOutputKind, BuildProfile, CanonicalHostEmitAuthority, ExportPolicy, LinkMode,
    NativeLibraryPair, NativeSymbolInventory, ProjectTargetKind, RuntimeKitRequest,
};
pub use pipeline::{build, emit_object_only};
pub use validation::{DEFAULT_ENTRYPOINT, default_output_kind, native_link_entrypoint, resolve_entrypoint};
