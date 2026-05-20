//! Beskid runtime static library: prebuilt archive validation or standalone (no runtime).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use beskid_abi::{BESKID_RUNTIME_ABI_VERSION, RUNTIME_EXPORT_SYMBOLS};
use object::read::archive::ArchiveFile;
use object::{Object, ObjectSymbol};

use crate::api::RuntimeStrategy;
use crate::error::{AotError, AotResult};

/// Optional path to a `.a`/`.lib` to pass the linker, plus symbols re-exported for tests/tooling.
#[derive(Debug, Clone)]
pub struct RuntimeArtifact {
    pub staticlib_path: Option<PathBuf>,
    pub exported_symbols: Vec<String>,
}

/// Collect exported symbol names from every object embedded in a static `ar` archive.
///
/// System `nm` from Xcode cannot parse LLVM object files produced by newer Rust
/// toolchains (it exits non-zero with "Unknown attribute kind"), so we use the
/// `object` crate—the same stack as codegen—for inspection.
fn static_archive_symbol_names(path: &Path) -> AotResult<Vec<String>> {
    let data = fs::read(path).map_err(|err| AotError::RuntimeBuild {
        message: format!("failed to read runtime archive `{}`: {err}", path.display()),
    })?;
    let archive = ArchiveFile::parse(data.as_slice()).map_err(|err| AotError::RuntimeBuild {
        message: format!(
            "failed to parse runtime archive `{}`: {err:#}",
            path.display()
        ),
    })?;
    let mut names = Vec::new();
    for member in archive.members() {
        let Ok(member) = member else {
            continue;
        };
        let Ok(member_data) = member.data(data.as_slice()) else {
            continue;
        };
        if member_data.is_empty() {
            continue;
        }
        let Ok(obj) = object::read::File::parse(member_data) else {
            continue;
        };
        for symbol in obj.symbols() {
            if let Ok(name) = symbol.name() {
                names.push(name.to_owned());
            }
        }
    }
    Ok(names)
}

fn present_export_symbols(names: &[String]) -> BTreeSet<&str> {
    let mut present = BTreeSet::new();
    for name in names {
        present.insert(name.as_str());
        if let Some(stripped) = name.strip_prefix('_') {
            present.insert(stripped);
        }
    }
    present
}

fn missing_required_symbols<'a>(present: &BTreeSet<&str>, required: &[&'a str]) -> Vec<&'a str> {
    required
        .iter()
        .copied()
        .filter(|symbol| !present.contains(symbol))
        .collect()
}

fn ensure_runtime_symbols_present(archive_path: &Path, required: &[&str]) -> AotResult<()> {
    let names = static_archive_symbol_names(archive_path)?;
    let present = present_export_symbols(&names);
    let missing = missing_required_symbols(&present, required);
    if !missing.is_empty() {
        return Err(AotError::RuntimeBuild {
            message: format!(
                "runtime archive `{}` is missing required symbols: {}",
                archive_path.display(),
                missing.join(", ")
            ),
        });
    }

    Ok(())
}

/// How the AOT pipeline obtains a runtime static library to link against.
#[derive(Debug, Clone)]
pub struct RuntimeBuildRequest {
    pub strategy: RuntimeStrategy,
}

/// Resolve the runtime archive according to `req.strategy`.
pub fn prepare_runtime(req: &RuntimeBuildRequest) -> AotResult<RuntimeArtifact> {
    match &req.strategy {
        RuntimeStrategy::Standalone => Ok(RuntimeArtifact {
            staticlib_path: None,
            exported_symbols: Vec::new(),
        }),
        RuntimeStrategy::UsePrebuilt { path, abi_version } => {
            if *abi_version != BESKID_RUNTIME_ABI_VERSION {
                return Err(AotError::RuntimeAbiMismatch {
                    expected: BESKID_RUNTIME_ABI_VERSION,
                    actual: *abi_version,
                });
            }
            if !path.exists() {
                return Err(AotError::RuntimeArchiveMissing { path: path.clone() });
            }
            ensure_runtime_symbols_present(path, RUNTIME_EXPORT_SYMBOLS)?;
            Ok(RuntimeArtifact {
                staticlib_path: Some(path.clone()),
                exported_symbols: runtime_symbols(),
            })
        }
    }
}

fn runtime_symbols() -> Vec<String> {
    RUNTIME_EXPORT_SYMBOLS
        .iter()
        .map(|symbol| (*symbol).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_abi::SYM_ABI_VERSION;

    #[test]
    fn missing_required_symbols_returns_empty_when_all_present() {
        let names = vec![
            "_alloc".to_owned(),
            "_beskid_runtime_abi_version".to_owned(),
        ];
        let present = present_export_symbols(&names);
        let missing = missing_required_symbols(&present, &[SYM_ABI_VERSION, "alloc"]);
        assert!(missing.is_empty());
    }

    #[test]
    fn missing_required_symbols_detects_absent_entries() {
        let names = vec!["_alloc".to_owned()];
        let present = present_export_symbols(&names);
        let missing =
            missing_required_symbols(&present, &[SYM_ABI_VERSION, "alloc", "str_new"]);
        assert_eq!(missing, vec![SYM_ABI_VERSION, "str_new"]);
    }
}
