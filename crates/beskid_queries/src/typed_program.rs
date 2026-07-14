use std::sync::Arc;

use beskid_analysis::projects::SyntaxProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;

use crate::{BeskidDatabase, ProjectSession, SemanticError, SourceUnitId, TypedProgram};

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

    Ok(TypedProgram {
        project,
        entry: SourceUnitId::new(db, entry_unit.path.clone()),
        generation,
        assembly,
    })
}
