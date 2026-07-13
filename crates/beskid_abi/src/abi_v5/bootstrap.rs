use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    ABI_V5, AbiFieldLayout, AbiFunction, AbiLayout, AbiManifestV5, AbiType, AssemblyExport,
    ManifestValidationError, PlatformImport, RuntimeIntrinsic, TargetMetadata, TargetTriple,
    TrapCode,
};

pub const CANONICAL_RUNTIME_PACKAGE_PUBLISHER: &str =
    crate::generated::abi_v5_contract::ABI_V5_RUNTIME_PUBLISHER;
pub const CANONICAL_RUNTIME_PACKAGE_NAME: &str =
    crate::generated::abi_v5_contract::ABI_V5_RUNTIME_PACKAGE;
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
    pub forbidden_rust_symbols: Vec<String>,
    pub object_format: String,
    pub symbol_prefix: String,
    pub layout_hash: String,
    pub runtime_source_hash: String,
}

impl RuntimeAuditMetadata {
    pub fn for_manifest(
        manifest: &AbiManifestV5,
        runtime_source_hash: &str,
    ) -> Result<Self, ManifestValidationError> {
        manifest.validate()?;
        let mut allowed_imports = manifest
            .platform_imports
            .iter()
            .map(|entry| entry.symbol.clone())
            .chain(
                manifest
                    .assembly_exports
                    .iter()
                    .map(|entry| entry.symbol.as_str().into()),
            )
            .collect::<Vec<_>>();
        allowed_imports.sort();
        Ok(Self {
            allowed_imports,
            allowed_exports: manifest
                .exports
                .iter()
                .map(|entry| entry.symbol.clone())
                .collect(),
            forbidden_rust_symbols: forbidden_symbol_families(),
            object_format: object_format(&manifest.target).into(),
            symbol_prefix: symbol_prefix(&manifest.target).into(),
            layout_hash: manifest.layout_hash(),
            runtime_source_hash: runtime_source_hash.into(),
        })
    }

    pub fn validate(&self, manifest: &AbiManifestV5) -> Result<(), ManifestValidationError> {
        let expected = Self::for_manifest(manifest, &self.runtime_source_hash)?;
        if self == &expected {
            Ok(())
        } else {
            Err(ManifestValidationError::InvalidRuntimeAuditMetadata)
        }
    }

    pub fn audit_object_symbols<'a>(
        &self,
        symbols: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), String> {
        for raw in symbols {
            let symbol = raw.strip_prefix(&self.symbol_prefix).unwrap_or(raw);
            if self
                .forbidden_rust_symbols
                .iter()
                .any(|family| symbol.contains(family))
            {
                return Err(format!("forbidden runtime provenance symbol `{raw}`"));
            }
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceAudit {
    forbidden_symbol_families: Vec<String>,
}

fn forbidden_symbol_families() -> Vec<String> {
    serde_json::from_str::<SourceAudit>(include_str!(concat!(
        env!("OUT_DIR"),
        "/abi-v5-audit.json"
    )))
    .expect("build-validated audit source")
    .forbidden_symbol_families
}

fn object_format(target: &TargetMetadata) -> &'static str {
    match target.triple {
        TargetTriple::X86_64UnknownLinuxGnu => "elf",
        TargetTriple::Aarch64AppleDarwin => "macho",
        TargetTriple::X86_64PcWindowsMsvc => "coff",
        TargetTriple::Other(_) => "unsupported",
    }
}
fn symbol_prefix(target: &TargetMetadata) -> &'static str {
    if matches!(target.triple, TargetTriple::Aarch64AppleDarwin) {
        "_"
    } else {
        ""
    }
}

impl AbiManifestV5 {
    pub fn canonical_runtime(target: TargetMetadata) -> Self {
        let source: SourceContract =
            serde_json::from_str(crate::generated::abi_v5_contract::ABI_V5_SOURCE_JSON)
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
                .filter(|layout| {
                    layout
                        .target
                        .as_deref()
                        .is_none_or(|value| value == target_slug)
                })
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
            assembly_exports: source
                .assembly
                .iter()
                .map(|entry| source_assembly(entry, target_slug))
                .collect(),
            traps: source
                .traps
                .iter()
                .map(|trap| TrapCode::try_from(trap.code).expect("validated trap code"))
                .collect(),
            target,
        }
    }

    pub(super) fn validate_canonical_bootstrap_contract(
        &self,
    ) -> Result<(), ManifestValidationError> {
        if !self.imports.is_empty() {
            return Err(ManifestValidationError::InvalidRuntimeImportSet {
                actual: self.imports.clone(),
            });
        }
        let canonical = Self::canonical_runtime(self.target.clone());
        if self.exports != canonical.exports {
            return Err(ManifestValidationError::InvalidRuntimeExportSet {
                actual: self.exports.clone(),
            });
        }
        if self.trusted_runtime_intrinsics != canonical.trusted_runtime_intrinsics {
            return Err(ManifestValidationError::InvalidRuntimeIntrinsicSet {
                actual: self.trusted_runtime_intrinsics.clone(),
            });
        }
        if self.platform_imports != canonical.platform_imports {
            return Err(ManifestValidationError::InvalidPlatformImportSet {
                actual: self.platform_imports.clone(),
            });
        }
        if self.layouts != canonical.layouts {
            return Err(ManifestValidationError::InvalidRuntimeLayoutSet {
                actual: self.layouts.clone(),
            });
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
    layouts: Vec<SourceLayout>,
    platform_imports: Vec<SourcePlatformImport>,
    assembly: Vec<SourceAssembly>,
    traps: Vec<SourceTrap>,
    #[serde(rename = "meta")]
    _meta: serde_json::Value,
    #[serde(rename = "audit")]
    _audit: serde_json::Value,
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
    capability: String,
    params: Vec<SourceParameter>,
    result: String,
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
    locations: BTreeMap<String, Vec<String>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceTrap {
    #[serde(rename = "name")]
    _name: String,
    code: u8,
}

fn source_type(value: &str) -> AbiType {
    match value {
        "void" | "never" => AbiType::Void,
        "pointer" => AbiType::Pointer,
        "usize" => AbiType::USize,
        "isize" => AbiType::ISize,
        "i8" => AbiType::I8,
        "u8" => AbiType::U8,
        "i16" => AbiType::I16,
        "u16" => AbiType::U16,
        "i32" => AbiType::I32,
        "u32" => AbiType::U32,
        "i64" => AbiType::I64,
        "u64" => AbiType::U64,
        "v128" => AbiType::V128,
        "f32" => AbiType::F32,
        "f64" => AbiType::F64,
        _ => unreachable!("build validates ABI types"),
    }
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
        capability: entry.capability.clone(),
        param_names,
        params,
        result: source_type(&entry.result),
        noreturn: entry.result == "never",
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
            .map(|field| AbiFieldLayout {
                name: field.name.clone(),
                offset: field.offset,
                ty: source_type(&field.ty),
            })
            .collect(),
    }
}
fn source_register(value: &str) -> super::AssemblyRegister {
    use super::AssemblyRegister::*;
    match value {
        "rbx" => X86_64Rbx,
        "rbp" => X86_64Rbp,
        "rdi" => X86_64Rdi,
        "rsi" => X86_64Rsi,
        "r12" => X86_64R12,
        "r13" => X86_64R13,
        "r14" => X86_64R14,
        "r15" => X86_64R15,
        "xmm6" => X86_64Xmm6,
        "xmm7" => X86_64Xmm7,
        "xmm8" => X86_64Xmm8,
        "xmm9" => X86_64Xmm9,
        "xmm10" => X86_64Xmm10,
        "xmm11" => X86_64Xmm11,
        "xmm12" => X86_64Xmm12,
        "xmm13" => X86_64Xmm13,
        "xmm14" => X86_64Xmm14,
        "xmm15" => X86_64Xmm15,
        "x19" => Aarch64X19,
        "x20" => Aarch64X20,
        "x21" => Aarch64X21,
        "x22" => Aarch64X22,
        "x23" => Aarch64X23,
        "x24" => Aarch64X24,
        "x25" => Aarch64X25,
        "x26" => Aarch64X26,
        "x27" => Aarch64X27,
        "x28" => Aarch64X28,
        "x29" => Aarch64X29,
        "v8" => Aarch64V8,
        "v9" => Aarch64V9,
        "v10" => Aarch64V10,
        "v11" => Aarch64V11,
        "v12" => Aarch64V12,
        "v13" => Aarch64V13,
        "v14" => Aarch64V14,
        "v15" => Aarch64V15,
        _ => unreachable!("build validates preserved registers"),
    }
}
fn source_assembly(entry: &SourceAssembly, target: &str) -> AssemblyExport {
    let (param_names, params) = source_params(&entry.params);
    AssemblyExport {
        symbol: match entry.symbol.as_str() {
            "beskid_arch_v5_context_init" => super::AssemblySymbol::ContextInit,
            "beskid_arch_v5_context_switch" => super::AssemblySymbol::ContextSwitch,
            _ => unreachable!(),
        },
        param_names,
        params,
        parameter_locations: entry.locations[target].clone(),
        result: source_type(&entry.result),
        preserved_registers: entry.preserved[target]
            .iter()
            .map(|value| source_register(value))
            .collect(),
    }
}

pub fn render_runtime_c_header(
    manifest: &AbiManifestV5,
) -> Result<String, ManifestValidationError> {
    manifest.validate()?;
    Ok(include_str!(concat!(env!("OUT_DIR"), "/beskid_runtime_abi_v5.h")).into())
}

pub fn render_runtime_asm_include(
    manifest: &AbiManifestV5,
) -> Result<String, ManifestValidationError> {
    manifest.validate()?;
    Ok(include_str!(concat!(env!("OUT_DIR"), "/beskid_runtime_abi_v5.inc")).into())
}
