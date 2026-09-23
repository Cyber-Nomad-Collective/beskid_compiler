use std::collections::BTreeSet;

use rustc_demangle::try_demangle;
use serde::{Deserialize, Serialize};

use super::super::{AbiManifestV5, ManifestValidationError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeAuditMetadata {
    pub allowed_imports: Vec<String>,
    pub allowed_exports: Vec<String>,
    pub loader_required_exports: Vec<String>,
    pub forbidden_rust_symbols: Vec<String>,
    pub object_format: String,
    pub symbol_prefix: String,
    pub layout_hash: String,
    pub runtime_source_hash: String,
}

impl RuntimeAuditMetadata {
    pub fn for_manifest(manifest: &AbiManifestV5, runtime_source_hash: &str) -> Result<Self, ManifestValidationError> {
        manifest.validate()?;
        // Keep the allowlist in the same canonical spelling used by `normalized_symbol` below.
        // The manifest records target-native linker spellings (`_exit` on Darwin); provenance
        // adapters may already have removed the Mach-O prefix, so retaining that prefix here
        // would reject a legitimate platform import.
        let mut allowed_imports = manifest
            .platform_imports
            .iter()
            .map(|entry| {
                normalize_object_symbol(
                    &entry.symbol,
                    manifest.target.object_format.as_str(),
                    &manifest.target.symbol_prefix,
                )
            })
            .collect::<Vec<_>>();
        // Darwin-only transitive libc dependencies. Normalization strips the Mach-O leading
        // underscore, leaving these exact spellings. `tlv_bootstrap` is the C11 TLS helper;
        // `error` (from `<error.h>`) is pulled in by C library dependencies of the runtime object.
        if manifest.target.object_format.as_str() == "macho" {
            allowed_imports.push("tlv_bootstrap".into());
            allowed_imports.push("error".into());
        }
        // Per-target OS imports declared by runtime intrinsic bindings (e.g. `unlink`, `mmap`,
        // `open`) are linked into the runtime object but are not platform imports of the manifest.
        // Only the bindings for the current target apply; a Linux runtime must not be allowed to
        // import Windows or Darwin OS symbols.
        let target_triple = manifest.target.triple.as_str();
        allowed_imports.extend(
            manifest
                .trusted_runtime_intrinsics
                .iter()
                .flat_map(|intrinsic| intrinsic.target_bindings.iter())
                .filter(|binding| binding.target == target_triple)
                .flat_map(|binding| binding.os_imports.iter().cloned()),
        );
        // Linux libc transitive dependency: `__errno_location` is linked implicitly by the C
        // compiler when calling syscall wrappers that set errno (`unlink`, `open`, `stat`, ...).
        if manifest.target.object_format.as_str() == "elf" {
            allowed_imports.push("__errno_location".into());
        }
        allowed_imports.sort();
        allowed_imports.dedup();
        let mut loader_required_exports = manifest
            .exports
            .iter()
            .map(|entry| entry.symbol.clone())
            .chain(manifest.assembly_exports.iter().map(|entry| entry.symbol.as_str().into()))
            .chain(
                crate::generated::abi_v5_contract::ABI_V5_CORELIB_SERVICE_BINDINGS
                    .iter()
                    .filter(|binding| binding.target == target_triple)
                    .map(|binding| binding.implementation.into()),
            )
            .collect::<Vec<_>>();
        loader_required_exports.sort();
        loader_required_exports.dedup();
        let mut allowed_exports = loader_required_exports.clone();
        allowed_exports.extend(manifest.trusted_runtime_intrinsics.iter().map(|intrinsic| intrinsic.symbol.clone()));
        allowed_exports.extend(
            manifest
                .trusted_runtime_intrinsics
                .iter()
                .flat_map(|intrinsic| intrinsic.target_bindings.iter())
                .map(|binding| binding.implementation.clone()),
        );
        allowed_exports.sort();
        allowed_exports.dedup();
        Ok(Self {
            allowed_imports,
            allowed_exports,
            loader_required_exports,
            forbidden_rust_symbols: forbidden_symbol_families(),
            object_format: manifest.target.object_format.as_str().into(),
            symbol_prefix: manifest.target.symbol_prefix.clone(),
            layout_hash: manifest.layout_hash(),
            runtime_source_hash: runtime_source_hash.into(),
        })
    }

    pub fn validate(&self, manifest: &AbiManifestV5) -> Result<(), ManifestValidationError> {
        let expected = Self::for_manifest(manifest, &self.runtime_source_hash)?;
        if self == &expected { Ok(()) } else { Err(ManifestValidationError::InvalidRuntimeAuditMetadata) }
    }

    pub fn audit_object_symbol_tables<'a>(
        &self,
        defined: impl IntoIterator<Item = &'a str>,
        undefined: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), String> {
        let defined = self.normalized_symbol_set("defined", defined)?;
        let undefined = self.normalized_symbol_set("undefined", undefined)?;
        exact_symbol_set("defined", &self.allowed_exports, &defined)?;
        exact_symbol_set("undefined", &self.allowed_imports, &undefined)?;
        Ok(())
    }

    /// Audit a linked runtime object whose manifest exports must be present while optional
    /// platform intrinsics and imports may be dead-stripped by the native linker.
    pub fn audit_linked_runtime_symbol_tables<'a>(
        &self,
        required_exports: &[String],
        defined: impl IntoIterator<Item = &'a str>,
        undefined: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), String> {
        let mut defined = self.collapsed_symbol_set(defined)?;
        let mut undefined = self.collapsed_symbol_set(undefined)?;
        // LLVM's COFF symbol adapter reports MSVC string-literal COMDATs and the absolute
        // feature marker as external definitions. They are implementation metadata, not PE
        // exports. Forbidden provenance has already been checked while collapsing the set;
        // discard only these two exact compiler-owned families before enforcing the ABI export
        // allowlist. Undefined imports are never filtered here.
        if self.object_format == "coff" {
            defined.retain(|symbol| !is_coff_compiler_metadata_definition(symbol));
        }
        // `nm -u` reports references from every member of a static archive. A reference that is
        // also defined by another member is internally resolved when the archive is linked and
        // must not be treated as an external runtime dependency.
        undefined.retain(|symbol| !defined.contains(symbol));
        required_symbol_set("defined", required_exports, &defined)?;
        allowlisted_symbol_set("defined", &self.allowed_exports, &defined)?;
        allowlisted_symbol_set("undefined", &self.allowed_imports, &undefined)
    }

    fn normalized_symbol_set<'a>(
        &self,
        table: &str,
        symbols: impl IntoIterator<Item = &'a str>,
    ) -> Result<BTreeSet<String>, String> {
        let mut normalized = BTreeSet::new();
        for raw in symbols {
            let symbol = self.normalized_symbol(raw)?;
            if !normalized.insert(symbol.clone()) {
                return Err(format!("duplicate {table} symbol `{symbol}`"));
            }
        }
        Ok(normalized)
    }

    /// Normalize a linked-artifact symbol table in which one symbol legitimately repeats.
    ///
    /// `nm` walks every member of a static archive, so an import is reported once per referencing
    /// member: the Linux platform objects import `mmap` for both the page-allocation intrinsics
    /// and guarded scheduler stacks. Collapsing those repeats keeps the provenance and allowlist
    /// checks exact while refusing to treat multi-member references as malformed input.
    fn collapsed_symbol_set<'a>(&self, symbols: impl IntoIterator<Item = &'a str>) -> Result<BTreeSet<String>, String> {
        symbols.into_iter().map(|raw| self.normalized_symbol(raw)).collect()
    }

    fn normalized_symbol(&self, raw: &str) -> Result<String, String> {
        reject_forbidden_provenance(raw, &self.forbidden_rust_symbols)?;
        // Object readers may already have removed Mach-O decoration. Preserve an
        // exact manifest-owned import identity before stripping another underscore;
        // undeclared names still take the ordinary one-prefix, fail-closed path.
        if self.object_format == "macho" && self.allowed_imports.iter().any(|symbol| symbol == raw) {
            return Ok(raw.into());
        }
        let normalized = normalize_object_symbol(raw, &self.object_format, &self.symbol_prefix);
        // Darwin's `_exit` platform import, C11 TLS `_tlv_bootstrap` helper, and libc `__error`
        // errno accessor all have a leading underscore in their native C names before Mach-O
        // decoration. A raw archive therefore reports `__exit`/`__tlv_bootstrap`/`___error`, while
        // the matrix adapter has already removed one decoration (`_exit`/`_tlv_bootstrap`/`__error`).
        // After the single-prefix `normalize_object_symbol` pass, these become `_exit`,
        // `_tlv_bootstrap`, and `__error` (raw) or `exit`, `tlv_bootstrap`, and `_error` (matrix).
        // Canonicalize only these declared Darwin imports here; generic object-symbol normalization
        // deliberately remains one-prefix-only and fail-closed.
        if self.object_format == "macho" {
            match normalized.as_str() {
                "_exit" | "_tlv_bootstrap" | "_error" => return Ok(normalized[1..].into()),
                "__error" => return Ok(normalized[2..].into()),
                _ => {}
            }
        }
        Ok(normalized)
    }
}

fn is_coff_compiler_metadata_definition(symbol: &str) -> bool {
    symbol.starts_with("??_C@") || symbol == "@feat.00"
}

fn exact_symbol_set(table: &str, expected: &[String], actual: &BTreeSet<String>) -> Result<(), String> {
    let expected = expected.iter().cloned().collect::<BTreeSet<_>>();
    if expected == *actual {
        return Ok(());
    }
    let missing = expected.difference(actual).cloned().collect::<Vec<_>>();
    let unexpected = actual.difference(&expected).cloned().collect::<Vec<_>>();
    Err(format!("{table} symbol table mismatch: missing={missing:?}, unexpected={unexpected:?}"))
}

fn required_symbol_set(table: &str, required: &[String], actual: &BTreeSet<String>) -> Result<(), String> {
    let required = required.iter().cloned().collect::<BTreeSet<_>>();
    let missing = required.difference(actual).cloned().collect::<Vec<_>>();
    if missing.is_empty() { Ok(()) } else { Err(format!("{table} symbol table is missing={missing:?}")) }
}

fn allowlisted_symbol_set(table: &str, allowed: &[String], actual: &BTreeSet<String>) -> Result<(), String> {
    let allowed = allowed.iter().cloned().collect::<BTreeSet<_>>();
    let unexpected = actual.difference(&allowed).cloned().collect::<Vec<_>>();
    if unexpected.is_empty() { Ok(()) } else { Err(format!("{table} symbol table has unexpected={unexpected:?}")) }
}

pub(super) fn normalize_object_symbol(raw: &str, object_format: &str, symbol_prefix: &str) -> String {
    let mut symbol = raw;
    if object_format == "coff" {
        symbol = symbol.strip_prefix("__imp_").unwrap_or(symbol);
    }
    symbol = symbol.strip_prefix(symbol_prefix).unwrap_or(symbol);
    if object_format == "elf" {
        symbol = symbol.split_once('@').map_or(symbol, |(base, _)| base);
    }
    symbol.into()
}

fn reject_forbidden_provenance(raw: &str, forbidden: &[String]) -> Result<(), String> {
    let unprefixed = raw.strip_prefix('_').unwrap_or(raw);
    let demangled = try_demangle(raw).or_else(|_| try_demangle(unprefixed)).ok().map(|symbol| symbol.to_string());
    let forbidden_family = forbidden
        .iter()
        .any(|family| raw.contains(family) || demangled.as_deref().is_some_and(|symbol| symbol.contains(family)));
    if demangled.is_some() || forbidden_family {
        Err(format!("forbidden runtime provenance symbol `{raw}`"))
    } else {
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceAudit {
    forbidden_symbol_families: Vec<String>,
    #[serde(rename = "corelibServices")]
    _corelib_services: serde_json::Value,
    #[serde(rename = "entryAdapters")]
    _entry_adapters: serde_json::Value,
}

pub(super) fn forbidden_symbol_families() -> Vec<String> {
    serde_json::from_str::<SourceAudit>(include_str!(concat!(env!("OUT_DIR"), "/abi-v5-audit.json")))
        .expect("build-validated audit source")
        .forbidden_symbol_families
}
