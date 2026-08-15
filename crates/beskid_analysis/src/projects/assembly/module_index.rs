//! Cross-unit module graph built from expanded syntax facts.

mod build;
mod discovery;
mod model;
mod path_inference;
mod resolution;

#[cfg(test)]
mod tests;

pub use self::model::{AssemblyModule, ModuleGraph, ModuleIndex};
pub use self::path_inference::infer_logical_module_path;
