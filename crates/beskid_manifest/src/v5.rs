//! ABI-v5 runtime manifest parsing and deterministic multi-target generation.

mod artifacts;
mod model;
mod parsing;
mod render;
mod validation;

pub use artifacts::{generate_v5_artifacts, write_v5_artifacts};
pub use model::{
    AssemblyV5, AuditV5, CorelibServiceV5, EntryAdapterV5, FieldV5, FunctionV5, GeneratedV5Artifacts, IntrinsicV5,
    LayoutV5, ParameterLocationV5, ParameterV5, PlatformImportV5, RuntimeManifestV5, RuntimeMetaV5, SoftBuiltinV5,
    TargetAdapterBindingV5, TargetV5, TrapV5,
};
pub use parsing::load_v5_manifest_source;
