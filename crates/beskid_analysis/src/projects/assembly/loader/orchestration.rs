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
use super::super::{ProgramAssembly, SourceUnit};
use super::discovery::{collect_bd_files, unit_progress_label};
use super::options::{AssemblyError, UnitMaterializer};
use super::scanner::{
    import_paths_from_source_full, module_declaration_paths_from_source, module_paths_from_qualified_references,
    parent_module_import_path,
};
use super::trusted_paths::trusted_corelib_service_paths;
use crate::projects::model::{AssemblyDiscovery, AssemblyOptions};
use crate::projects::{CompilePlan, PreparedProjectWorkspace};

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
            queue.push_back(entry_canonical.clone());

            while let Some(path) = queue.pop_front() {
                if discovered_sources.len() >= options.max_units {
                    return Err(AssemblyError::MaxUnits { max: options.max_units });
                }
                let key = path.canonicalize().unwrap_or_else(|_| path.clone());
                if !seen.insert(key) {
                    continue;
                }

                let source = if path == entry_canonical {
                    if let Some(entry_text) = entry_source {
                        entry_text.to_string()
                    } else {
                        fs::read_to_string(&path)
                            .map_err(|source| AssemblyError::Read { path: path.clone(), source })?
                    }
                } else {
                    fs::read_to_string(&path).map_err(|source| AssemblyError::Read { path: path.clone(), source })?
                };

                discovered.push(path.clone());
                discovered_sources.push((path.clone(), source.clone()));

                for import_path in import_paths_from_source_full(&source) {
                    if let Some(dep_file) = resolve_module_file(&import_path, &roots) {
                        queue.push_back(dep_file);
                    }
                    if let Some(parent_import) = parent_module_import_path(&import_path)
                        && let Some(parent_file) = resolve_module_file(&parent_import, &roots)
                    {
                        queue.push_back(parent_file);
                    }
                }
                let mut qualified_paths = module_paths_from_qualified_references(&source);
                qualified_paths.sort();
                for module_path in qualified_paths {
                    if let Some(dep_file) = resolve_module_file(&module_path, &roots) {
                        queue.push_back(dep_file);
                    }
                }
                for module_path in module_declaration_paths_from_source(&path, &source) {
                    if let Some(dep_file) = resolve_module_file(&module_path, &roots) {
                        queue.push_back(dep_file);
                    }
                }
            }
        }
        AssemblyDiscovery::WorkspaceScan => {
            if entry_canonical.is_file() {
                enqueue(entry_canonical.clone(), &mut discovered, &mut seen);
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
    let entry_key = entry_canonical.canonicalize().unwrap_or(entry_canonical.clone());

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
    let built_units: Result<Vec<(usize, bool, SourceUnit, super::UnitHir)>, AssemblyError> = pool.install(|| {
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
                let result = match builder.build_unit(&input.path, &input.source) {
                    Ok((unit, hir)) => {
                        let done = build_done.fetch_add(1, Ordering::Relaxed) + 1;
                        report_progress(pipeline, PROGRAM_ASSEMBLE, done, build_total.max(1), label);
                        Ok((discovered_index, input.is_entry, unit, hir))
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
    let mut hir_units_vec = Vec::with_capacity(built_units.len());
    let mut entry_index = 0usize;
    for (_, is_entry, unit, hir) in built_units {
        if is_entry {
            entry_index = units.len();
        }
        units.push(unit);
        hir_units_vec.push(hir);
    }

    super::reindex_hir_units_in_place(&mut hir_units_vec);

    if units.is_empty() {
        return Err(AssemblyError::EntryNotFound { path: entry_path.to_path_buf() });
    }

    let disk_stats = disk_cache_stats();
    tracing::debug!(
        target: "beskid.analysis.assembly",
        hits = disk_stats.hits,
        misses = disk_stats.misses,
        "assembly artifact cache stats"
    );
    let _ = beskid_artifacts::ArtifactStore::new(&project_root).refresh_manifest();

    let hir_units = Arc::new(hir_units_vec);
    let prefetch_dependency_roots = options.discovery == AssemblyDiscovery::WorkspaceScan;
    let module_index =
        Arc::new(ModuleIndex::build(&units, hir_units.as_ref(), entry_index, &roots, plan, prefetch_dependency_roots));

    let trusted_corelib_service_paths = trusted_corelib_service_paths(plan, workspace, &units);

    Ok(ProgramAssembly {
        roots,
        units: Arc::new(units),
        hir_units,
        entry_index,
        discovery: options.discovery,
        module_index,
        has_std_dependency: plan.has_std_dependency,
        trusted_corelib_service_paths,
    })
}
