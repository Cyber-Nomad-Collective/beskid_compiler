use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use beskid_pipeline::{PipelineObserver, compiler_stack_size, phases::PROGRAM_ASSEMBLE, report_progress};
use rayon::prelude::*;

use super::super::discovery::resolve_module_file;
use super::super::module_index::ModuleIndex;
use super::super::roots::effective_roots_for_plan;
use super::super::unit_builder::UnitBuilder;
use super::super::unit_cache::{disk_cache_stats, ensure_manifest};
use super::super::roots::EffectiveCompilationRoots;
use super::super::{AssemblyRootSet, ProgramAssembly, SourceUnit, own_unit_paths_under};
use super::discovery::{collect_bd_files, unit_progress_label};
use super::options::{AssemblyError, UnitMaterializer};
use super::scanner::{
    import_paths_from_program, module_declaration_paths_from_program, module_paths_from_qualified_references,
    parent_module_import_path, parse_program_for_discovery,
};
use super::trusted_paths::trusted_corelib_service_paths;
use crate::projects::graph::pathing::{dependency_manifest_path, normalize_existing_path};
use crate::projects::model::{
    AssemblyDiscovery, AssemblyOptions, AssemblyRecoveryPolicy, DependencySource, ProjectKind,
};
use crate::projects::{CompilePlan, PreparedProjectWorkspace};
use crate::syntax::SyntaxGenerationId;
use crate::syntax_query::SyntaxIndex;

/// Build a [`ProgramAssembly`] for `entry_path` using effective roots and discovery options.
///
/// Crate-internal; public callers use [`beskid_queries::program_assembly`].
pub(crate) fn assemble_program(
    plan: &CompilePlan,
    workspace: Option<&PreparedProjectWorkspace>,
    entry_path: &Path,
    entry_source: Option<&str>,
    options: &AssemblyOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<ProgramAssembly, AssemblyError> {
    assemble_program_with_materializer(plan, workspace, entry_path, entry_source, options, None, pipeline)
}

/// Like [`assemble_program`], using an optional Salsa unit materializer when provided.
pub fn assemble_program_with_materializer(
    plan: &CompilePlan,
    workspace: Option<&PreparedProjectWorkspace>,
    entry_path: &Path,
    entry_source: Option<&str>,
    options: &AssemblyOptions,
    materializer: Option<UnitMaterializer>,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<ProgramAssembly, AssemblyError> {
    let generation = SyntaxGenerationId::allocate().ok_or(AssemblyError::GenerationExhausted)?;
    let roots = effective_roots_for_plan(plan, workspace);
    let module_roots: Vec<PathBuf> = super::roots::module_roots_from_effective(&roots);

    let entry_canonical = entry_path.canonicalize().unwrap_or_else(|_| entry_path.to_path_buf());

    let scan_without_entry = options.discovery == AssemblyDiscovery::WorkspaceScan
        && plan.target.entry.as_deref().unwrap_or("").trim().is_empty();

    if !scan_without_entry && !entry_canonical.is_file() {
        return Err(AssemblyError::EntryNotFound { path: entry_path.to_path_buf() });
    }

    let mut discovered: Vec<PathBuf> = Vec::new();
    let mut discovered_sources: Vec<(PathBuf, String)> = Vec::new();
    let mut seen = HashSet::new();

    let enqueue = |path: PathBuf, discovered: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>| {
        let key = path.canonicalize().unwrap_or(path.clone());
        if seen.insert(key) {
            discovered.push(path);
        }
    };

    match options.discovery {
        AssemblyDiscovery::ImportClosure => {
            let mut queue = VecDeque::new();
            queue.push_back(entry_path.to_path_buf());

            while let Some(path) = queue.pop_front() {
                if discovered_sources.len() >= options.max_units {
                    return Err(AssemblyError::MaxUnits { max: options.max_units });
                }
                let key = path.canonicalize().unwrap_or_else(|_| path.clone());
                let is_entry = key == entry_canonical;
                if !seen.insert(key) {
                    continue;
                }

                let source = if is_entry {
                    if let Some(entry_text) = entry_source {
                        entry_text.to_string()
                    } else {
                        fs::read_to_string(&path)
                            .map_err(|source| AssemblyError::Read { path: path.clone(), source })?
                    }
                } else {
                    match fs::read_to_string(&path) {
                        Ok(source) => source,
                        Err(error) if options.skip_parse_errors => {
                            tracing::warn!(file = %path.display(), error = %error, "skipping unreadable dependency during discovery");
                            continue;
                        }
                        Err(source) => return Err(AssemblyError::Read { path: path.clone(), source }),
                    }
                };

                let discovery_program = match parse_program_for_discovery(&path, &source) {
                    Ok(program) => Some(program),
                    Err(error) if options.skip_parse_errors && !is_entry => {
                        tracing::warn!(file = %path.display(), error = %error, "skipping malformed dependency during discovery");
                        continue;
                    }
                    Err(_) if options.recovery_policy == AssemblyRecoveryPolicy::EditorRetainRecovered => None,
                    Err(error) => return Err(error),
                };
                discovered.push(path.clone());
                discovered_sources.push((path.clone(), source.clone()));
                // Editor materialization may retain repaired syntax, but it cannot add dependency authority.
                let Some(discovery_program) = discovery_program else {
                    continue;
                };
                for import_path in import_paths_from_program(&discovery_program.node) {
                    if let Some(dep_file) = resolve_module_file(&import_path, &roots) {
                        queue.push_back(dep_file);
                    }
                    if let Some(parent_import) = parent_module_import_path(&import_path)
                        && let Some(parent_file) = resolve_module_file(&parent_import, &roots)
                    {
                        queue.push_back(parent_file);
                    }
                }
                let mut qualified_paths = module_paths_from_qualified_references(&discovery_program.node);
                qualified_paths.sort();
                for module_path in qualified_paths {
                    if let Some(dep_file) = resolve_module_file(&module_path, &roots) {
                        queue.push_back(dep_file);
                    }
                }
                for module_path in module_declaration_paths_from_program(&discovery_program.node) {
                    if let Some(dep_file) = resolve_module_file(&module_path, &roots) {
                        queue.push_back(dep_file);
                    }
                }
            }
        }
        AssemblyDiscovery::WorkspaceScan => {
            if entry_canonical.is_file() {
                enqueue(entry_path.to_path_buf(), &mut discovered, &mut seen);
            }

            let mut paths: Vec<PathBuf> = Vec::new();
            for root in &module_roots {
                collect_bd_files(root, &mut paths);
                if let Some(generated_root) = root.parent().map(|parent| parent.join(".generated"))
                    && generated_root.is_dir()
                {
                    collect_bd_files(&generated_root, &mut paths);
                }
            }
            paths.sort();
            for path in paths {
                if discovered.len() >= options.max_units {
                    return Err(AssemblyError::MaxUnits { max: options.max_units });
                }
                enqueue(path, &mut discovered, &mut seen);
            }
        }
    }

    let project_root = plan.project_root.clone();
    if let Err(err) = ensure_manifest(&project_root) {
        tracing::warn!(
            target: "beskid.analysis.assembly",
            project_root = %project_root.display(),
            error = %err,
            "unit cache manifest skipped"
        );
    }
    let entry_key = entry_canonical;

    struct UnitBuildInput {
        path: PathBuf,
        is_entry: bool,
        source: String,
    }

    let build_inputs: Vec<UnitBuildInput> = if !discovered_sources.is_empty() {
        discovered_sources
            .iter()
            .map(|(path, source)| {
                let path_key = path.canonicalize().unwrap_or_else(|_| path.clone());
                Ok(UnitBuildInput { path: path.clone(), is_entry: path_key == entry_key, source: source.clone() })
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        discovered
            .iter()
            .filter_map(|path| {
                let path_key = path.canonicalize().unwrap_or_else(|_| path.clone());
                let is_entry = path_key == entry_key;
                let source = if is_entry {
                    entry_source.map(str::to_string).unwrap_or_default()
                } else {
                    match fs::read_to_string(path) {
                        Ok(text) => text,
                        Err(source) if options.skip_parse_errors && !is_entry => {
                            tracing::warn!(
                                target: "beskid.analysis.assembly",
                                file = %path.display(),
                                error = %source,
                                "skipping unreadable unit"
                            );
                            return None;
                        }
                        Err(source) => {
                            return Some(Err(AssemblyError::Read { path: path.clone(), source }));
                        }
                    }
                };
                Some(Ok(UnitBuildInput { path: path.clone(), is_entry, source }))
            })
            .collect::<Result<Vec<_>, _>>()?
    };

    let default_threads =
        if materializer.is_some() { 1 } else { std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) };
    let thread_cap =
        std::env::var("BESKID_ASSEMBLY_THREADS").ok().and_then(|value| value.parse().ok()).unwrap_or(default_threads);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(thread_cap.max(1))
        .stack_size(compiler_stack_size())
        .build()
        .map_err(|err| AssemblyError::Parse { path: entry_path.to_path_buf(), message: err.to_string() })?;

    let project_root_for_pool = project_root.clone();
    let salsa_build = materializer.as_ref().map(|build| build.as_ref() as _);
    let build_total = build_inputs.len() as u64;
    let build_done = AtomicU64::new(0);
    let built_units: Result<Vec<(usize, bool, SourceUnit, SyntaxIndex)>, AssemblyError> = pool.install(|| {
        build_inputs
            .par_iter()
            .enumerate()
            .map(|(discovered_index, input)| {
                let logical_name = input.path.display().to_string();
                let file = input.path.display().to_string();
                let started = std::time::Instant::now();
                let unit_span = tracing::info_span!(
                    target: "beskid.analysis.assembly",
                    "assembly.unit",
                    unit = %logical_name,
                    file = %file,
                    duration_ms = tracing::field::Empty,
                );
                let _unit_guard = unit_span.enter();

                let builder = UnitBuilder::new(&project_root_for_pool);
                let builder = if let Some(build) = salsa_build { builder.with_salsa_build(build) } else { builder };
                let label = unit_progress_label(&input.path);
                let result = match builder.build_unit(&input.path, &input.source, generation) {
                    Ok((unit, syntax_index)) => {
                        let done = build_done.fetch_add(1, Ordering::Relaxed) + 1;
                        report_progress(pipeline, PROGRAM_ASSEMBLE, done, build_total.max(1), label);
                        Ok((discovered_index, input.is_entry, unit, syntax_index))
                    }
                    Err(AssemblyError::Parse { path, message }) if options.skip_parse_errors && !input.is_entry => {
                        tracing::warn!(
                            target: "beskid.analysis.assembly",
                            unit = %path.display(),
                            file = %path.display(),
                            error = %message,
                            "skipping unparseable unit"
                        );
                        Err(AssemblyError::Parse { path, message: "skipped".to_string() })
                    }
                    Err(err) => Err(err),
                };
                unit_span.record("duration_ms", started.elapsed().as_millis() as u64);
                result
            })
            .filter(|result| {
                !matches!(
                    result,
                    Err(AssemblyError::Parse { message, .. }) if message == "skipped"
                )
            })
            .collect()
    });

    let mut built_units = built_units?;
    built_units.sort_by_key(|(index, _, _, _)| *index);
    let mut units = Vec::with_capacity(built_units.len());
    let mut syntax_indexes = Vec::with_capacity(built_units.len());
    let mut entry_position = None;
    for (_, is_entry, unit, syntax_index) in built_units {
        if is_entry {
            entry_position = Some(units.len());
        }
        units.push(unit);
        syntax_indexes.push(syntax_index);
    }

    // An explicit entry is the single root. An entry-less workspace scan has no main entry: every
    // unit of the host project's own source root is a root, judged as the project's own code.
    // An `Aggregate` project has no own source; its roots are the own units of its direct path
    // members (the Cargo virtual-workspace rule: checking the virtual manifest checks every member).
    // Never fall back to an arbitrary unit (that silently checked one unit and reported nothing
    // for the rest).
    let (entry_index, root_set) = match entry_position {
        Some(index) => (index, AssemblyRootSet::Entry),
        None if scan_without_entry => {
            let (own_paths, empty_error) = match aggregate_member_source_roots(plan, &roots)? {
                Some(member_roots) => {
                    let mut paths: Vec<PathBuf> = Vec::new();
                    for member_root in &member_roots {
                        for path in own_unit_paths_under(&units, member_root, &roots) {
                            if !paths.contains(&path) {
                                paths.push(path);
                            }
                        }
                    }
                    (paths, AssemblyError::NoAggregateMemberUnits { manifest_path: plan.manifest_path.clone() })
                }
                None => (
                    own_unit_paths_under(&units, &roots.host.source_root, &roots),
                    AssemblyError::NoRootUnits { source_root: roots.host.source_root.clone() },
                ),
            };
            let first = own_paths
                .first()
                .and_then(|first| units.iter().position(|unit| unit.path == *first))
                .ok_or(empty_error)?;
            (first, AssemblyRootSet::OwnUnits(Arc::from(own_paths)))
        }
        None => return Err(AssemblyError::EntryNotFound { path: entry_path.to_path_buf() }),
    };

    let disk_stats = disk_cache_stats();
    tracing::debug!(
        target: "beskid.analysis.assembly",
        hits = disk_stats.hits,
        misses = disk_stats.misses,
        "assembly artifact cache stats"
    );
    let _ = beskid_artifacts::ArtifactStore::new(&project_root).refresh_manifest();

    let module_index = Arc::new(ModuleIndex::build(&units, &syntax_indexes, &roots, plan));

    let trusted_corelib_service_paths = trusted_corelib_service_paths(plan, &roots, &units);
    let glue_libraries = manifest_glue_libraries(plan)?;

    super::super::runtime_fixture::attach_runtime_fixture(
        ProgramAssembly { compiled_mod_metadata: Vec::new(),
            verified_package_identities: workspace
                .map(|workspace| workspace.package_identities().clone())
                .unwrap_or_default(),
            runtime_fixture: None,
            roots,
            units: Arc::new(units),
            syntax_indexes: Arc::new(syntax_indexes),
            generation,
            entry_index,
            root_set,
            discovery: options.discovery,
            recovery_policy: options.recovery_policy,
            module_index,
            has_std_dependency: plan.has_std_dependency,
            trusted_corelib_service_paths,
            glue_libraries,
        },
        plan,
    )
}

/// Owner library labels of the plan manifest's `glue` blocks, in declaration order. A plan
/// without a manifest file on disk (synthetic fixture plans) declares none. Fails closed when the
/// manifest cannot be read.
fn manifest_glue_libraries(plan: &CompilePlan) -> Result<Arc<[String]>, AssemblyError> {
    if !plan.manifest_path.is_file() {
        return Ok(Arc::from([]));
    }
    let manifest = crate::projects::load_manifest_from_path(&plan.manifest_path).map_err(|error| {
        AssemblyError::GlueOwners { manifest_path: plan.manifest_path.clone(), message: error.to_string() }
    })?;
    Ok(manifest.glue.into_iter().map(|owner| owner.library).collect())
}

/// Effective source roots of the direct path members of an `Aggregate` root project, in manifest
/// declaration order, or `None` when the plan's manifest is not an `Aggregate`.
///
/// Members are the aggregate's `source = path` dependencies; registry and git dependencies are
/// never members. A path member that the compile plan does not carry as a source dependency is a
/// `Template` or `Bsol` package with no Beskid source, so it contributes no roots. A plan without
/// a manifest file on disk (synthetic fixture plans) is not an aggregate. Fails closed when the
/// manifest cannot be read or a member root cannot be identified unambiguously.
fn aggregate_member_source_roots(
    plan: &CompilePlan,
    roots: &EffectiveCompilationRoots,
) -> Result<Option<Vec<PathBuf>>, AssemblyError> {
    if !plan.manifest_path.is_file() {
        return Ok(None);
    }
    let manifest_error =
        |message: String| AssemblyError::AggregateMembers { manifest_path: plan.manifest_path.clone(), message };
    let manifest = crate::projects::load_manifest_from_path(&plan.manifest_path)
        .map_err(|error| manifest_error(error.to_string()))?;
    if manifest.project.kind != ProjectKind::Aggregate {
        return Ok(None);
    }
    let mut member_roots = Vec::new();
    for dependency in manifest.dependencies.iter().filter(|dependency| dependency.source == DependencySource::Path) {
        let relative = dependency.path.as_deref().ok_or_else(|| {
            manifest_error(format!("member `{}` declares source = path without `path`", dependency.name))
        })?;
        let member_manifest = normalize_existing_path(
            &dependency_manifest_path(&plan.project_root, relative).map_err(|error| manifest_error(error.to_string()))?,
        );
        let Some(resolved) = plan
            .dependency_projects
            .iter()
            .find(|project| normalize_existing_path(&project.manifest_path) == member_manifest)
        else {
            continue;
        };
        let resolved_root = crate::paths::unit_path_key(&resolved.source_root);
        let effective = match roots
            .dependencies
            .iter()
            .find(|entry| crate::paths::unit_path_key(&entry.source_root) == resolved_root)
        {
            Some(entry) => entry,
            None => {
                let mut named = roots
                    .dependencies
                    .iter()
                    .filter(|entry| entry.dependency_name.as_deref() == Some(resolved.dependency_name.as_str()));
                match (named.next(), named.next()) {
                    (Some(entry), None) => entry,
                    (None, _) => {
                        return Err(manifest_error(format!(
                            "member `{}` has no effective source root in the prepared workspace",
                            dependency.name
                        )));
                    }
                    (Some(_), Some(_)) => {
                        return Err(manifest_error(format!(
                            "member `{}` names more than one effective source root",
                            dependency.name
                        )));
                    }
                }
            }
        };
        if !member_roots.contains(&effective.source_root) {
            member_roots.push(effective.source_root.clone());
        }
    }
    Ok(Some(member_roots))
}
