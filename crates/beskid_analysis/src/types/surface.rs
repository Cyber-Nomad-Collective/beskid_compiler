//! Per-unit exported type shapes and merged dependency views for entry checking.

mod build;
mod builder;
mod merge;
mod model;

pub use build::{build_entry_unit_type_surface, build_unit_type_surface, source_less_function_bounds};
pub use merge::{contract_signatures_for_types, merge_unit_surfaces, merge_unit_surfaces_with_types};
pub use model::{FunctionBound, MergedTypeEnv, UnitTypeSurface};

#[cfg(test)]
mod tests;
