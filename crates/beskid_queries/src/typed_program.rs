use std::sync::Arc;

use beskid_abi::runtime_source::RuntimeIntrinsicCapability;
use beskid_analysis::projects::SyntaxProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;

use crate::{BeskidDatabase, Db, ProjectSession, SemanticError, SourceUnitId, TypedProgram};

/// Return the existing owner of a prepared syntax assembly when it has already
/// been registered in this database, otherwise mint the first owner for it.
///
/// Prepared frontends and their syntax consumers share a database. Reusing the
/// recorded owner is therefore required to preserve the one-project-per-source
/// invariant across test discovery, REPL inspection, and code generation.
pub fn project_session_for_syntax_assembly(
    db: &BeskidDatabase,
    assembly: &SyntaxProgramAssembly,
    fallback_target_name: &str,
    fallback_lockfile_digest: &str,
) -> Result<ProjectSession, SemanticError> {
    let mut owner = None;
    for unit in assembly.units() {
        let unit = SourceUnitId::new(db, unit.path.clone());
        let Some(input) = db.syntax_unit(unit) else {
            continue;
        };
        let candidate = input.project(db);
        if let Some(existing) = owner {
            if existing != candidate {
                return Err(SemanticError::new(
                    "prepared syntax assembly contains source units from different project sessions",
                ));
            }
        } else {
            owner = Some(candidate);
        }
    }

    Ok(owner.unwrap_or_else(|| {
        ProjectSession::new(
            db,
            assembly.roots().host.source_root.clone(),
            assembly.entry_unit().path.clone(),
            fallback_target_name.into(),
            fallback_lockfile_digest.into(),
        )
    }))
}

/// Register an expanded syntax assembly as one generation-safe typed-program identity.
pub fn build_typed_program(
    db: &mut BeskidDatabase,
    project: ProjectSession,
    generation: SyntaxGenerationId,
    assembly: Arc<SyntaxProgramAssembly>,
) -> Result<TypedProgram, SemanticError> {
    let entry_unit = assembly
        .units()
        .get(assembly.entry_index())
        .ok_or_else(|| SemanticError::new("syntax assembly has no valid entry unit"))?;

    for unit in assembly.units() {
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
        .units()
        .iter()
        .filter_map(|unit| {
            beskid_analysis::projects::infer_logical_module_path(
                unit,
                assembly.roots(),
                assembly.has_std_dependency(),
            )
            .map(|module_path| (module_path, SourceUnitId::new(db, unit.path.clone())))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let mut registry = db
        .syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry");
    for unit in assembly.units() {
        let unit_id = SourceUnitId::new(db, unit.path.clone());
        let imports = unit
            .program
            .node
            .items
            .iter()
            .filter_map(|item| match &item.node {
                beskid_analysis::syntax::Node::UseDeclaration(declaration) => {
                    let path = declaration
                        .node
                        .path
                        .node
                        .segments
                        .iter()
                        .map(|segment| segment.node.name.node.name.clone())
                        .collect::<Vec<_>>();
                    let binding = declaration
                        .node
                        .alias
                        .as_ref()
                        .map(|alias| alias.node.name.clone())
                        .or_else(|| path.last().cloned())?;
                    Some((
                        path,
                        binding,
                        declaration.node.visibility.node
                            == beskid_analysis::syntax::Visibility::Public,
                    ))
                }
                _ => None,
            })
            .filter_map(|(path, binding, public)| {
                module_units
                    .get(&path)
                    .copied()
                    .map(|target| crate::db::SyntaxImport {
                        path,
                        binding,
                        target,
                        public,
                    })
            })
            .collect();
        registry.imports.insert((unit_id, generation), imports);
    }

    Ok(TypedProgram {
        project,
        entry: SourceUnitId::new(db, entry_unit.path.clone()),
        generation,
        assembly,
        runtime_intrinsic_capability: None,
    })
}

/// Attach compiler-minted runtime intrinsic authority after validating that the assembled syntax
/// is the exact embedded canonical corpus. This is deliberately separate from the ordinary
/// assembly constructor so package names, paths, and user source cannot acquire the capability.
pub fn build_canonical_runtime_typed_program(
    db: &mut BeskidDatabase,
    project: ProjectSession,
    generation: SyntaxGenerationId,
    assembly: Arc<SyntaxProgramAssembly>,
    capability: RuntimeIntrinsicCapability,
) -> Result<TypedProgram, SemanticError> {
    let expected = beskid_abi::runtime_source::canonical_runtime_sources();
    let actual = assembly
        .units()
        .iter()
        .map(|unit| beskid_abi::abi_v5::SourceUnit {
            logical_path: unit.logical_name.clone(),
            source: unit.source.clone(),
        })
        .collect::<Vec<_>>();
    let exact_corpus = actual.len() == expected.len()
        && actual.iter().all(|source| {
            capability.authorizes_source(&source.logical_path)
                && expected.iter().any(|expected| expected == source)
        });
    if !exact_corpus {
        return Err(SemanticError::new(
            "syntax assembly is not the compiler-embedded canonical runtime corpus",
        ));
    }

    let mut typed = build_typed_program(db, project, generation, assembly)?;
    typed.runtime_intrinsic_capability = Some(Arc::new(capability));
    Ok(typed)
}
