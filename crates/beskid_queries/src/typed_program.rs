use std::sync::Arc;

use beskid_abi::runtime_source::{
    CorelibService, CorelibServiceCapability, RuntimeIntrinsicCapability, canonical_corelib_deadline_source,
    canonical_corelib_private_field_sources, canonical_corelib_service_sources, corelib_service_source_identity,
    corelib_source_locations_match,
};
use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;

use crate::{BeskidDatabase, Db, ProjectSession, SemanticError, SourceUnitId, TypedProgram};

pub(crate) const CANONICAL_RUNTIME_CORPUS_BINDING: &str = "__beskid_canonical_runtime";

/// Return the existing owner of a prepared syntax assembly when it has already
/// been registered in this database, otherwise mint the first owner for it.
///
/// Prepared frontends and their syntax consumers share a database. Reusing the
/// recorded owner is therefore required to preserve the one-project-per-source
/// invariant across test discovery, REPL inspection, and code generation.
pub fn project_session_for_syntax_assembly(
    db: &BeskidDatabase,
    assembly: &ProgramAssembly,
    fallback_target_name: &str,
    fallback_lockfile_digest: &str,
) -> Result<ProjectSession, SemanticError> {
    Ok(registered_syntax_owner(db, assembly)?.unwrap_or_else(|| {
        ProjectSession::new(
            db,
            assembly.roots.host.source_root.clone(),
            assembly.entry_unit().path.clone(),
            fallback_target_name.into(),
            fallback_lockfile_digest.into(),
        )
    }))
}

/// Owner for a prepared syntax assembly of a planned project entry.
///
/// Reuses the session that already owns any unit of the assembly (several
/// entries of one project share dependency units, so they must share their
/// owner). Otherwise the first owner is the plan-keyed registry session, never
/// an unregistered one, so every later caller that resolves the same plan and
/// entry (LSP/IDE facts, typed entry bundles, program assembly) finds the
/// session that owns the registered syntax.
pub fn project_session_for_planned_syntax_assembly(
    db: &mut BeskidDatabase,
    assembly: &ProgramAssembly,
    plan: &beskid_analysis::projects::CompilePlan,
    entry_path: &std::path::Path,
    lockfile_digest: String,
) -> Result<ProjectSession, SemanticError> {
    if let Some(owner) = registered_syntax_owner(db, assembly)? {
        return Ok(owner);
    }
    Ok(db.ensure_project_session(plan, entry_path, lockfile_digest))
}

fn registered_syntax_owner(
    db: &BeskidDatabase,
    assembly: &ProgramAssembly,
) -> Result<Option<ProjectSession>, SemanticError> {
    let mut owner = None;
    for unit in assembly.units.iter() {
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
    Ok(owner)
}

/// Register an expanded syntax assembly as one generation-safe typed-program identity.
pub fn build_typed_program(
    db: &mut BeskidDatabase,
    project: ProjectSession,
    generation: SyntaxGenerationId,
    assembly: Arc<ProgramAssembly>,
) -> Result<TypedProgram, SemanticError> {
    if generation != assembly.generation {
        return Err(SemanticError::new("typed-program generation does not match ProgramAssembly generation"));
    }
    assembly.package_identities().validate().map_err(|error| SemanticError::new(error.to_string()))?;
    for unit in assembly.units.iter() {
        assembly
            .package_identities()
            .validate_source(&unit.path, &unit.source)
            .map_err(|error| SemanticError::new(error.to_string()))?;
    }
    let generation = assembly.generation;
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
        let public_dynamic = assembly.is_canonical_public_dynamic_unit(unit);
        if public_dynamic {
            let parsed = beskid_analysis::services::parse_program_with_source_name_and_diagnostics(
                &unit.logical_name,
                &unit.source,
            )
            .map_err(|error| SemanticError::new(format!("canonical Dynamic source parse: {error}")))?;
            if parsed.recovered || !parsed.diagnostics.is_empty() || parsed.program != unit.program {
                return Err(SemanticError::new(
                    "Dynamic source bytes do not authorize a modified/recovered syntax tree",
                ));
            }
            let input =
                db.syntax_unit(identity).ok_or_else(|| SemanticError::new("Dynamic source is not registered"))?;
            let mut revision = input.revision(db).as_ref().clone();
            revision.runtime_source_authority =
                Some(Arc::from(beskid_abi::runtime_source::CANONICAL_PUBLIC_DYNAMIC_SOURCE_PATH));
            use salsa::Setter;
            input.set_revision(db).to(Arc::new(revision));
        }
        // Correspondence is not execution authority. The native adapter separately validates
        // the complete SDK closure and issues the host-callback capability.
        let canonical = beskid_abi::sdk_source::canonical_sdk_sources().iter().find(|expected| {
            expected.path().ends_with(".bd")
                && expected.bytes() == unit.source.as_bytes()
                && unit
                    .path
                    .ends_with(std::path::Path::new(expected.path().strip_prefix("src/").unwrap_or(expected.path())))
        });
        if let Some(expected) = canonical {
            let parsed = beskid_analysis::services::parse_program_with_source_name_and_diagnostics(
                &unit.logical_name,
                &unit.source,
            )
            .map_err(|error| SemanticError::new(format!("canonical SDK source parse: {error}")))?;
            if parsed.recovered || !parsed.diagnostics.is_empty() || parsed.program != unit.program {
                return Err(SemanticError::new("SDK source bytes do not authorize a modified/recovered syntax tree"));
            }
            let input = db.syntax_unit(identity).ok_or_else(|| SemanticError::new("SDK source is not registered"))?;
            let mut revision = input.revision(db).as_ref().clone();
            revision.sdk_source_authority = Some(Arc::from(expected.path()));
            use salsa::Setter;
            input.set_revision(db).to(Arc::new(revision));
        }
    }

    {
        let mut registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
        let proof = assembly.package_identities();
        let key = (project, generation);
        if let Some(existing) = registry.package_identities.get(&key) {
            if existing != proof {
                return Err(SemanticError::new("package proof differs from registered project generation"));
            }
        } else {
            registry.package_identities.insert(key, proof.clone());
        }
    }

    let mut module_units = std::collections::HashMap::<Vec<String>, Vec<SourceUnitId>>::new();
    for unit in assembly.units.iter() {
        let Some(module_path) = beskid_analysis::projects::infer_logical_module_path(unit, &assembly.roots) else {
            continue;
        };
        let unit_id = SourceUnitId::new(db, unit.path.clone());
        let units = module_units.entry(module_path).or_default();
        if !units.contains(&unit_id) {
            units.push(unit_id);
        }
    }
    let mut registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    for (path, units) in &module_units {
        registry.modules.insert((generation, path.clone()), units.clone());
    }
    // Attestation is source identity, not a runtime service, so every typed program records it
    // whatever service capability (if any) the caller later attaches.
    for (path, logical_path) in canonical_corelib_attested_units(&assembly) {
        registry.corelib_source_paths.insert((SourceUnitId::new(db, path), generation), logical_path);
    }
    for unit in assembly.units.iter() {
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
                        declaration.node.visibility.node == beskid_analysis::syntax::Visibility::Public,
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
                        declaration.node.visibility.node == beskid_analysis::syntax::Visibility::Public,
                    ))
                }
                _ => None,
            })
            .filter_map(|(path, binding, has_explicit_alias, public)| {
                registry
                    .visible_module_units(generation, &path)
                    .and_then(|targets| match targets {
                        [target] => Some(*target),
                        _ => None,
                    })
                    .map(|target| crate::db::SyntaxImport { path, binding, has_explicit_alias, target, public })
            })
            .collect();
        registry.imports.insert((unit_id, generation), imports);
    }
    drop(registry);
    // Scoped-cleanup (E1230) and dead-growth (E1231) legality are judged by the reachability-scoped
    // legality gate (`legality::check_items`) for the items a lowering request actually lowers,
    // never here for every unit: admission must not let an unreachable item poison the assembly.

    Ok(TypedProgram {
        project,
        entry: SourceUnitId::new(db, entry_unit.path.clone()),
        generation,
        assembly,
        runtime_intrinsic_capability: None,
        corelib_service_capability: None,
    })
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
    assembly: Arc<ProgramAssembly>,
    capability: CorelibServiceCapability,
) -> Result<TypedProgram, SemanticError> {
    let service_units = canonical_corelib_service_units(&assembly, &capability)
        .into_iter()
        .map(|(path, logical_path, services)| (SourceUnitId::new(db, path), logical_path, services))
        .collect::<Vec<_>>();
    let mut typed = build_typed_program(db, project, generation, assembly)?;
    if !service_units.is_empty() {
        for (service_unit, logical_path, services) in service_units {
            attach_corelib_services(db, &mut typed, service_unit, logical_path, services);
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
    assembly: Arc<ProgramAssembly>,
    capability: CorelibServiceCapability,
) -> Result<TypedProgram, SemanticError> {
    build_typed_program_with_corelib_services(db, project, generation, assembly, capability)
}

fn attach_corelib_services(
    db: &BeskidDatabase,
    typed: &mut TypedProgram,
    service_unit: SourceUnitId,
    logical_path: String,
    services: Vec<CorelibService>,
) {
    let mut registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    registry.corelib_services.insert((service_unit, typed.generation), services);
    registry.corelib_source_paths.insert((service_unit, typed.generation), logical_path);
}

fn canonical_corelib_service_units(
    assembly: &ProgramAssembly,
    capability: &CorelibServiceCapability,
) -> Vec<(std::path::PathBuf, String, Vec<CorelibService>)> {
    canonical_corelib_service_sources()
        .into_iter()
        .filter_map(|expected| {
            let unit = exact_compiler_owned_corelib_unit(assembly, &expected)?;
            let services = capability
                .services()
                .iter()
                .copied()
                .filter(|service| service.source_path == expected.logical_path)
                .collect();
            Some((unit.path.clone(), expected.logical_path, services))
        })
        .collect()
}

/// Compiler-owned units that carry no runtime service but are source-attested for exact
/// private-field admissions: the opaque Deadline declaration and the Network resource authority
/// with the socket declarations it constructs. Each must be the exact canonical source.
fn canonical_corelib_attested_units(assembly: &ProgramAssembly) -> Vec<(std::path::PathBuf, String)> {
    std::iter::once(canonical_corelib_deadline_source())
        .chain(canonical_corelib_private_field_sources())
        .filter_map(|expected| {
            let unit = exact_compiler_owned_corelib_unit(assembly, &expected)?;
            Some((unit.path.clone(), expected.logical_path))
        })
        .collect()
}

fn exact_compiler_owned_corelib_unit<'a>(
    assembly: &'a ProgramAssembly,
    expected: &beskid_abi::abi_v5::SourceUnit,
) -> Option<&'a beskid_analysis::projects::SourceUnit> {
    let identity = corelib_service_source_identity(&expected.logical_path);
    let candidates = assembly
        .units
        .iter()
        .filter(|unit| {
            if unit.source != expected.source {
                return false;
            }

            // Origin is request evidence, distinct from the canonical semantic key.
            // Never resolve a user origin to decide whether it is compiler-owned.
            let direct = identity.as_ref().is_some_and(|identity| {
                corelib_source_locations_match(&unit.path, &identity.canonical_path)
                    && (corelib_source_locations_match(&unit.origin_path, &identity.declared_path)
                        || corelib_source_locations_match(&unit.origin_path, &identity.canonical_path))
            });
            let authorized_path = direct
                // Resolve only the destination issued by the loader, never the user's
                // origin. Both the selected location and physical identity must match.
                || assembly
                    .trusted_corelib_service_paths
                    .iter()
                    .any(|trusted| corelib_source_locations_match(&unit.origin_path, trusted)
                        && trusted.canonicalize().is_ok_and(|path| path == unit.path));
            authorized_path
                // `unit.path` is the resolved semantic key. Inspect the request origin so a
                // symlinked materialized file cannot inherit the target's service authority.
                && std::fs::symlink_metadata(&unit.origin_path)
                    .is_ok_and(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
        })
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [unit] => Some(*unit),
        _ => None,
    }
}

/// Attach compiler-minted runtime intrinsic authority after validating that the assembled syntax
/// is the exact embedded canonical corpus. This is deliberately separate from the ordinary
/// assembly constructor so package names, paths, and user source cannot acquire the capability.
pub fn build_canonical_runtime_typed_program(
    db: &mut BeskidDatabase,
    project: ProjectSession,
    generation: SyntaxGenerationId,
    assembly: Arc<ProgramAssembly>,
    capability: RuntimeIntrinsicCapability,
) -> Result<TypedProgram, SemanticError> {
    let expected = beskid_abi::runtime_source::canonical_runtime_sources();
    let actual = assembly
        .units
        .iter()
        .map(|unit| beskid_abi::abi_v5::SourceUnit {
            logical_path: unit.logical_name.clone(),
            source: unit.source.clone(),
        })
        .collect::<Vec<_>>();
    let exact_corpus = actual.len() == expected.len()
        && beskid_abi::abi_v5::canonical_source_hash(&actual).is_ok_and(|hash| hash == capability.source_hash())
        && actual.iter().all(|source| expected.iter().any(|expected| expected == source));
    if !exact_corpus {
        return Err(SemanticError::new("syntax assembly is not the compiler-embedded canonical runtime corpus"));
    }
    // Matching source text cannot authorize an independently altered AST. This
    // constructor admits only the parse of those exact bytes in each supplied unit.
    for unit in assembly.units.iter() {
        let parsed = beskid_analysis::services::parse_program_with_source_name(
            unit.path.to_str().unwrap_or_default(),
            &unit.source,
        )
        .map_err(|_| SemanticError::new("canonical runtime source does not parse"))?;
        if parsed != unit.program {
            return Err(SemanticError::new("canonical runtime syntax differs from embedded source"));
        }
    }

    let services = capability.corelib_service_capability();
    let service_sources = canonical_corelib_service_sources();
    let mut typed = build_typed_program(db, project, generation, assembly)?;
    for unit in typed.assembly.units.clone().iter() {
        let source = SourceUnitId::new(db, unit.path.clone());
        let input =
            db.syntax_unit(source).ok_or_else(|| SemanticError::new("canonical runtime unit is unregistered"))?;
        if capability.authorizes_source(&unit.logical_name) {
            let mut revision = input.revision(db).as_ref().clone();
            revision.runtime_source_authority = Some(Arc::from(unit.logical_name.as_str()));
            use salsa::Setter;
            input.set_revision(db).to(Arc::new(revision));
        } else if service_sources
            .iter()
            .any(|expected| expected.logical_path == unit.logical_name && expected.source == unit.source)
        {
            // The whole embedded closure was verified above. Ordinary support
            // units receive only their exact service table, never raw intrinsics.
            let owned = services
                .services()
                .iter()
                .copied()
                .filter(|service| service.source_path == unit.logical_name)
                .collect();
            attach_corelib_services(db, &mut typed, source, unit.logical_name.clone(), owned);
        }
    }
    typed.corelib_service_capability = Some(services);
    attach_canonical_runtime_cross_unit_scope(db, &typed);
    typed.runtime_intrinsic_capability = Some(Arc::new(capability));
    Ok(typed)
}

/// Make the exact compiler-owned runtime corpus one private resolution scope.
///
/// Runtime source is split by ownership domain, while its Bootstrap helpers are
/// intentionally shared by unqualified name. This scope is installed only after
/// the whole embedded corpus has been byte-for-byte verified above. Ordinary
/// assemblies keep their explicit-import-only resolution contract, and duplicate
/// public names remain unresolved through `unique_imported_function`.
fn attach_canonical_runtime_cross_unit_scope(db: &BeskidDatabase, typed: &TypedProgram) {
    let units = typed
        .assembly
        .units
        .iter()
        .filter(|unit| {
            db.syntax_unit(SourceUnitId::new(db, unit.path.clone()))
                .is_some_and(|input| input.revision(db).runtime_source_authority.is_some())
        })
        .map(|unit| SourceUnitId::new(db, unit.path.clone()))
        .collect::<Vec<_>>();
    attach_private_runtime_scope(db, typed.generation, &units);
}

fn attach_private_runtime_scope(db: &BeskidDatabase, generation: SyntaxGenerationId, units: &[SourceUnitId]) {
    let mut registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    for owner in units {
        let imports = registry.imports.entry((*owner, generation)).or_default();
        for target in units.iter().copied().filter(|target| target != owner) {
            if imports.iter().any(|import| {
                import.target == target
                    && import.binding == CANONICAL_RUNTIME_CORPUS_BINDING
                    && import.has_explicit_alias
            }) {
                continue;
            }
            imports.push(crate::db::SyntaxImport {
                path: vec![CANONICAL_RUNTIME_CORPUS_BINDING.to_owned()],
                binding: CANONICAL_RUNTIME_CORPUS_BINDING.to_owned(),
                has_explicit_alias: true,
                target,
                public: false,
            });
        }
    }
}

/// A fixture is an explicitly proved sibling of the complete production corpus, never a
/// substitute for part of it. Corelib dependencies retain only their own service authority.
pub fn build_runtime_fixture_typed_program(
    db: &mut BeskidDatabase,
    project: ProjectSession,
    generation: SyntaxGenerationId,
    assembly: Arc<ProgramAssembly>,
    manifest: &beskid_abi::abi_v5::AbiManifestV5,
) -> Result<TypedProgram, SemanticError> {
    let proof =
        assembly.runtime_fixture.as_ref().ok_or_else(|| SemanticError::new("runtime fixture proof is absent"))?;
    let fixture = proof.fixture();
    if assembly.entry_unit().logical_name != fixture.logical_path
        || assembly.entry_unit().source != fixture.source
        || assembly.units.iter().filter(|unit| unit.logical_name == fixture.logical_path).count() != 1
    {
        return Err(SemanticError::new("runtime fixture source differs from its proof"));
    }
    let expected_runtime = beskid_abi::runtime_source::canonical_runtime_sources();
    let canonical_runtime_unit =
        |logical_name: &str| expected_runtime.iter().any(|expected| expected.logical_path == logical_name);
    // Private runtime scope is granted by logical path below. An extra source under the runtime
    // namespace must never reach that grant merely because the exact corpus check ignores it.
    if assembly
        .units
        .iter()
        .any(|unit| unit.logical_name.starts_with("src/Runtime/") && !canonical_runtime_unit(&unit.logical_name))
    {
        return Err(SemanticError::new("runtime fixture assembly carries a non-canonical runtime source"));
    }
    let runtime_units = assembly
        .units
        .iter()
        .filter(|unit| expected_runtime.iter().any(|expected| expected.logical_path == unit.logical_name))
        .cloned()
        .collect::<Vec<_>>();
    let runtime = Arc::new(ProgramAssembly::new(
        assembly.roots.clone(),
        Arc::new(runtime_units),
        0,
        assembly.discovery,
        assembly.module_index.clone(),
        false,
        generation,
    ));
    // Keep the original exact-corpus constructor as the sole production authority check.
    build_canonical_runtime_typed_program(
        db,
        project,
        generation,
        runtime,
        beskid_abi::runtime_source::canonical_runtime_intrinsic_capability(manifest)
            .map_err(|_| SemanticError::new("canonical runtime capability unavailable"))?,
    )?;
    let mut typed = build_typed_program_with_corelib_services(
        db,
        project,
        generation,
        assembly.clone(),
        beskid_abi::runtime_source::canonical_corelib_service_capability(manifest)
            .map_err(|_| SemanticError::new("canonical corelib capability unavailable"))?,
    )?;
    let scope = assembly
        .units
        .iter()
        .filter(|unit| canonical_runtime_unit(&unit.logical_name) || unit.logical_name == fixture.logical_path)
        .map(|unit| SourceUnitId::new(db, unit.path.clone()))
        .collect::<Vec<_>>();
    attach_private_runtime_scope(db, generation, &scope);
    typed.runtime_intrinsic_capability = Some(Arc::new(
        proof
            .intrinsic_capability(manifest)
            .map_err(|_| SemanticError::new("runtime fixture capability unavailable"))?,
    ));
    Ok(typed)
}
