use std::collections::HashMap;
use std::path::Path;

use crate::resolve::{ItemId, Resolution};
use crate::syntax::Program;
use crate::syntax::Spanned;

use super::builder::TypeSurfaceBuilder;
use super::model::{FunctionBound, UnitTypeSurface};

/// Build the exported type surface for one unit without walking function bodies.
pub fn build_unit_type_surface(
    program: &Spanned<Program>,
    resolution: &Resolution,
    source_path: &Path,
) -> UnitTypeSurface {
    let mut builder = TypeSurfaceBuilder::new(resolution, source_path);
    builder.walk_program(program);
    builder.finish()
}

/// Free-function where-bound provenance for a program resolved without a source path.
///
/// A source-less single-program resolution has no unit surface, so the checker would otherwise
/// see no bound facts and reject every `where V: Contract` clause. The bounds use the same
/// declaring-module contract lookup as a unit surface. The empty source key matches no
/// source-scoped table entry, so a qualified bound falls back to the module-graph path lookup.
pub fn source_less_function_bounds(
    program: &Spanned<Program>,
    resolution: &Resolution,
) -> HashMap<ItemId, Vec<FunctionBound>> {
    let builder = TypeSurfaceBuilder::new(resolution, Path::new(""));
    let mut bounds = HashMap::new();
    builder.collect_function_bounds(&program.node.items, &mut bounds);
    bounds
}
