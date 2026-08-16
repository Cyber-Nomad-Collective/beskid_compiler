//! Per-unit prepare-spine operations (assemble, resolve entry, type entry).

use std::path::Path;

use crate::projects::CompilePlan;
use crate::projects::assembly::{ModuleIndex, ProgramAssembly, SourceUnit};
use crate::resolve::Resolution;
use crate::services::semantic_facts::{
    DependencyTypingPolicy, ProgramResolutionSource, SemanticFactsError, type_resolved_program,
};
use crate::syntax::Program;
use crate::syntax::Spanned;
use crate::syntax_query::SyntaxIndex;
use crate::types::surface::{UnitTypeSurface, build_unit_type_surface};
use crate::types::{CheckerResult, TypeChecker, TypeResult};

/// Query: exported signatures and layouts for one unit (EntryOnly surface pass).
pub fn type_unit_signatures(
    program: &Spanned<Program>,
    resolution: &Resolution,
    source_path: &Path,
) -> UnitTypeSurface {
    build_unit_type_surface(program, resolution, source_path)
}

/// Query: type callable bodies in one unit using a pre-built surface (FullClosure body slice).
pub fn type_unit_body(
    program: &Spanned<Program>,
    resolution: &Resolution,
    surface: &UnitTypeSurface,
    source_path: &Path,
) -> CheckerResult {
    let mut checker = TypeChecker::new(resolution, surface).with_source_path(source_path);
    checker.type_callable_items(&program.node.items);
    checker.finish()
}

/// Query: assemble one unit's syntax from source (invalidated by content hash).
pub fn assemble_unit(source_unit: &SourceUnit) -> SourceUnit {
    source_unit.clone()
}

/// Query: resolve entry syntax against module index.
pub fn resolve_entry(
    entry_program: &Spanned<Program>,
    assembly: &ProgramAssembly,
    entry_source_path: Option<&Path>,
) -> Result<Resolution, SemanticFactsError> {
    assembly
        .module_index
        .resolve_entry_program(entry_program, entry_source_path, assembly)
        .map_err(SemanticFactsError::Resolve)
}

/// Query: type entry program with full dependency bodies (executable path).
pub fn type_entry(
    entry_program: Spanned<Program>,
    assembly: &ProgramAssembly,
) -> Result<(Spanned<Program>, Resolution, TypeResult), SemanticFactsError> {
    type_resolved_program(
        entry_program,
        ProgramResolutionSource::Assembly(Some(assembly)),
        None,
        DependencyTypingPolicy::FullClosure,
    )
}

/// Query: type entry for semantic gate (dependency signatures only).
pub fn type_entry_gate(
    entry_program: Spanned<Program>,
    assembly: &ProgramAssembly,
) -> Result<(Spanned<Program>, Resolution, TypeResult), SemanticFactsError> {
    type_resolved_program(
        entry_program,
        ProgramResolutionSource::Assembly(Some(assembly)),
        None,
        DependencyTypingPolicy::EntryOnly,
    )
}

/// Query: prefetch dependency signatures without typing dependency bodies.
pub fn type_dep_signatures(assembly: &ProgramAssembly) -> Result<TypeResult, SemanticFactsError> {
    let entry_unit = assembly.entry_unit().clone();
    let entry_program = assemble_unit(&entry_unit).program;
    let (_, _, typed) = type_resolved_program(
        entry_program,
        ProgramResolutionSource::Assembly(Some(assembly)),
        None,
        DependencyTypingPolicy::EntryOnly,
    )?;
    Ok(typed)
}

/// Query: build module index from assembled units.
pub fn module_index_query(
    units: &[SourceUnit],
    syntax_indexes: &[SyntaxIndex],
    roots: &crate::projects::assembly::EffectiveCompilationRoots,
    plan: &CompilePlan,
) -> ModuleIndex {
    ModuleIndex::build(units, syntax_indexes, roots, plan)
}

/// Invalidate dependent units when imports change (BFS over import edges).
pub fn invalidate_dependents(changed_fingerprint: &str, import_edges: &[(String, Vec<String>)]) -> Vec<String> {
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
