use std::sync::Arc;

use beskid_analysis::projects::SyntaxProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;

use crate::{BeskidDatabase, Db, ProjectSession, SemanticError, SourceUnitId, TypedProgram};

/// Register an expanded syntax assembly as one generation-safe typed-program identity.
pub fn build_typed_program(
    db: &mut BeskidDatabase,
    project: ProjectSession,
    generation: SyntaxGenerationId,
    assembly: Arc<SyntaxProgramAssembly>,
) -> Result<TypedProgram, SemanticError> {
    let entry_unit = assembly
        .units
        .get(assembly.entry_index)
        .ok_or_else(|| SemanticError::new("syntax assembly has no valid entry unit"))?;

    for unit in assembly.units.iter() {
        let identity = SourceUnitId::new(db, unit.path.clone());
        db.ensure_expanded_syntax_unit(
            project,
            identity,
            generation,
            unit.source.clone(),
            Arc::new(unit.program.clone()),
        )?;
    }

    let module_units = assembly
        .units
        .iter()
        .filter_map(|unit| {
            beskid_analysis::projects::infer_logical_module_path(
                unit,
                &assembly.roots,
                assembly.has_std_dependency,
            )
            .map(|module_path| (module_path, SourceUnitId::new(db, unit.path.clone())))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let mut registry = db
        .syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry");
    for unit in assembly.units.iter() {
        let unit_id = SourceUnitId::new(db, unit.path.clone());
        let imports = unit
            .program
            .node
            .items
            .iter()
            .filter_map(|item| match &item.node {
                beskid_analysis::syntax::Node::UseDeclaration(declaration) => Some(
                    declaration
                        .node
                        .path
                        .node
                        .segments
                        .iter()
                        .map(|segment| segment.node.name.node.name.clone())
                        .collect(),
                ),
                _ => None,
            })
            .filter_map(|path| {
                module_units
                    .get(&path)
                    .copied()
                    .map(|target| crate::db::SyntaxImport { path, target })
            })
            .collect();
        registry.imports.insert((unit_id, generation), imports);
    }

    Ok(TypedProgram {
        project,
        entry: SourceUnitId::new(db, entry_unit.path.clone()),
        generation,
        assembly,
    })
}
