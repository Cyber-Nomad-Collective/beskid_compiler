use std::sync::Arc;

use beskid_abi::runtime_source::{
    CorelibService, CorelibServiceCapability, RuntimeIntrinsicCapability,
    canonical_corelib_service_source_path, canonical_corelib_service_sources,
};
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
        .fold(
            std::collections::HashMap::<Vec<String>, Vec<SourceUnitId>>::new(),
            |mut modules, (path, unit)| {
                let units = modules.entry(path).or_default();
                if !units.contains(&unit) {
                    units.push(unit);
                }
                modules
            },
        );
    let mut registry = db
        .syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry");
    for (path, units) in &module_units {
        registry
            .modules
            .insert((generation, path.clone()), units.clone());
    }
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
                        declaration.node.alias.is_some(),
                        declaration.node.visibility.node
                            == beskid_analysis::syntax::Visibility::Public,
                    ))
                }
                // An out-of-line `pub mod A.B;` is an assembled syntax dependency and a
                // public namespace edge. Register it alongside `pub use` so qualified facts
                // can continue from an imported hub into its declared child.
                beskid_analysis::syntax::Node::ModuleDeclaration(declaration) => {
                    let path = declaration
                        .node
                        .path
                        .node
                        .segments
                        .iter()
                        .map(|segment| segment.node.name.node.name.clone())
                        .collect::<Vec<_>>();
                    let binding = path.last().cloned()?;
                    Some((
                        path,
                        binding,
                        false,
                        declaration.node.visibility.node
                            == beskid_analysis::syntax::Visibility::Public,
                    ))
                }
                _ => None,
            })
            .filter_map(|(path, binding, has_explicit_alias, public)| {
                module_units
                    .get(&path)
                    .and_then(|targets| match targets.as_slice() {
                        [target] => Some(*target),
                        _ => None,
                    })
                    .map(|target| crate::db::SyntaxImport {
                        path,
                        binding,
                        has_explicit_alias,
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
        corelib_service_capability: None,
    })
}

/// Attach the separately-minted Corelib syscall service authority only after verifying the exact
/// embedded Foundation facade. This deliberately cannot produce runtime intrinsic authority.
pub fn build_canonical_corelib_syscall_typed_program(
    db: &mut BeskidDatabase,
    project: ProjectSession,
    generation: SyntaxGenerationId,
    assembly: Arc<SyntaxProgramAssembly>,
    capability: CorelibServiceCapability,
) -> Result<TypedProgram, SemanticError> {
    let expected = beskid_abi::runtime_source::canonical_corelib_syscall_sources();
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
            "syntax assembly is not the compiler-embedded Corelib syscall corpus",
        ));
    }

    let mut typed = build_typed_program(db, project, generation, assembly)?;
    let entry = typed.entry;
    let services = capability
        .services()
        .iter()
        .copied()
        .filter(|service| service.source_path == expected[0].logical_path)
        .collect();
    attach_corelib_services(db, &mut typed, entry, services);
    typed.corelib_service_capability = Some(Arc::new(capability));
    Ok(typed)
}

/// Build a normal multi-unit syntax program and, when it contains the exact compiler-embedded
/// Corelib syscall facade, grant service facts to that unit alone.
///
/// Corelib test and application assemblies include many units, so they cannot satisfy the
/// standalone-corpus constructor above. This path deliberately identifies the facade by both its
/// canonical relative source path and the compiler-embedded bytes, then records service authority
/// against only that `SourceUnitId`. All sibling and host units remain ordinary syntax units.
pub fn build_typed_program_with_corelib_services(
    db: &mut BeskidDatabase,
    project: ProjectSession,
    generation: SyntaxGenerationId,
    assembly: Arc<SyntaxProgramAssembly>,
    capability: CorelibServiceCapability,
) -> Result<TypedProgram, SemanticError> {
    let service_units = canonical_corelib_service_units(&assembly, &capability)
        .into_iter()
        .map(|(path, services)| (SourceUnitId::new(db, path), services))
        .collect::<Vec<_>>();
    let mut typed = build_typed_program(db, project, generation, assembly)?;
    if !service_units.is_empty() {
        for (service_unit, services) in service_units {
            attach_corelib_services(db, &mut typed, service_unit, services);
        }
        typed.corelib_service_capability = Some(Arc::new(capability));
    }
    Ok(typed)
}

/// Compatibility spelling for callers that assemble only the syscall facade.
pub fn build_typed_program_with_corelib_syscall_services(
    db: &mut BeskidDatabase,
    project: ProjectSession,
    generation: SyntaxGenerationId,
    assembly: Arc<SyntaxProgramAssembly>,
    capability: CorelibServiceCapability,
) -> Result<TypedProgram, SemanticError> {
    build_typed_program_with_corelib_services(db, project, generation, assembly, capability)
}

fn attach_corelib_services(
    db: &BeskidDatabase,
    typed: &mut TypedProgram,
    service_unit: SourceUnitId,
    services: Vec<CorelibService>,
) {
    db.syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .corelib_services
        .insert((service_unit, typed.generation), services);
}

fn canonical_corelib_service_units(
    assembly: &SyntaxProgramAssembly,
    capability: &CorelibServiceCapability,
) -> Vec<(std::path::PathBuf, Vec<CorelibService>)> {
    canonical_corelib_service_sources()
        .into_iter()
        .filter_map(|expected| {
            let canonical_path = canonical_corelib_service_source_path(&expected.logical_path)?;
            let candidates = assembly
                .units()
                .iter()
                .filter(|unit| {
                    unit.logical_name == expected.logical_path
                        && unit.source == expected.source
                        // Service authority is bound to the compiler-owned lexical source
                        // identity. Resolving either side would let a user-project symlink
                        // inherit the Corelib service imports.
                        && unit.path == canonical_path
                })
                .collect::<Vec<_>>();
            (candidates.len() == 1).then(|| {
                let services = capability
                    .services()
                    .iter()
                    .copied()
                    .filter(|service| service.source_path == expected.logical_path)
                    .collect();
                (candidates[0].path.clone(), services)
            })
        })
        .collect()
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
