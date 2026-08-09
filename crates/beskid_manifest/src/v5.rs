//! ABI-v5 runtime manifest parsing and deterministic multi-target generation.

mod artifacts;
mod model;
mod parsing;
mod render;
mod validation;

pub use artifacts::{generate_v5_artifacts, write_v5_artifacts};
pub use model::{GeneratedV5Artifacts, RuntimeManifestV5};
pub use parsing::load_v5_manifest_source;
