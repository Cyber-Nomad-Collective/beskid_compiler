//! Discover, parse, and index compilation units for a compile plan.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::projects::model::{AssemblyDiscovery, AssemblyOptions};
use crate::projects::{CompilePlan, PreparedProjectWorkspace};
use crate::services::parse_program_with_source_name;
use crate::syntax::{Program, Spanned};

use super::discovery::resolve_module_file;
use super::module_index::ModuleIndex;
use super::roots::effective_roots_for_plan;
use super::{ProgramAssembly, SourceUnit};

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

fn expand_syntax_for_assembly(program: Spanned<Program>) -> Spanned<Program> {
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
    let roots = effective_roots_for_plan(plan, workspace);
    let module_roots: Vec<PathBuf> = super::roots::module_roots_from_effective(&roots);

    let entry_canonical = entry_path
        .canonicalize()
        .unwrap_or_else(|_| entry_path.to_path_buf());

    let mut discovered: Vec<PathBuf> = Vec::new();
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

    let mut prelude_seeds = Vec::new();
    let mut prelude_reexport_paths = Vec::new();
    if options.include_std_prelude {
        for root in &module_roots {
            if is_compiler_mod_sdk_source_root(root) {
                continue;
            }
            let prelude = root.join("Prelude.bd");
            if !prelude.is_file() || prelude_seeds.contains(&prelude) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&prelude) else {
                continue;
            };
            if text.contains("pub mod Testing.Assertions") {
                for module_path in [
                    "Testing.Assertions",
                    "Testing.Contracts",
                    "Core.Results",
                    "Core.String",
                    "Core.ErrorHandling",
                ] {
                    prelude_reexport_paths.push(module_path.to_string());
                }
                break;
            }
        }
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
    seen.clear();

    match options.discovery {
        AssemblyDiscovery::ImportClosure => {
            let mut queue = VecDeque::new();
            queue.push_back(entry_canonical.clone());
            for seed in prelude_seeds {
                queue.push_back(seed);
            }
            for module_path in prelude_reexport_paths {
                if let Some(dep_file) = resolve_module_file(&module_path, &roots) {
                    queue.push_back(dep_file);
                }
            }

            while let Some(path) = queue.pop_front() {
                if discovered.len() >= options.max_units {
                    return Err(AssemblyError::MaxUnits {
                        max: options.max_units,
                    });
                }
                let key = path.canonicalize().unwrap_or_else(|_| path.clone());
                if !seen.insert(key) {
                    continue;
                }
                discovered.push(path.clone());

                let source = fs::read_to_string(&path).map_err(|source| AssemblyError::Read {
                    path: path.clone(),
                    source,
                })?;

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

    let mut units = Vec::with_capacity(discovered.len());
    let mut entry_index = 0usize;
    let entry_key = entry_canonical.canonicalize().unwrap_or(entry_canonical.clone());

    for path in &discovered {
        let path_key = path.canonicalize().unwrap_or_else(|_| path.clone());
        let is_entry = path_key == entry_key;

        let source = if is_entry && entry_source.is_some() {
            entry_source.expect("entry source when is_entry").to_string()
        } else {
            match fs::read_to_string(path) {
                Ok(text) => text,
                Err(source) if options.skip_parse_errors && !is_entry => {
                    log::warn!(
                        "skipping unreadable unit {} ({source})",
                        path.display()
                    );
                    continue;
                }
                Err(source) => {
                    return Err(AssemblyError::Read {
                        path: path.clone(),
                        source,
                    });
                }
            }
        };

        let logical_name = path.display().to_string();
        let program = match parse_program_with_source_name(&logical_name, &source) {
            Ok(program) => expand_syntax_for_assembly(program),
            Err(err) if options.skip_parse_errors && !is_entry => {
                log::warn!(
                    "skipping unparseable unit {} ({err})",
                    path.display()
                );
                continue;
            }
            Err(err) => {
                return Err(AssemblyError::Parse {
                    path: path.clone(),
                    message: err.to_string(),
                });
            }
        };

        if is_entry {
            entry_index = units.len();
        }

        units.push(SourceUnit {
            logical_name,
            path: path.clone(),
            source,
            program,
        });
    }

    if units.is_empty() {
        return Err(AssemblyError::EntryNotFound {
            path: entry_path.to_path_buf(),
        });
    }

    let module_index = ModuleIndex::build(&units, entry_index, &roots, plan);

    Ok(ProgramAssembly {
        roots,
        units,
        entry_index,
        discovery: options.discovery,
        module_index,
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

fn import_paths_from_source_full(source: &str) -> Vec<String> {
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

