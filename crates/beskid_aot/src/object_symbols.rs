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
    for symbol in object.symbols() {
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
    // Dynamic symbol tables supplement ordinary symbol tables for linked ELF/Mach-O images.
    // PE/COFF does not expose its image directories through this iterator, so those are
    // collected separately below.
    for symbol in object.dynamic_symbols() {
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
        } else {
            defined.insert(normalized);
        }
    }
    for export in object.exports().map_err(|error| AotError::ObjectModule {
        message: format!("cannot parse export directory from {}: {error}", path.display()),
    })? {
        if let Some(normalized) = normalize_directory_symbol(export.name(), symbol_prefix, "export", path)? {
            defined.insert(normalized);
        }
    }
    for import in object.imports().map_err(|error| AotError::ObjectModule {
        message: format!("cannot parse import directory from {}: {error}", path.display()),
    })? {
        if let Some(normalized) = normalize_directory_symbol(import.name(), symbol_prefix, "import", path)? {
            imported.insert(normalized);
        }
    }
    Ok(())
}

fn normalize_directory_symbol(
    symbol: &[u8],
    symbol_prefix: &str,
    table: &str,
    path: &Path,
) -> AotResult<Option<String>> {
    if symbol.is_empty() {
        return Ok(None);
    }
    let symbol = std::str::from_utf8(symbol).map_err(|error| AotError::ObjectModule {
        message: format!("cannot decode {table} symbol from {} as UTF-8: {error}", path.display()),
    })?;
    Ok(Some(normalize_target_symbol(symbol, symbol_prefix)))
}

fn normalize_target_symbol(symbol: &str, symbol_prefix: &str) -> String {
    symbol.strip_prefix(symbol_prefix).unwrap_or(symbol).to_owned()
}

#[cfg(test)]
mod tests {
    use super::{extract_symbol_inventory, normalize_target_symbol};

    #[test]
    fn pe_export_and_import_directories_are_symbol_sources() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pe_export_table.dll");
        let inventory = extract_symbol_inventory(&path, "").expect("parse PE fixture");

        assert_eq!(inventory.defined, ["expected_export"]);
        assert_eq!(inventory.imported, ["expected_import"]);
    }

    #[test]
    fn target_prefix_is_normalized_only_at_extraction_seam() {
        assert_eq!(normalize_target_symbol("_beskid_rt_v5_abi_version", "_"), "beskid_rt_v5_abi_version");
        assert_eq!(normalize_target_symbol("beskid_rt_v5_abi_version", "_"), "beskid_rt_v5_abi_version");
        assert_eq!(normalize_target_symbol("beskid_rt_v5_abi_version", ""), "beskid_rt_v5_abi_version");
    }
}
