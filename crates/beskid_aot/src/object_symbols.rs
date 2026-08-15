//! Symbol-table extraction for emitted objects, archives, and linked images.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use object::read::archive::ArchiveFile;
use object::{FileKind, Object, ObjectSymbol};

use crate::api::NativeSymbolInventory;
use crate::error::{AotError, AotResult};

pub(crate) fn extract_symbol_inventory(path: &Path, symbol_prefix: &str) -> AotResult<NativeSymbolInventory> {
    let data = fs::read(path).map_err(|error| AotError::Io { path: path.to_owned(), message: error.to_string() })?;
    let mut defined = BTreeSet::new();
    let mut imported = BTreeSet::new();
    let kind = FileKind::parse(data.as_slice()).map_err(|error| AotError::ObjectModule {
        message: format!("cannot identify symbol-bearing artifact {}: {error}", path.display()),
    })?;
    if matches!(kind, FileKind::Archive) {
        let archive = ArchiveFile::parse(data.as_slice()).map_err(|error| AotError::ObjectModule {
            message: format!("cannot parse archive {}: {error}", path.display()),
        })?;
        for member in archive.members() {
            let member = member.map_err(|error| AotError::ObjectModule {
                message: format!("cannot parse member of {}: {error}", path.display()),
            })?;
            let bytes = member.data(data.as_slice()).map_err(|error| AotError::ObjectModule {
                message: format!("cannot read member of {}: {error}", path.display()),
            })?;
            if bytes.is_empty() || FileKind::parse(bytes).is_err() {
                continue;
            }
            extract_object_symbols(bytes, symbol_prefix, &mut defined, &mut imported, path)?;
        }
    } else {
        extract_object_symbols(data.as_slice(), symbol_prefix, &mut defined, &mut imported, path)?;
    }
    imported.retain(|symbol| !defined.contains(symbol));
    Ok(NativeSymbolInventory {
        artifact: path.to_owned(),
        defined: defined.into_iter().collect(),
        imported: imported.into_iter().collect(),
    })
}

fn extract_object_symbols(
    bytes: &[u8],
    symbol_prefix: &str,
    defined: &mut BTreeSet<String>,
    imported: &mut BTreeSet<String>,
    path: &Path,
) -> AotResult<()> {
    let object = object::read::File::parse(bytes).map_err(|error| AotError::ObjectModule {
        message: format!("cannot parse symbols from {}: {error}", path.display()),
    })?;
    for symbol in object.symbols().chain(object.dynamic_symbols()) {
        if !symbol.is_global() && !symbol.is_weak() {
            continue;
        }
        let Ok(raw) = symbol.name() else { continue };
        if raw.is_empty() {
            continue;
        }
        let normalized = normalize_target_symbol(raw, symbol_prefix);
        if symbol.is_undefined() {
            imported.insert(normalized);
        } else if symbol.section_index().is_some() || symbol.is_common() {
            defined.insert(normalized);
        }
    }
    Ok(())
}

fn normalize_target_symbol(symbol: &str, symbol_prefix: &str) -> String {
    symbol.strip_prefix(symbol_prefix).unwrap_or(symbol).to_owned()
}

#[cfg(test)]
mod tests {
    use super::normalize_target_symbol;

    #[test]
    fn target_prefix_is_normalized_only_at_extraction_seam() {
        assert_eq!(normalize_target_symbol("_beskid_rt_v5_abi_version", "_"), "beskid_rt_v5_abi_version");
        assert_eq!(normalize_target_symbol("beskid_rt_v5_abi_version", "_"), "beskid_rt_v5_abi_version");
        assert_eq!(normalize_target_symbol("beskid_rt_v5_abi_version", ""), "beskid_rt_v5_abi_version");
    }
}
