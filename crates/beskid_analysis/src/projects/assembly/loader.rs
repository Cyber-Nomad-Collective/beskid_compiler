//! Discover, parse, and index compilation units for a compile plan.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rayon::prelude::*;
use thiserror::Error;

use super::discovery::resolve_module_file;
use super::module_index::ModuleIndex;
use super::roots::effective_roots_for_plan;
use super::unit_builder::UnitBuilder;
use super::unit_cache::{disk_cache_stats, ensure_manifest};
use super::{ProgramAssembly, SourceUnit, UnitHir};
use crate::projects::model::{AssemblyDiscovery, AssemblyOptions};
use crate::projects::{CompilePlan, PreparedProjectWorkspace};
use crate::syntax::{Program, Spanned};

/// Optional Salsa-backed unit builder (set by `beskid_queries` during assembly).
pub type UnitMaterializer = std::sync::Arc<
    dyn Fn(&Path, &str) -> Result<(SourceUnit, UnitHir), AssemblyError> + Send + Sync,
>;

#[derive(Debug, Error)]
pub enum AssemblyError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("entry file not found under effective roots: {path}")]
    EntryNotFound { path: PathBuf },
    #[error("assembly exceeded max_units ({max})")]
    MaxUnits { max: usize },
}

fn prelude_reexport_module_paths(prelude_text: &str) -> Vec<String> {
    prelude_text
        .lines()
        .filter_map(|line| {
            let line = line.split("//").next()?.trim();
            let rest = line.strip_prefix("pub mod ")?;
            let path = rest.trim_end_matches(';').trim();
            (!path.is_empty()).then(|| path.to_string())
        })
        .collect()
}

pub(crate) fn expand_syntax_for_assembly(program: Spanned<Program>) -> Spanned<Program> {
    crate::macros::expand_program_with_diagnostics(
        program,
        crate::macros::DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
        "",
        "",
    )
    .program
}

/// Build a [`ProgramAssembly`] for `entry_path` using effective roots and discovery options.
pub fn assemble_program(
    plan: &CompilePlan,
    workspace: Option<&PreparedProjectWorkspace>,
    entry_path: &Path,
    entry_source: Option<&str>,
    options: &AssemblyOptions,
) -> Result<ProgramAssembly, AssemblyError> {
    assemble_program_with_materializer(plan, workspace, entry_path, entry_source, options, None)
}

/// Like [`assemble_program`], using an optional Salsa unit materializer when provided.
pub fn assemble_program_with_materializer(
    plan: &CompilePlan,
    workspace: Option<&PreparedProjectWorkspace>,
    entry_path: &Path,
    entry_source: Option<&str>,
    options: &AssemblyOptions,
    materializer: Option<UnitMaterializer>,
) -> Result<ProgramAssembly, AssemblyError> {
    let roots = effective_roots_for_plan(plan, workspace);
    let module_roots: Vec<PathBuf> = super::roots::module_roots_from_effective(&roots);

    let entry_canonical = entry_path
        .canonicalize()
        .unwrap_or_else(|_| entry_path.to_path_buf());

    let mut discovered: Vec<PathBuf> = Vec::new();
    let mut discovered_sources: Vec<(PathBuf, String)> = Vec::new();
    let mut seen = HashSet::new();

    let enqueue = |path: PathBuf, discovered: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>| {
        let key = path.canonicalize().unwrap_or(path.clone());
        if seen.insert(key) {
            discovered.push(path);
        }
    };

    if !entry_canonical.is_file() {
        return Err(AssemblyError::EntryNotFound {
            path: entry_path.to_path_buf(),
        });
    }

    let entry_is_prelude = entry_canonical
        .file_name()
        .is_some_and(|name| name == "Prelude.bd");

    let mut prelude_seeds = Vec::new();
    let mut prelude_reexport_paths = Vec::new();
    if options.include_std_prelude && !entry_is_prelude {
        for root in &module_roots {
            if is_compiler_mod_sdk_source_root(root) {
                continue;
            }
            let prelude = root.join("Prelude.bd");
            if !prelude.is_file() {
                continue;
            }
            let Ok(text) = fs::read_to_string(&prelude) else {
                continue;
            };
            prelude_reexport_paths.extend(prelude_reexport_module_paths(&text));
        }
        prelude_reexport_paths.sort();
        prelude_reexport_paths.dedup();
        if options.discovery == AssemblyDiscovery::WorkspaceScan && plan.has_std_dependency {
            for root in &module_roots {
                if is_compiler_mod_sdk_source_root(root) {
                    continue;
                }
                let prelude = root.join("Prelude.bd");
                if prelude.is_file() && !prelude_seeds.contains(&prelude) {
                    prelude_seeds.push(prelude);
                    break;
                }
            }
        }
    }

    discovered.clear();
    discovered_sources.clear();
    seen.clear();

    match options.discovery {
        AssemblyDiscovery::ImportClosure => {
            let mut queue = VecDeque::new();
            queue.push_back(entry_canonical.clone());
            for seed in prelude_seeds {
                queue.push_back(seed);
            }
            for module_path in &prelude_reexport_paths {
                if let Some(dep_file) = resolve_module_file(module_path, &roots) {
                    queue.push_back(dep_file);
                }
            }

            while let Some(path) = queue.pop_front() {
                if discovered_sources.len() >= options.max_units {
                    return Err(AssemblyError::MaxUnits {
                        max: options.max_units,
                    });
                }
                let key = path.canonicalize().unwrap_or_else(|_| path.clone());
                if !seen.insert(key) {
                    continue;
                }

                let source = if path == entry_canonical {
                    if let Some(entry_text) = entry_source {
                        entry_text.to_string()
                    } else {
                        fs::read_to_string(&path).map_err(|source| AssemblyError::Read {
                            path: path.clone(),
                            source,
                        })?
                    }
                } else {
                    fs::read_to_string(&path).map_err(|source| AssemblyError::Read {
                        path: path.clone(),
                        source,
                    })?
                };

                discovered.push(path.clone());
                discovered_sources.push((path.clone(), source.clone()));

                for import_path in import_paths_from_source_full(&source) {
                    if let Some(dep_file) = resolve_module_file(&import_path, &roots) {
                        queue.push_back(dep_file);
                    }
                }
            }
        }
        AssemblyDiscovery::WorkspaceScan => {
            enqueue(entry_canonical.clone(), &mut discovered, &mut seen);
            for seed in prelude_seeds {
                enqueue(seed, &mut discovered, &mut seen);
            }

            let mut paths: Vec<PathBuf> = Vec::new();
            for root in &module_roots {
                collect_bd_files(root, &mut paths);
            }
            paths.sort();
            for path in paths {
                if discovered.len() >= options.max_units {
                    return Err(AssemblyError::MaxUnits {
                        max: options.max_units,
                    });
                }
                enqueue(path, &mut discovered, &mut seen);
            }
        }
    }

    let project_root = plan.project_root.clone();
    if let Err(err) = ensure_manifest(&project_root) {
        log::warn!(
            "unit cache manifest skipped for {}: {err}",
            project_root.display()
        );
    }
    let entry_key = entry_canonical
        .canonicalize()
        .unwrap_or(entry_canonical.clone());

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
                Ok(UnitBuildInput {
                    path: path.clone(),
                    is_entry: path_key == entry_key,
                    source: source.clone(),
                })
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
                            log::warn!("skipping unreadable unit {} ({source})", path.display());
                            return None;
                        }
                        Err(source) => {
                            return Some(Err(AssemblyError::Read {
                                path: path.clone(),
                                source,
                            }));
                        }
                    }
                };
                Some(Ok(UnitBuildInput {
                    path: path.clone(),
                    is_entry,
                    source,
                }))
            })
            .collect::<Result<Vec<_>, _>>()?
    };

    let default_threads = if materializer.is_some() {
        // Salsa materializer serializes on a shared DB mutex; extra rayon threads add overhead only.
        1
    } else {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    };
    let thread_cap = std::env::var("BESKID_ASSEMBLY_THREADS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default_threads);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(thread_cap.max(1))
        .build()
        .map_err(|err| AssemblyError::Parse {
            path: entry_path.to_path_buf(),
            message: err.to_string(),
        })?;

    let project_root_for_pool = project_root.clone();
    let salsa_build = materializer.as_ref().map(|build| build.as_ref() as _);
    let built_units: Result<Vec<(usize, bool, SourceUnit, super::UnitHir)>, AssemblyError> = pool
        .install(|| {
            build_inputs
                .par_iter()
                .enumerate()
                .map(|(discovered_index, input)| {
                    let builder = UnitBuilder::new(&project_root_for_pool);
                    let builder = if let Some(build) = salsa_build {
                        builder.with_salsa_build(build)
                    } else {
                        builder
                    };
                    match builder.build_unit(&input.path, &input.source) {
                        Ok((unit, hir)) => Ok((discovered_index, input.is_entry, unit, hir)),
                        Err(AssemblyError::Parse { path, message })
                            if options.skip_parse_errors && !input.is_entry =>
                        {
                            log::warn!("skipping unparseable unit {} ({message})", path.display());
                            Err(AssemblyError::Parse {
                                path,
                                message: "skipped".to_string(),
                            })
                        }
                        Err(err) => Err(err),
                    }
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

    if units.is_empty() {
        return Err(AssemblyError::EntryNotFound {
            path: entry_path.to_path_buf(),
        });
    }

    let disk_stats = disk_cache_stats();
    log::debug!(
        "assembly artifact cache hits={} misses={}",
        disk_stats.hits,
        disk_stats.misses
    );
    let _ = beskid_artifacts::ArtifactStore::new(&project_root).refresh_manifest();

    let hir_units = Arc::new(hir_units_vec);
    let prelude_module_paths: Vec<Vec<String>> = prelude_reexport_paths
        .iter()
        .map(|path| path.split('.').map(String::from).collect())
        .collect();
    let module_index = Arc::new(ModuleIndex::build(
        &units,
        hir_units.as_ref(),
        entry_index,
        &roots,
        plan,
        prelude_module_paths,
    ));

    Ok(ProgramAssembly {
        roots,
        units: Arc::new(units),
        hir_units,
        entry_index,
        discovery: options.discovery,
        module_index,
        has_std_dependency: plan.has_std_dependency,
    })
}

fn collect_bd_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read_dir) = fs::read_dir(root) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_bd_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("bd") {
            out.push(path);
        }
    }
}

pub(crate) fn import_paths_from_source_full(source: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        if let Some(import_path) = parse_use_import_path(trimmed) {
            paths.push(import_path);
        }
    }
    paths
}

fn parse_use_import_path(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix("use ")?;
    let without_comment = rest.split("//").next()?.trim_end_matches(';').trim();
    let import_path = without_comment
        .split_once(" as ")
        .map(|(path, _)| path.trim())
        .unwrap_or(without_comment);
    (!import_path.is_empty()).then(|| import_path.to_string())
}

fn is_compiler_mod_sdk_source_root(root: &Path) -> bool {
    let root_str = root.to_string_lossy();
    root_str.contains("compiler_sdk") || root_str.contains("compiler-sdk")
}
