//! Per-unit prepare-spine operations (assemble, resolve entry, type entry).

use std::path::Path;

use crate::hir::HirProgram;
use crate::projects::assembly::{
    ModuleIndex, ProgramAssembly, SourceUnit, UnitHir, build_hir_units,
};
use crate::projects::CompilePlan;
use crate::resolve::Resolution;
use crate::services::lower::{
    DependencyTypingPolicy, LowerResolveTypeError, TypedHirResolution, typed_hir_from_lowered,
};
use crate::syntax::Spanned;
use crate::types::TypeResult;

/// Query: assemble one unit's HIR from source (invalidated by content hash).
pub fn assemble_unit(source_unit: &SourceUnit) -> UnitHir {
    build_hir_units(std::slice::from_ref(source_unit))
        .into_iter()
        .next()
        .expect("single unit hir")
}

/// Query: resolve entry HIR against module index.
pub fn resolve_entry(
    entry_hir: &Spanned<HirProgram>,
    module_index: &ModuleIndex,
    entry_source_path: Option<&Path>,
) -> Result<Resolution, LowerResolveTypeError> {
    module_index
        .resolve_entry_hir(
            entry_hir,
            entry_source_path.map(|path| path.to_path_buf()).as_ref(),
        )
        .map_err(LowerResolveTypeError::Resolve)
}

/// Query: type entry program with full dependency bodies (executable path).
pub fn type_entry(
    entry_hir: Spanned<HirProgram>,
    assembly: &ProgramAssembly,
) -> Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    typed_hir_from_lowered(
        entry_hir,
        TypedHirResolution::Assembly(Some(assembly)),
        None,
        DependencyTypingPolicy::FullClosure,
    )
}

/// Query: type entry for semantic gate (dependency signatures only).
pub fn type_entry_gate(
    entry_hir: Spanned<HirProgram>,
    assembly: &ProgramAssembly,
) -> Result<(Spanned<HirProgram>, Resolution, TypeResult), LowerResolveTypeError> {
    typed_hir_from_lowered(
        entry_hir,
        TypedHirResolution::Assembly(Some(assembly)),
        None,
        DependencyTypingPolicy::EntryOnly,
    )
}

/// Query: prefetch dependency signatures without typing dependency bodies.
pub fn type_dep_signatures(
    assembly: &ProgramAssembly,
) -> Result<TypeResult, LowerResolveTypeError> {
    let entry_unit = assembly.entry_unit().clone();
    let entry_hir = assemble_unit(&entry_unit).hir;
    let (_, _, typed) =
        typed_hir_from_lowered(
            entry_hir,
            TypedHirResolution::Assembly(Some(assembly)),
            None,
            DependencyTypingPolicy::EntryOnly,
        )?;
    Ok(typed)
}

/// Query: build module index from assembled units.
pub fn module_index_query(
    units: &[SourceUnit],
    hir_units: &[UnitHir],
    entry_index: usize,
    roots: &crate::projects::assembly::EffectiveCompilationRoots,
    plan: &CompilePlan,
    prefetch_dependency_roots: bool,
) -> ModuleIndex {
    ModuleIndex::build(
        units,
        hir_units,
        entry_index,
        roots,
        plan,
        prefetch_dependency_roots,
    )
}

/// Invalidate dependent units when imports change (BFS over import edges).
pub fn invalidate_dependents(
    changed_fingerprint: &str,
    import_edges: &[(String, Vec<String>)],
) -> Vec<String> {
    let mut invalidated = vec![changed_fingerprint.to_string()];
    let mut queue = vec![changed_fingerprint.to_string()];
    while let Some(current) = queue.pop() {
        for (unit, imports) in import_edges {
            if imports.iter().any(|dep| dep == &current) && !invalidated.contains(unit) {
                invalidated.push(unit.clone());
                queue.push(unit.clone());
            }
        }
    }
    invalidated
}
