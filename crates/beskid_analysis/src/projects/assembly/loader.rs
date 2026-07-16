//! Discover, parse, and index compilation units for a compile plan.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use beskid_pipeline::{PipelineObserver, phases::PROGRAM_ASSEMBLE, report_progress};
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
use crate::syntax::{Node, Program, Spanned};

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

pub(crate) fn expand_syntax_for_assembly(program: Spanned<Program>) -> Spanned<Program> {
    crate::macros::expand_program_with_diagnostics(
        program,
        crate::macros::DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
        "",
        "",
    )
    .program
}

/// Default assembly options for a compile plan.
///
/// Targets with an explicit `entry` (Lib/App test entrypoints) use import-closure discovery;
/// aggregate / IDE-style plans without `entry` scan the workspace.
pub fn assembly_options_for_plan(plan: &CompilePlan) -> AssemblyOptions {
    let mut options = AssemblyOptions::default();
    if plan.target.entry.as_deref().unwrap_or("").trim().is_empty() {
        options.discovery = AssemblyDiscovery::WorkspaceScan;
    } else {
        options.discovery = AssemblyDiscovery::ImportClosure;
    }
    options
}

/// Merge plan-derived discovery with an explicit front-end override.
///
/// [`AssemblyDiscovery::ImportClosure`] in `front_end_discovery` means "use the plan default"
/// (import closure when `entry` is set, workspace scan when it is not). Any other mode overrides.
pub fn assembly_options_for_prepare(
    plan: &CompilePlan,
    front_end_discovery: AssemblyDiscovery,
) -> AssemblyOptions {
    let mut options = assembly_options_for_plan(plan);
    if front_end_discovery != AssemblyDiscovery::ImportClosure {
        options.discovery = front_end_discovery;
    }
    options
}

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
    assemble_program_with_materializer(
        plan,
        workspace,
        entry_path,
        entry_source,
        options,
        None,
        pipeline,
    )
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

    let entry_canonical = entry_path
        .canonicalize()
        .unwrap_or_else(|_| entry_path.to_path_buf());

    let scan_without_entry = options.discovery == AssemblyDiscovery::WorkspaceScan
        && plan.target.entry.as_deref().unwrap_or("").trim().is_empty();

    if !scan_without_entry && !entry_canonical.is_file() {
        return Err(AssemblyError::EntryNotFound {
            path: entry_path.to_path_buf(),
        });
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
                    if let Some(parent_import) = parent_module_import_path(&import_path)
                        && let Some(parent_file) = resolve_module_file(&parent_import, &roots)
                    {
                        queue.push_back(parent_file);
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
        tracing::warn!(
            target: "beskid.analysis.assembly",
            project_root = %project_root.display(),
            error = %err,
            "unit cache manifest skipped"
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
                            tracing::warn!(
                                target: "beskid.analysis.assembly",
                                file = %path.display(),
                                error = %source,
                                "skipping unreadable unit"
                            );
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
    let build_total = build_inputs.len() as u64;
    let build_done = AtomicU64::new(0);
    let built_units: Result<Vec<(usize, bool, SourceUnit, super::UnitHir)>, AssemblyError> = pool
        .install(|| {
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
                    let builder = if let Some(build) = salsa_build {
                        builder.with_salsa_build(build)
                    } else {
                        builder
                    };
                    let label = unit_progress_label(&input.path);
                    let result = match builder.build_unit(&input.path, &input.source) {
                        Ok((unit, hir)) => {
                            let done = build_done.fetch_add(1, Ordering::Relaxed) + 1;
                            report_progress(
                                pipeline,
                                PROGRAM_ASSEMBLE,
                                done,
                                build_total.max(1),
                                label,
                            );
                            Ok((discovered_index, input.is_entry, unit, hir))
                        }
                        Err(AssemblyError::Parse { path, message })
                            if options.skip_parse_errors && !input.is_entry =>
                        {
                            tracing::warn!(
                                target: "beskid.analysis.assembly",
                                unit = %path.display(),
                                file = %path.display(),
                                error = %message,
                                "skipping unparseable unit"
                            );
                            Err(AssemblyError::Parse {
                                path,
                                message: "skipped".to_string(),
                            })
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
        return Err(AssemblyError::EntryNotFound {
            path: entry_path.to_path_buf(),
        });
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
    let module_index = Arc::new(ModuleIndex::build(
        &units,
        hir_units.as_ref(),
        entry_index,
        &roots,
        plan,
        prefetch_dependency_roots,
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

/// Out-of-line module dependencies declared by parsed syntax (`pub mod A.B;`).
///
/// Import closure intentionally treats an unparseable source as contributing no extra module
/// declarations: the regular unit build remains the authority for reporting that parse error,
/// while discovery does not guess at a declaration from stale or malformed text.
pub(crate) fn module_declaration_paths_from_source(path: &Path, source: &str) -> Vec<String> {
    let logical_name = path.display().to_string();
    let Ok(program) = crate::services::parse_program_with_source_name(&logical_name, source) else {
        return Vec::new();
    };

    program
        .node
        .items
        .iter()
        .filter_map(|item| match &item.node {
            Node::ModuleDeclaration(declaration) => Some(
                declaration
                    .node
                    .path
                    .node
                    .segments
                    .iter()
                    .map(|segment| segment.node.name.node.name.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
            ),
            _ => None,
        })
        .filter(|module_path| !module_path.is_empty())
        .collect()
}

/// Module path prefixes from qualified references (`Core.Results.Result`, `Core.Syscall.WriteWith`).
pub(crate) fn module_paths_from_qualified_references(source: &str) -> Vec<String> {
    use std::collections::HashSet;
    let mut paths = HashSet::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("use ") {
            continue;
        }
        for dotted in find_dotted_module_references(trimmed) {
            let segments: Vec<&str> = dotted
                .split('.')
                .filter(|segment| !segment.is_empty())
                .collect();
            if segments.len() < 2 {
                continue;
            }
            for len in 1..segments.len() {
                paths.insert(segments[..len].join("."));
            }
        }
    }
    paths.into_iter().collect()
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_part(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn find_dotted_module_references(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if !is_ident_start(bytes[index]) || !bytes[index].is_ascii_uppercase() {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_ident_part(bytes[index]) {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'.' {
            continue;
        }
        let mut end = index;
        loop {
            if end >= bytes.len() || bytes[end] != b'.' {
                break;
            }
            end += 1;
            if end >= bytes.len() || !is_ident_start(bytes[end]) {
                break;
            }
            while end < bytes.len() && is_ident_part(bytes[end]) {
                end += 1;
            }
        }
        if end > start + 1 {
            out.push(line[start..end].to_string());
        }
    }
    out
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

/// When a unit imports nested symbols (`Core.Syscall.ReadRequest`), also pull in the
/// parent module facade (`Core/Syscall/Syscall.bd`) that hosts sibling functions referenced via
/// qualified paths (`Core.Syscall.ReadWith`) without an explicit `use`.
pub(crate) fn parent_module_import_path(import_path: &str) -> Option<String> {
    let segments: Vec<&str> = import_path
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.len() <= 2 {
        return None;
    }
    Some(segments[..segments.len() - 1].join("."))
}

fn unit_progress_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::projects::{
        AssemblyDiscovery, AssemblyError, AssemblyOptions, CompilePlan, ResolvedDependencyProject,
        Target, TargetKind, assemble_program, assembly_options_for_plan,
        assembly_options_for_prepare, plan_entry_path,
    };

    fn temp_project_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("beskid_asm_{label}_{nanos}"))
    }

    fn write_bd(root: &Path, relative: &str, source: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(path, source).expect("write bd source");
    }

    fn no_entry_plan_with_source(source: &str) -> (CompilePlan, PathBuf) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let project_root = std::env::temp_dir().join(format!("beskid_asm_test_{nanos}"));
        let source_root = project_root.join("src");
        fs::create_dir_all(&source_root).expect("create source root");
        fs::write(source_root.join("Main.bd"), source).expect("write Main.bd");
        let plan = CompilePlan {
            source_root: source_root.clone(),
            project_root: project_root.clone(),
            manifest_path: project_root.join("project.bproj"),
            project_name: "fixture".to_string(),
            target: Target {
                name: "__aggregate__".to_string(),
                kind: TargetKind::Lib,
                entry: None,
            },
            dependency_projects: Vec::new(),
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        };
        let entry_path = plan_entry_path(&plan, &source_root);
        (plan, entry_path)
    }

    #[test]
    fn no_entry_plan_uses_workspace_scan_discovery() {
        let (plan, _) = no_entry_plan_with_source("pub fn Main() { }");
        let options = assembly_options_for_plan(&plan);
        assert_eq!(options.discovery, AssemblyDiscovery::WorkspaceScan);
    }

    #[test]
    fn entry_plan_uses_import_closure_discovery() {
        let (mut plan, _) = no_entry_plan_with_source("pub fn Main() { }");
        plan.target.entry = Some("Main.bd".to_string());
        let options = assembly_options_for_plan(&plan);
        assert_eq!(options.discovery, AssemblyDiscovery::ImportClosure);
    }

    #[test]
    fn qualified_reference_scan_finds_module_prefixes() {
        let source =
            "Core.Results.Result<i64, SyscallError> Write() { Core.Syscall.WriteWith(x); }";
        let paths = super::module_paths_from_qualified_references(source);
        assert!(paths.contains(&"Core.Results".to_string()));
        assert!(paths.contains(&"Core".to_string()));
        assert!(paths.contains(&"Core.Syscall".to_string()));
    }

    #[test]
    fn workspace_scan_assembles_without_placeholder_entry_file() {
        let (plan, entry_path) = no_entry_plan_with_source("pub fn Main() { }");
        let options = assembly_options_for_plan(&plan);
        assert!(
            !entry_path.is_file(),
            "placeholder entry should not exist: {}",
            entry_path.display()
        );

        let assembly = assemble_program(&plan, None, &entry_path, Some(""), &options, None)
            .expect("workspace scan should assemble units without a real entry file");
        assert!(!assembly.units.is_empty());
        let _ = fs::remove_dir_all(&plan.project_root);
    }

    #[test]
    fn import_closure_still_requires_entry_file() {
        let (plan, entry_path) = no_entry_plan_with_source("pub fn Main() { }");
        let mut options = assembly_options_for_plan(&plan);
        options.discovery = AssemblyDiscovery::ImportClosure;
        let err = assemble_program(&plan, None, &entry_path, Some(""), &options, None)
            .expect_err("import closure without entry file should fail");
        assert!(
            matches!(err, AssemblyError::EntryNotFound { .. }),
            "unexpected error: {err}"
        );
        let _ = fs::remove_dir_all(&plan.project_root);
    }

    #[test]
    fn prepare_options_use_plan_default_when_front_end_is_import_closure() {
        let (plan, _) = no_entry_plan_with_source("pub fn Main() { }");
        let options = assembly_options_for_prepare(&plan, AssemblyDiscovery::ImportClosure);
        assert_eq!(options.discovery, AssemblyDiscovery::WorkspaceScan);

        let mut entry_plan = plan.clone();
        entry_plan.target.entry = Some("Main.bd".to_string());
        let options = assembly_options_for_prepare(&entry_plan, AssemblyDiscovery::ImportClosure);
        assert_eq!(options.discovery, AssemblyDiscovery::ImportClosure);
        let _ = fs::remove_dir_all(&plan.project_root);
    }

    #[test]
    fn prepare_options_honor_explicit_front_end_override() {
        let (mut plan, _) = no_entry_plan_with_source("pub fn Main() { }");
        plan.target.entry = Some("Main.bd".to_string());
        let options = assembly_options_for_prepare(&plan, AssemblyDiscovery::WorkspaceScan);
        assert_eq!(options.discovery, AssemblyDiscovery::WorkspaceScan);
        let _ = fs::remove_dir_all(&plan.project_root);
    }

    #[test]
    fn import_closure_assembles_entry_without_sibling_units() {
        let project_root = temp_project_root("import_closure_entry_only");
        let source_root = project_root.join("src");
        write_bd(&source_root, "Entry.bd", "pub fn Entry() { }");
        write_bd(&source_root, "Sibling.bd", "pub fn Sibling() { }");
        let plan = CompilePlan {
            source_root: source_root.clone(),
            project_root: project_root.clone(),
            manifest_path: project_root.join("project.bproj"),
            project_name: "fixture".to_string(),
            target: Target {
                name: "Entry".to_string(),
                kind: TargetKind::Lib,
                entry: Some("Entry.bd".to_string()),
            },
            dependency_projects: Vec::new(),
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        };
        let entry_path = source_root.join("Entry.bd");
        let options = assembly_options_for_plan(&plan);
        let assembly = assemble_program(&plan, None, &entry_path, None, &options, None)
            .expect("import closure should assemble entry");
        assert_eq!(assembly.units.len(), 1);
        assert_eq!(assembly.discovery, AssemblyDiscovery::ImportClosure);
        assert!(
            assembly.units.iter().all(
                |unit| unit.path.file_name().and_then(|name| name.to_str()) == Some("Entry.bd")
            ),
            "unexpected units: {:?}",
            assembly
                .units
                .iter()
                .map(|unit| unit.path.display().to_string())
                .collect::<Vec<_>>()
        );
        let _ = fs::remove_dir_all(&project_root);
    }

    #[test]
    fn import_closure_follows_transitive_use_imports() {
        let project_root = temp_project_root("import_closure_transitive");
        let source_root = project_root.join("src");
        write_bd(
            &source_root,
            "Entry.bd",
            "use Lib.A;\npub fn Entry() { Lib.A.Run(); }",
        );
        write_bd(
            &source_root,
            "Lib/A.bd",
            "use Lib.B;\npub fn Run() { Lib.B.Run(); }",
        );
        write_bd(&source_root, "Lib/B.bd", "pub fn Run() { }");
        write_bd(&source_root, "Unused.bd", "pub fn Unused() { }");
        let plan = CompilePlan {
            source_root: source_root.clone(),
            project_root: project_root.clone(),
            manifest_path: project_root.join("project.bproj"),
            project_name: "fixture".to_string(),
            target: Target {
                name: "Entry".to_string(),
                kind: TargetKind::Lib,
                entry: Some("Entry.bd".to_string()),
            },
            dependency_projects: Vec::new(),
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        };
        let entry_path = source_root.join("Entry.bd");
        let options = assembly_options_for_plan(&plan);
        let assembly = assemble_program(&plan, None, &entry_path, None, &options, None)
            .expect("import closure should follow transitive imports");
        let names: Vec<String> = assembly
            .units
            .iter()
            .map(|unit| {
                unit.path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names.len(), 3);
        assert!(names.iter().any(|name| name == "Entry.bd"));
        assert!(names.iter().any(|name| name == "A.bd"));
        assert!(names.iter().any(|name| name == "B.bd"));
        assert!(!names.iter().any(|name| name == "Unused.bd"));
        let _ = fs::remove_dir_all(&project_root);
    }

    #[test]
    fn import_closure_follows_public_module_declarations() {
        let project_root = temp_project_root("import_closure_public_module");
        let source_root = project_root.join("src");
        write_bd(
            &source_root,
            "Entry.bd",
            "use Core.Text.Regex;\npub fn Entry() { Core.Text.Regex.Parse(); }",
        );
        write_bd(
            &source_root,
            "Core/Text/Regex.bd",
            "pub mod Core.Text.Regex.Generated;\npub fn Parse() { Core.Text.Regex.Generated.ParsePat(); }",
        );
        write_bd(
            &source_root,
            "Core/Text/Regex/Generated.bd",
            "pub fn ParsePat() { }",
        );
        let plan = CompilePlan {
            source_root: source_root.clone(),
            project_root: project_root.clone(),
            manifest_path: project_root.join("project.bproj"),
            project_name: "fixture".to_string(),
            target: Target {
                name: "Entry".to_string(),
                kind: TargetKind::Lib,
                entry: Some("Entry.bd".to_string()),
            },
            dependency_projects: Vec::new(),
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        };
        let assembly = assemble_program(
            &plan,
            None,
            &source_root.join("Entry.bd"),
            None,
            &assembly_options_for_plan(&plan),
            None,
        )
        .expect("public module declaration should extend import closure");
        let loaded: Vec<_> = assembly.units.iter().map(|unit| &unit.path).collect();
        assert!(
            loaded.iter().any(|path| path.ends_with("Entry.bd")),
            "expected entry in closure, got: {loaded:?}"
        );
        assert!(
            loaded
                .iter()
                .any(|path| path.ends_with("Core/Text/Regex.bd")),
            "expected declared module owner in closure, got: {loaded:?}"
        );
        assert!(
            loaded
                .iter()
                .any(|path| path.ends_with("Core/Text/Regex/Generated.bd")),
            "expected declared generated module in closure, got: {loaded:?}"
        );
        let _ = fs::remove_dir_all(&project_root);
    }

    #[test]
    fn import_closure_follows_public_module_declarations_into_generated_sources() {
        let project_root = temp_project_root("import_closure_generated_public_module");
        let source_root = project_root.join("src");
        write_bd(
            &source_root,
            "Entry.bd",
            "use Core.Text.Regex;\npub fn Entry() { Core.Text.Regex.Parse(); }",
        );
        write_bd(
            &source_root,
            "Core/Text/Regex.bd",
            "pub mod Core.Text.Regex.Generated;\npub fn Parse() { Core.Text.Regex.Generated.ParsePat(); }",
        );
        write_bd(
            &project_root.join(".generated"),
            "Core/Text/Regex/Generated.g.bd",
            "pub fn ParsePat() { }",
        );
        let plan = CompilePlan {
            source_root: source_root.clone(),
            project_root: project_root.clone(),
            manifest_path: project_root.join("project.bproj"),
            project_name: "fixture".to_string(),
            target: Target {
                name: "Entry".to_string(),
                kind: TargetKind::Lib,
                entry: Some("Entry.bd".to_string()),
            },
            dependency_projects: Vec::new(),
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        };
        let assembly = assemble_program(
            &plan,
            None,
            &source_root.join("Entry.bd"),
            None,
            &assembly_options_for_plan(&plan),
            None,
        )
        .expect("public module declaration should resolve its generated source");
        let loaded: Vec<_> = assembly.units.iter().map(|unit| &unit.path).collect();
        assert!(
            loaded
                .iter()
                .any(|path| path.ends_with(".generated/Core/Text/Regex/Generated.g.bd")),
            "expected generated declared module in closure, got: {loaded:?}"
        );
        let _ = fs::remove_dir_all(&project_root);
    }

    #[test]
    fn import_closure_ignores_missing_public_module_declarations() {
        let project_root = temp_project_root("import_closure_missing_public_module");
        let source_root = project_root.join("src");
        write_bd(
            &source_root,
            "Entry.bd",
            "pub mod Core.Text.DoesNotExist;\npub fn Entry() { }",
        );
        let plan = CompilePlan {
            source_root: source_root.clone(),
            project_root: project_root.clone(),
            manifest_path: project_root.join("project.bproj"),
            project_name: "fixture".to_string(),
            target: Target {
                name: "Entry".to_string(),
                kind: TargetKind::Lib,
                entry: Some("Entry.bd".to_string()),
            },
            dependency_projects: Vec::new(),
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        };
        let assembly = assemble_program(
            &plan,
            None,
            &source_root.join("Entry.bd"),
            None,
            &assembly_options_for_plan(&plan),
            None,
        )
        .expect("absent module declaration target should not invalidate existing closure");
        assert_eq!(assembly.units.len(), 1);
        let _ = fs::remove_dir_all(&project_root);
    }

    #[test]
    fn import_closure_terminates_public_module_declaration_cycles() {
        let project_root = temp_project_root("import_closure_public_module_cycle");
        let source_root = project_root.join("src");
        write_bd(&source_root, "Entry.bd", "use Core.A;\npub fn Entry() { }");
        write_bd(&source_root, "Core/A.bd", "pub mod Core.B;\npub fn A() { }");
        write_bd(&source_root, "Core/B.bd", "pub mod Core.A;\npub fn B() { }");
        let plan = CompilePlan {
            source_root: source_root.clone(),
            project_root: project_root.clone(),
            manifest_path: project_root.join("project.bproj"),
            project_name: "fixture".to_string(),
            target: Target {
                name: "Entry".to_string(),
                kind: TargetKind::Lib,
                entry: Some("Entry.bd".to_string()),
            },
            dependency_projects: Vec::new(),
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        };
        let assembly = assemble_program(
            &plan,
            None,
            &source_root.join("Entry.bd"),
            None,
            &assembly_options_for_plan(&plan),
            None,
        )
        .expect("module declaration cycles should be de-duplicated");
        assert_eq!(assembly.units.len(), 3);
        let _ = fs::remove_dir_all(&project_root);
    }

    #[test]
    fn workspace_scan_assembles_all_host_sources() {
        let project_root = temp_project_root("workspace_scan_all");
        let source_root = project_root.join("src");
        write_bd(&source_root, "Main.bd", "pub fn Main() { }");
        write_bd(&source_root, "Other.bd", "pub fn Other() { }");
        let plan = CompilePlan {
            source_root: source_root.clone(),
            project_root: project_root.clone(),
            manifest_path: project_root.join("project.bproj"),
            project_name: "fixture".to_string(),
            target: Target {
                name: "__aggregate__".to_string(),
                kind: TargetKind::Lib,
                entry: None,
            },
            dependency_projects: Vec::new(),
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        };
        let entry_path = plan_entry_path(&plan, &source_root);
        let options = assembly_options_for_plan(&plan);
        let assembly = assemble_program(&plan, None, &entry_path, Some(""), &options, None)
            .expect("workspace scan should assemble every host unit");
        assert_eq!(assembly.discovery, AssemblyDiscovery::WorkspaceScan);
        assert_eq!(assembly.units.len(), 2);
        let _ = fs::remove_dir_all(&project_root);
    }

    #[test]
    fn import_closure_module_index_skips_unimported_dependency_tree() {
        let project_root = temp_project_root("import_closure_dep_prefetch");
        let source_root = project_root.join("src");
        let dep_root = project_root.join("deps").join("core");
        let dep_source_root = dep_root.join("src");
        write_bd(&source_root, "Entry.bd", "pub fn Entry() { }");
        for index in 0..8 {
            write_bd(
                &dep_source_root,
                &format!("Shard{index}.bd"),
                &format!("pub fn Shard{index}() {{ }}"),
            );
        }
        let plan = CompilePlan {
            source_root: source_root.clone(),
            project_root: project_root.clone(),
            manifest_path: project_root.join("project.bproj"),
            project_name: "fixture".to_string(),
            target: Target {
                name: "Entry".to_string(),
                kind: TargetKind::Lib,
                entry: Some("Entry.bd".to_string()),
            },
            dependency_projects: vec![ResolvedDependencyProject {
                dependency_name: "core".to_string(),
                manifest_path: dep_root.join("core.bproj"),
                project_root: dep_root.clone(),
                project_name: "core".to_string(),
                source_root: dep_source_root.clone(),
            }],
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        };
        let entry_path = source_root.join("Entry.bd");
        let options = AssemblyOptions {
            discovery: AssemblyDiscovery::ImportClosure,
            ..AssemblyOptions::default()
        };
        let assembly = assemble_program(&plan, None, &entry_path, None, &options, None)
            .expect("import closure should assemble entry without dependency units");
        assert_eq!(assembly.units.len(), 1);
        assert!(
            assembly.module_index.prefetched_paths().is_empty(),
            "expected no dependency prefetch for zero-import entry, got {} paths",
            assembly.module_index.prefetched_paths().len()
        );

        let scan_options = AssemblyOptions {
            discovery: AssemblyDiscovery::WorkspaceScan,
            ..AssemblyOptions::default()
        };
        let scanned = assemble_program(&plan, None, &entry_path, None, &scan_options, None)
            .expect("workspace scan should assemble host and prefetch dependency tree");
        assert!(
            scanned.units.len() >= 9,
            "workspace scan should assemble host and dependency shards as units, got {}",
            scanned.units.len()
        );
        let _ = fs::remove_dir_all(&project_root);
    }
}
