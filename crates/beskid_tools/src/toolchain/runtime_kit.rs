//! Hermetic production of installed ABI-v5 runtime kits.

mod build;
mod matrix;
mod model;
mod native_host;
mod verification;

pub use build::build;
pub use matrix::build_matrix;
pub use model::{RuntimeKitBuildOptions, RuntimeKitMatrixBuildOptions, RuntimeKitProfile, RuntimeKitProfileArtifacts};
pub use native_host::build_native_host;

#[cfg(test)]
mod tests;
