//! Symbol-table extraction for emitted objects, archives, and linked images.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use object::read::archive::ArchiveFile;
use object::{BinaryFormat, FileKind, Object, ObjectSymbol};

use crate::api::NativeSymbolInventory;
use crate::error::{AotError, AotResult};

pub(crate) fn extract_symbol_inventory(path: &Path, symbol_prefix: &str) -> AotResult<NativeSymbolInventory> {
    let data = fs::read(path).map_err(|error| AotError::Io { path: path.to_owned(), message: error.to_string() })?;
    extract_symbol_inventory_from_bytes(&data, path, symbol_prefix)
}

fn extract_symbol_inventory_from_bytes(
    data: &[u8],
    path: &Path,
    symbol_prefix: &str,
) -> AotResult<NativeSymbolInventory> {
    let mut defined = BTreeSet::new();
    let mut imported = BTreeSet::new();
    let kind = FileKind::parse(data).map_err(|error| AotError::ObjectModule {
        message: format!("cannot identify symbol-bearing artifact {}: {error}", path.display()),
    })?;
    if matches!(kind, FileKind::Archive) {
        let archive = ArchiveFile::parse(data).map_err(|error| AotError::ObjectModule {
            message: format!("cannot parse archive {}: {error}", path.display()),
        })?;
        for member in archive.members() {
            let member = member.map_err(|error| AotError::ObjectModule {
                message: format!("cannot parse member of {}: {error}", path.display()),
            })?;
            let bytes = member.data(data).map_err(|error| AotError::ObjectModule {
                message: format!("cannot read member of {}: {error}", path.display()),
            })?;
            if bytes.is_empty() || FileKind::parse(bytes).is_err() {
                continue;
            }
            extract_object_symbols(bytes, symbol_prefix, &mut defined, &mut imported, path)?;
        }
    } else {
        extract_object_symbols(data, symbol_prefix, &mut defined, &mut imported, path)?;
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
        if is_common_definition(object.format(), &symbol) {
            defined.insert(normalized);
        } else if symbol.is_undefined() {
            imported.insert(normalized);
        } else if symbol.section_index().is_some() {
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

/// Whether one symbol-table entry is a common (tentative) data definition.
///
/// A C tentative definition such as `size_t word;` may be emitted as a common symbol. The
/// linker allocates it, so for provenance it is a definition, never an import. ELF (`SHN_COMMON`)
/// and COFF (external, undefined section, non-zero value) report it through `is_common`. Mach-O
/// encodes it as `N_UNDF | N_EXT` with the size in a non-zero `n_value`, which the object reader
/// reports as undefined; a true Mach-O import always has a zero `n_value`.
fn is_common_definition<'data>(format: BinaryFormat, symbol: &impl ObjectSymbol<'data>) -> bool {
    if symbol.is_common() {
        return true;
    }
    format == BinaryFormat::MachO && symbol.is_undefined() && symbol.is_global() && symbol.address() != 0
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
    use std::path::Path;

    use object::write::{Object as WriteObject, StandardSection, Symbol, SymbolSection};
    use object::{Architecture, BinaryFormat, Endianness, SymbolFlags, SymbolKind, SymbolScope};

    use super::{extract_symbol_inventory, extract_symbol_inventory_from_bytes, normalize_target_symbol};

    const DATA_EXPORT: &str = "beskid_rt_v5_gc_barrier_active";
    const ZEROED_EXPORT: &str = "beskid_rt_v5_zeroed_word";
    const IMPORT: &str = "beskid_rt_v5_imported_word";

    fn data_symbol(name: &str) -> Symbol {
        Symbol {
            name: name.as_bytes().to_vec(),
            value: 0,
            size: 0,
            kind: SymbolKind::Data,
            scope: SymbolScope::Dynamic,
            weak: false,
            section: SymbolSection::Undefined,
            flags: SymbolFlags::None,
        }
    }

    /// One object with a common (tentative) data definition, an explicitly zero-initialized
    /// `.bss` definition and an undefined data import, written by the object crate's writer
    /// (which applies the format's default mangling, the Mach-O `_` prefix included).
    fn written_object(format: BinaryFormat, architecture: Architecture) -> Vec<u8> {
        let mut object = WriteObject::new(format, architecture, Endianness::Little);
        object.add_common_symbol(data_symbol(DATA_EXPORT), 8, 8);
        let zeroed = object.add_symbol(data_symbol(ZEROED_EXPORT));
        let bss = object.section_id(StandardSection::UninitializedData);
        object.add_symbol_bss(zeroed, bss, 8, 8);
        object.add_symbol(data_symbol(IMPORT));
        object.write().expect("write object")
    }

    /// A minimal Mach-O object whose symbol table holds a true common symbol (`N_UNDF | N_EXT`
    /// with its size in `n_value`, as Apple clang emits `size_t word;`) and an import (`N_UNDF |
    /// N_EXT` with a zero `n_value`). The object writer cannot express a Mach-O common symbol.
    fn macho_common_object() -> Vec<u8> {
        let common = format!("_{DATA_EXPORT}");
        let import = format!("_{IMPORT}");
        let mut strings = vec![0u8];
        let common_offset = strings.len() as u32;
        strings.extend_from_slice(common.as_bytes());
        strings.push(0);
        let import_offset = strings.len() as u32;
        strings.extend_from_slice(import.as_bytes());
        strings.push(0);
        while strings.len() % 8 != 0 {
            strings.push(0);
        }
        let symoff = 32u32 + 24;
        let stroff = symoff + 2 * 16;
        let mut bytes = Vec::new();
        for word in
            [object::macho::MH_MAGIC_64, object::macho::CPU_TYPE_ARM64, 0, object::macho::MH_OBJECT, 1, 24, 0, 0]
        {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        for word in [object::macho::LC_SYMTAB, 24, symoff, 2, stroff, strings.len() as u32] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        for (strx, value) in [(common_offset, 8u64), (import_offset, 0u64)] {
            bytes.extend_from_slice(&strx.to_le_bytes());
            bytes.push(object::macho::N_UNDF | object::macho::N_EXT);
            bytes.push(0);
            // GET_COMM_ALIGN: alignment 2^3 in the high byte of n_desc.
            bytes.extend_from_slice(&(if value == 0 { 0u16 } else { 3u16 << 8 }).to_le_bytes());
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&strings);
        bytes
    }

    fn assert_data_exports_are_definitions(bytes: &[u8], prefix: &str, expected_defined: &[&str], label: &str) {
        let inventory = extract_symbol_inventory_from_bytes(bytes, Path::new(label), prefix).expect("parse object");
        assert_eq!(inventory.defined, expected_defined, "{label} defined");
        assert_eq!(inventory.imported, [IMPORT], "{label} imported");
    }

    #[test]
    fn zero_initialized_data_exports_are_definitions_on_every_object_format() {
        let both = [DATA_EXPORT, ZEROED_EXPORT];
        assert_data_exports_are_definitions(&written_object(BinaryFormat::Elf, Architecture::X86_64), "", &both, "elf");
        assert_data_exports_are_definitions(
            &written_object(BinaryFormat::Coff, Architecture::X86_64),
            "",
            &both,
            "coff",
        );
        assert_data_exports_are_definitions(
            &written_object(BinaryFormat::MachO, Architecture::Aarch64),
            "_",
            &both,
            "macho",
        );
        assert_data_exports_are_definitions(&macho_common_object(), "_", &[DATA_EXPORT], "macho-common");
    }

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
