use std::path::Path;

use crate::syntax::Program;
use crate::resolve::Resolution;
use crate::syntax::Spanned;

use super::builder::TypeSurfaceBuilder;
use super::model::UnitTypeSurface;

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
