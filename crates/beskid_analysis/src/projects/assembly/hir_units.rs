//! Per-compilation-unit HIR cache shared by resolution, typecheck, and codegen.

use std::path::PathBuf;

use crate::hir::{AstProgram, HirProgram, lower_program as lower_hir_program};
use crate::syntax::{Program, Spanned};

use super::SourceUnit;

/// Lowered HIR for one assembled unit (parallel to [`SourceUnit`] by index).
pub struct UnitHir {
    pub path: PathBuf,
    pub hir: Spanned<HirProgram>,
}

/// Lower every unit's AST once (deterministic spans for resolve/codegen).
pub fn build_hir_units(units: &[SourceUnit]) -> Vec<UnitHir> {
    units
        .iter()
        .map(|unit| UnitHir {
            path: unit.path.clone(),
            hir: unit_to_hir(&unit.program),
        })
        .collect()
}

pub(crate) fn unit_to_hir(program: &Spanned<Program>) -> Spanned<HirProgram> {
    let ast: Spanned<AstProgram> = program.clone().into();
    lower_hir_program(&ast)
}
