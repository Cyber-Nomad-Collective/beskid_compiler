use std::collections::{BTreeMap, BTreeSet};

use rustc_demangle::try_demangle;
use serde::{Deserialize, Serialize};

use super::{
    ABI_V5, AbiFieldLayout, AbiFunction, AbiLayout, AbiManifestV5, AbiType, AssemblyExport, AssemblyParameterLocation,
    AssemblyRegister, AssemblySymbol, ManifestValidationError, PlatformImport, RuntimeIntrinsic, RuntimeTargetBinding,
    TargetMetadata, TrapCode,
};

pub const CANONICAL_RUNTIME_PACKAGE_PUBLISHER: &str = crate::generated::abi_v5_contract::ABI_V5_RUNTIME_PUBLISHER;
pub const CANONICAL_RUNTIME_PACKAGE_NAME: &str = crate::generated::abi_v5_contract::ABI_V5_RUNTIME_PACKAGE;
pub const TRAP_EXIT_STATUS: u8 = crate::generated::abi_v5_contract::ABI_V5_TRAP_EXIT_STATUS as u8;
pub const TRAP_DIAGNOSTIC_PREFIX: &str = crate::generated::abi_v5_contract::ABI_V5_TRAP_DIAGNOSTIC;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimePackageIdentity {
    publisher: String,
    name: String,
    abi_version: u32,
}

impl RuntimePackageIdentity {
    pub fn publisher(&self) -> &str {
        &self.publisher
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn abi_version(&self) -> u32 {
        self.abi_version
    }
}

pub fn canonical_runtime_package() -> RuntimePackageIdentity {
    RuntimePackageIdentity {
        publisher: CANONICAL_RUNTIME_PACKAGE_PUBLISHER.into(),
        name: CANONICAL_RUNTIME_PACKAGE_NAME.into(),
        abi_version: ABI_V5,
    }
}

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
        // Darwin C11 thread-local storage lowers through the platform TLV bootstrap helper.
        // Normalization strips the Mach-O leading underscore, leaving this exact spelling.
        if manifest.target.object_format.as_str() == "macho" {
            allowed_imports.push("tlv_bootstrap".into());
        }
        allowed_imports.sort();
        allowed_imports.dedup();
        let mut loader_required_exports = manifest
            .exports
            .iter()
            .map(|entry| entry.symbol.clone())
            .chain(manifest.assembly_exports.iter().map(|entry| entry.symbol.as_str().into()))
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
        let normalized = normalize_object_symbol(raw, &self.object_format, &self.symbol_prefix);
        // Darwin's `_exit` platform import and C11 TLS `_tlv_bootstrap` helper both have a
        // leading underscore in their native names before Mach-O decoration. A raw archive
        // therefore reports `__exit`/`__tlv_bootstrap`, while the matrix adapter has already
        // removed one decoration. Canonicalize only these declared Darwin imports here; generic
        // object-symbol normalization deliberately remains one-prefix-only and fail-closed.
        if self.object_format == "macho" && matches!(normalized.as_str(), "_exit" | "_tlv_bootstrap") {
            Ok(normalized[1..].into())
        } else {
            Ok(normalized)
        }
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

fn normalize_object_symbol(raw: &str, object_format: &str, symbol_prefix: &str) -> String {
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
    let rust_mangled = unprefixed.starts_with('R') || (unprefixed.starts_with("ZN") && unprefixed.ends_with('E'));
    let demangled = try_demangle(raw).or_else(|_| try_demangle(unprefixed)).ok().map(|symbol| symbol.to_string());
    let forbidden_family = forbidden
        .iter()
        .any(|family| raw.contains(family) || demangled.as_deref().is_some_and(|symbol| symbol.contains(family)));
    if rust_mangled || demangled.is_some() || forbidden_family {
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

fn forbidden_symbol_families() -> Vec<String> {
    serde_json::from_str::<SourceAudit>(include_str!(concat!(env!("OUT_DIR"), "/abi-v5-audit.json")))
        .expect("build-validated audit source")
        .forbidden_symbol_families
}

impl AbiManifestV5 {
    pub fn canonical_runtime(target: TargetMetadata) -> Self {
        let source: SourceContract = serde_json::from_str(crate::generated::abi_v5_contract::ABI_V5_SOURCE_JSON)
            .expect("build-validated ABI-v5 generated source");
        let _target_source = source
            .targets
            .iter()
            .find(|entry| entry.triple == target.triple.as_str())
            .expect("target validation and generated source agree");
        let target_slug = target.triple.as_str();
        Self {
            abi_version: ABI_V5,
            trap_exit_status: TRAP_EXIT_STATUS,
            trap_diagnostic: TRAP_DIAGNOSTIC_PREFIX.into(),
            imports: Vec::new(),
            exports: source.exports.iter().map(source_function).collect(),
            layouts: source
                .layouts
                .iter()
                .filter(|layout| layout.target.as_deref().is_none_or(|value| value == target_slug))
                .map(source_layout)
                .collect(),
            trusted_runtime_package: Some(canonical_runtime_package()),
            trusted_runtime_intrinsics: source.intrinsics.iter().map(source_intrinsic).collect(),
            platform_imports: source
                .platform_imports
                .iter()
                .filter(|entry| entry.target == target_slug)
                .map(source_platform_import)
                .collect(),
            assembly_exports: source.assembly.iter().map(|entry| source_assembly(entry, target_slug)).collect(),
            traps: crate::generated::abi_v5_contract::ABI_V5_TRAPS
                .iter()
                .map(|(_, code)| TrapCode::try_from(*code).expect("validated trap code"))
                .collect(),
            target,
        }
    }

    pub(super) fn validate_canonical_bootstrap_contract(&self) -> Result<(), ManifestValidationError> {
        if !self.imports.is_empty() {
            return Err(ManifestValidationError::InvalidRuntimeImportSet { actual: self.imports.clone() });
        }
        let canonical = Self::canonical_runtime(self.target.clone());
        if self.exports != canonical.exports {
            return Err(ManifestValidationError::InvalidRuntimeExportSet { actual: self.exports.clone() });
        }
        if self.trusted_runtime_intrinsics != canonical.trusted_runtime_intrinsics {
            return Err(ManifestValidationError::InvalidRuntimeIntrinsicSet {
                actual: self.trusted_runtime_intrinsics.clone(),
            });
        }
        if self.platform_imports != canonical.platform_imports {
            return Err(ManifestValidationError::InvalidPlatformImportSet { actual: self.platform_imports.clone() });
        }
        if self.layouts != canonical.layouts {
            return Err(ManifestValidationError::InvalidRuntimeLayoutSet { actual: self.layouts.clone() });
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceContract {
    targets: Vec<SourceTarget>,
    exports: Vec<SourceFunction>,
    intrinsics: Vec<SourceIntrinsic>,
    #[serde(rename = "softBuiltins")]
    _soft_builtins: Vec<SourceSoftBuiltin>,
    layouts: Vec<SourceLayout>,
    platform_imports: Vec<SourcePlatformImport>,
    assembly: Vec<SourceAssembly>,
    #[serde(rename = "corelibServices")]
    _corelib_services: serde_json::Value,
    #[serde(rename = "entryAdapters")]
    _entry_adapters: serde_json::Value,
    #[serde(rename = "traps")]
    _traps: serde_json::Value,
    #[serde(rename = "meta")]
    _meta: serde_json::Value,
    #[serde(rename = "audit")]
    _audit: serde_json::Value,
    #[serde(default)]
    _statuses: Vec<SourceStatus>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceTarget {
    triple: String,
    #[serde(rename = "endianness")]
    _endianness: String,
    #[serde(rename = "pointerWidth")]
    _pointer_width: u8,
    #[serde(rename = "callingConvention")]
    _calling_convention: String,
    #[serde(rename = "objectFormat")]
    _object_format: String,
    #[serde(rename = "symbolPrefix")]
    _symbol_prefix: String,
    #[serde(rename = "stackAlignment")]
    _stack_alignment: u32,
    #[serde(rename = "shadowSpace")]
    _shadow_space: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceParameter {
    name: String,
    #[serde(rename = "type")]
    ty: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceFunction {
    symbol: String,
    params: Vec<SourceParameter>,
    result: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceIntrinsic {
    name: String,
    symbol: String,
    capability: String,
    params: Vec<SourceParameter>,
    result: String,
    #[serde(default, rename = "resultStatus")]
    _result_status: Option<String>,
    #[serde(default, rename = "targetBindings")]
    target_bindings: Vec<SourceTargetBinding>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceTargetBinding {
    #[serde(rename = "target")]
    target: String,
    #[serde(rename = "implementation")]
    implementation: String,
    #[serde(rename = "osImports")]
    _os_imports: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceSoftBuiltin {
    #[serde(rename = "name")]
    _name: String,
    #[serde(rename = "symbol")]
    _symbol: String,
    #[serde(rename = "params")]
    _params: Vec<SourceParameter>,
    #[serde(rename = "result")]
    _result: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceStatus {
    #[serde(rename = "name")]
    _name: String,
    #[serde(rename = "repr")]
    _repr: String,
    #[serde(rename = "values")]
    _values: Vec<SourceStatusValue>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceStatusValue {
    #[serde(rename = "name")]
    _name: String,
    #[serde(rename = "value")]
    _value: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceField {
    name: String,
    offset: u64,
    #[serde(rename = "type")]
    ty: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceLayout {
    name: String,
    target: Option<String>,
    size: u64,
    alignment: u64,
    fields: Vec<SourceField>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcePlatformImport {
    symbol: String,
    target: String,
    library: String,
    params: Vec<SourceParameter>,
    result: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceAssembly {
    symbol: String,
    params: Vec<SourceParameter>,
    result: String,
    preserved: BTreeMap<String, Vec<String>>,
    locations: BTreeMap<String, Vec<SourceParameterLocation>>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SourceParameterLocation {
    Register { register: String },
    Stack { base: String, offset: u64 },
}
fn source_type(value: &str) -> AbiType {
    crate::generated::abi_v5_contract::ABI_V5_TYPES
        .iter()
        .find_map(|(name, ty)| (*name == value).then_some(*ty))
        .expect("build validates ABI types")
}
fn source_params(params: &[SourceParameter]) -> (Vec<String>, Vec<AbiType>) {
    (
        params.iter().map(|entry| entry.name.clone()).collect(),
        params.iter().map(|entry| source_type(&entry.ty)).collect(),
    )
}
fn source_function(entry: &SourceFunction) -> AbiFunction {
    let (param_names, params) = source_params(&entry.params);
    AbiFunction {
        symbol: entry.symbol.clone(),
        param_names,
        params,
        result: source_type(&entry.result),
        noreturn: entry.result == "never",
    }
}
fn source_intrinsic(entry: &SourceIntrinsic) -> RuntimeIntrinsic {
    let (param_names, params) = source_params(&entry.params);
    RuntimeIntrinsic {
        name: entry.name.clone(),
        symbol: entry.symbol.clone(),
        capability: entry.capability.clone(),
        param_names,
        params,
        result: source_type(&entry.result),
        noreturn: entry.result == "never",
        target_bindings: entry
            .target_bindings
            .iter()
            .map(|binding| RuntimeTargetBinding {
                target: binding.target.clone(),
                implementation: binding.implementation.clone(),
            })
            .collect(),
    }
}
fn source_platform_import(entry: &SourcePlatformImport) -> PlatformImport {
    let (param_names, params) = source_params(&entry.params);
    PlatformImport {
        symbol: entry.symbol.clone(),
        library: entry.library.clone(),
        param_names,
        params,
        result: source_type(&entry.result),
        noreturn: entry.result == "never",
    }
}
fn source_layout(entry: &SourceLayout) -> AbiLayout {
    AbiLayout {
        name: entry.name.clone(),
        size: entry.size,
        alignment: entry.alignment,
        fields: entry
            .fields
            .iter()
            .map(|field| AbiFieldLayout { name: field.name.clone(), offset: field.offset, ty: field.ty.clone() })
            .collect(),
    }
}
fn source_register(value: &str) -> super::AssemblyRegister {
    AssemblyRegister::new(value)
}
fn source_assembly(entry: &SourceAssembly, target: &str) -> AssemblyExport {
    let (param_names, params) = source_params(&entry.params);
    AssemblyExport {
        symbol: AssemblySymbol::new(&entry.symbol),
        param_names,
        params,
        parameter_locations: entry.locations[target]
            .iter()
            .map(|location| match location {
                SourceParameterLocation::Register { register } => {
                    AssemblyParameterLocation::Register { register: AssemblyRegister::new(register) }
                }
                SourceParameterLocation::Stack { base, offset } => {
                    AssemblyParameterLocation::Stack { base: AssemblyRegister::new(base), offset: *offset }
                }
            })
            .collect(),
        result: source_type(&entry.result),
        preserved_registers: entry.preserved[target].iter().map(|value| source_register(value)).collect(),
    }
}

pub fn render_runtime_c_header(manifest: &AbiManifestV5) -> Result<String, ManifestValidationError> {
    manifest.validate()?;
    Ok(include_str!(concat!(env!("OUT_DIR"), "/beskid_runtime_abi_v5.h")).into())
}

pub fn render_runtime_asm_include(manifest: &AbiManifestV5) -> Result<String, ManifestValidationError> {
    manifest.validate()?;
    crate::generated::abi_v5_contract::ABI_V5_ASM_INCLUDES
        .iter()
        .find_map(|(target, source)| (*target == manifest.target.triple.as_str()).then(|| (*source).into()))
        .ok_or(ManifestValidationError::InvalidRuntimeAuditMetadata)
}
