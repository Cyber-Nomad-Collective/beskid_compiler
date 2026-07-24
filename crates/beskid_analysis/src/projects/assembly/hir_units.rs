//! Per-compilation-unit HIR cache shared by resolution, typecheck, and codegen.

use std::path::PathBuf;

use crate::hir::index::reset_program_node_ids;
use crate::hir::{AstProgram, HirProgram, index_program_from_base, lower_program as lower_hir_program};
use crate::syntax::{Program, Spanned};

use super::SourceUnit;

/// Lowered HIR for one assembled unit (parallel to [`SourceUnit`] by index).
pub struct UnitHir {
    pub path: PathBuf,
    pub hir: Spanned<HirProgram>,
}

/// Lower every unit's AST once (deterministic spans for resolve/codegen).
pub fn build_hir_units(units: &[SourceUnit]) -> Vec<UnitHir> {
    units.iter().map(|unit| UnitHir { path: unit.path.clone(), hir: unit_to_hir(&unit.program) }).collect()
}

/// Assign globally unique [`HirNodeId`] values across an assembled unit list.
pub fn reindex_hir_units_in_place(units: &mut [UnitHir]) {
    let mut base = 0u32;
    for unit in units {
        reset_program_node_ids(&mut unit.hir);
        base = index_program_from_base(&mut unit.hir, base);
    }
}

pub(crate) fn unit_to_hir(program: &Spanned<Program>) -> Spanned<HirProgram> {
    let ast: Spanned<AstProgram> = program.clone().into();
    lower_hir_program(&ast)
}
