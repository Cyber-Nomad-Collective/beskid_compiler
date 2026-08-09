//! Cross-unit module graph built by collection-only resolver passes.

use super::{ProgramAssembly, hir_units};

mod build;
mod discovery;
mod model;
mod path_inference;
mod resolution;

#[cfg(test)]
mod tests;

pub use self::model::ModuleIndex;
pub use self::path_inference::infer_logical_module_path;
