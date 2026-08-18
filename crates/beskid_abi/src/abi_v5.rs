//! Typed, validated contracts for the direct-call ABI v5 boundary.
//!
//! The generated ABI-v4 dispatch tables remain a compile-only bridge for the
//! current workspace. They are removed when runtime/codegen migration lands;
//! this module does not provide a fallback or dispatch path to them.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod bootstrap;

pub use bootstrap::{
    CANONICAL_RUNTIME_PACKAGE_NAME, CANONICAL_RUNTIME_PACKAGE_PUBLISHER, RuntimeAuditMetadata, RuntimePackageIdentity,
    TRAP_DIAGNOSTIC_PREFIX, TRAP_EXIT_STATUS, canonical_runtime_package, render_runtime_asm_include,
    render_runtime_c_header,
};

pub const ABI_V5: u32 = 5;
pub const RUNTIME_SYMBOL_PREFIX: &str = "beskid_rt_v5_";
pub const LIBRARY_LIFECYCLE_SYMBOLS: [&str; 2] = ["beskid_library_attach_v5", "beskid_library_detach_v5"];
macro_rules! string_contract_type {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }
    };
}

string_contract_type!(TargetTriple);
string_contract_type!(Endianness);
string_contract_type!(CallingConvention);
string_contract_type!(ObjectFormat);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetMetadata {
    pub triple: TargetTriple,
    pub endianness: Endianness,
    pub pointer_width: u8,
    pub calling_convention: CallingConvention,
    pub object_format: ObjectFormat,
    pub symbol_prefix: String,
    pub stack_alignment: u32,
    pub shadow_space: u32,
}

impl TargetMetadata {
    pub fn supported() -> Vec<Self> {
        crate::generated::abi_v5_contract::ABI_V5_TARGETS
            .iter()
            .map(|target| Self {
                triple: target.triple.into(),
                endianness: target.endianness.into(),
                pointer_width: target.pointer_width,
                calling_convention: target.calling_convention.into(),
                object_format: target.object_format.into(),
                symbol_prefix: target.symbol_prefix.into(),
                stack_alignment: target.stack_alignment,
                shadow_space: target.shadow_space,
            })
            .collect()
    }

    pub fn validate(&self) -> Result<(), TargetValidationError> {
        let expected = Self::supported()
            .into_iter()
            .find(|candidate| candidate.triple == self.triple)
            .ok_or_else(|| TargetValidationError::UnsupportedTriple(self.triple.as_str().into()))?;
        if self != &expected {
            return Err(TargetValidationError::MetadataMismatch { triple: self.triple.as_str().into() });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetValidationError {
    UnsupportedTriple(String),
    MetadataMismatch { triple: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbiType {
    Void,
    Pointer,
    USize,
    ISize,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    V128,
    F32,
    F64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiFunction {
    pub symbol: String,
    pub param_names: Vec<String>,
    pub params: Vec<AbiType>,
    pub result: AbiType,
    pub noreturn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiFieldLayout {
    pub name: String,
    pub offset: u64,
    #[serde(rename = "type")]
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiLayout {
    pub name: String,
    pub size: u64,
    pub alignment: u64,
    pub fields: Vec<AbiFieldLayout>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeTargetBinding {
    pub target: String,
    pub implementation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeIntrinsic {
    pub name: String,
    pub symbol: String,
    pub capability: String,
    pub param_names: Vec<String>,
    pub params: Vec<AbiType>,
    pub result: AbiType,
    pub noreturn: bool,
    #[serde(default, rename = "targetBindings", skip_serializing_if = "Vec::is_empty")]
    pub target_bindings: Vec<RuntimeTargetBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformImport {
    pub symbol: String,
    pub library: String,
    pub param_names: Vec<String>,
    pub params: Vec<AbiType>,
    pub result: AbiType,
    pub noreturn: bool,
}

string_contract_type!(AssemblySymbol);
string_contract_type!(AssemblyRegister);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AssemblyParameterLocation {
    Register { register: AssemblyRegister },
    Stack { base: AssemblyRegister, offset: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyExport {
    pub symbol: AssemblySymbol,
    pub param_names: Vec<String>,
    pub params: Vec<AbiType>,
    pub parameter_locations: Vec<AssemblyParameterLocation>,
    pub result: AbiType,
    pub preserved_registers: Vec<AssemblyRegister>,
}

impl AssemblyExport {
    pub fn required_for_target(target: &TargetMetadata) -> Vec<Self> {
        AbiManifestV5::canonical_runtime(target.clone()).assembly_exports
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrapCode {
    pub name: String,
    pub code: u8,
}

impl TrapCode {
    pub fn all() -> Vec<Self> {
        crate::generated::abi_v5_contract::ABI_V5_TRAPS
            .iter()
            .map(|(name, code)| Self { name: (*name).into(), code: *code })
            .collect()
    }
}

impl From<TrapCode> for u8 {
    fn from(value: TrapCode) -> Self {
        value.code
    }
}

impl TryFrom<u8> for TrapCode {
    type Error = InvalidTrapCode;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::all().into_iter().find(|code| code.code == value).ok_or(InvalidTrapCode(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidTrapCode(pub u8);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbiManifestV5 {
    pub abi_version: u32,
    pub trap_exit_status: u8,
    pub trap_diagnostic: String,
    pub target: TargetMetadata,
    pub imports: Vec<AbiFunction>,
    pub exports: Vec<AbiFunction>,
    pub layouts: Vec<AbiLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted_runtime_package: Option<RuntimePackageIdentity>,
    pub trusted_runtime_intrinsics: Vec<RuntimeIntrinsic>,
    pub platform_imports: Vec<PlatformImport>,
    pub assembly_exports: Vec<AssemblyExport>,
    pub traps: Vec<TrapCode>,
}

impl AbiManifestV5 {
    pub fn validate(&self) -> Result<(), ManifestValidationError> {
        if self.abi_version != ABI_V5 {
            return Err(ManifestValidationError::WrongAbiVersion(self.abi_version));
        }
        if self.trap_exit_status != TRAP_EXIT_STATUS || self.trap_diagnostic != TRAP_DIAGNOSTIC_PREFIX {
            return Err(ManifestValidationError::InvalidTrapContract);
        }
        self.target.validate().map_err(ManifestValidationError::InvalidTarget)?;

        let mut symbols = HashSet::new();
        for function in self.imports.iter().chain(&self.exports) {
            if !function.symbol.starts_with(RUNTIME_SYMBOL_PREFIX)
                && !LIBRARY_LIFECYCLE_SYMBOLS.contains(&function.symbol.as_str())
            {
                return Err(ManifestValidationError::UnversionedRuntimeSymbol { symbol: function.symbol.clone() });
            }
            if !symbols.insert(function.symbol.clone()) {
                return Err(ManifestValidationError::DuplicateSymbol { symbol: function.symbol.clone() });
            }
        }

        validate_named_contracts(self.trusted_runtime_intrinsics.iter().map(|entry| entry.name.as_str()))?;
        let mut intrinsic_symbols = HashSet::new();
        let assembly_symbols = self.assembly_exports.iter().map(|entry| entry.symbol.as_str()).collect::<HashSet<_>>();
        for intrinsic in &self.trusted_runtime_intrinsics {
            // The canonical scheduler may call only manifest-owned assembly context exports.
            // All other runtime intrinsic names remain version-prefixed, preventing a source
            // declaration from turning into an arbitrary ABI import.
            if !intrinsic.symbol.starts_with(RUNTIME_SYMBOL_PREFIX)
                && !assembly_symbols.contains(intrinsic.symbol.as_str())
            {
                return Err(ManifestValidationError::UnversionedRuntimeSymbol { symbol: intrinsic.symbol.clone() });
            }
            if !intrinsic_symbols.insert(intrinsic.symbol.as_str()) {
                return Err(ManifestValidationError::DuplicateSymbol { symbol: intrinsic.symbol.clone() });
            }
        }
        validate_named_contracts(self.platform_imports.iter().map(|entry| entry.symbol.as_str()))?;
        validate_layouts(&self.layouts)?;

        if let Some(package) = &self.trusted_runtime_package {
            if package != &canonical_runtime_package() {
                return Err(ManifestValidationError::UnauthorizedRuntimePackage { actual: package.clone() });
            }
            self.validate_canonical_bootstrap_contract()?;
            let trap = self.exports.iter().find(|function| function.symbol == "beskid_rt_v5_trap");
            if !matches!(trap, Some(function) if function.noreturn && function.result == AbiType::Void) {
                return Err(ManifestValidationError::InvalidTrapContract);
            }
        }

        if !assembly_exports_are_valid(&self.target, &self.assembly_exports) {
            return Err(ManifestValidationError::InvalidAssemblyExports { actual: self.assembly_exports.clone() });
        }

        let expected = TrapCode::all();
        let actual_traps: HashSet<_> = self.traps.iter().cloned().collect();
        let expected_traps: HashSet<_> = expected.iter().cloned().collect();
        if self.traps.len() != expected.len() || actual_traps != expected_traps {
            return Err(ManifestValidationError::InvalidTrapSet {
                actual: self.traps.iter().cloned().map(u8::from).collect(),
            });
        }
        Ok(())
    }

    pub fn layout_hash(&self) -> String {
        canonical_layout_hash(&self.layouts)
    }

    /// Typed manifest metadata only. Intrinsic legality is decided by the
    /// DB-owned `beskid_queries::runtime_intrinsic` provenance query.
    pub fn intrinsic_metadata(&self, name: &str) -> Option<&RuntimeIntrinsic> {
        self.trusted_runtime_intrinsics.iter().find(|intrinsic| intrinsic.name == name)
    }
}

fn assembly_exports_are_valid(target: &TargetMetadata, exports: &[AssemblyExport]) -> bool {
    let mut actual = exports.to_vec();
    actual.sort_unstable_by(|left, right| left.symbol.cmp(&right.symbol));
    let mut expected = AbiManifestV5::canonical_runtime(target.clone()).assembly_exports;
    expected.sort_unstable_by(|left, right| left.symbol.cmp(&right.symbol));
    actual == expected
}

fn validate_named_contracts<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<(), ManifestValidationError> {
    let mut seen = HashSet::new();
    for name in names {
        if name.is_empty() || !seen.insert(name) {
            return Err(ManifestValidationError::DuplicateSymbol { symbol: name.into() });
        }
    }
    Ok(())
}

fn validate_layouts(layouts: &[AbiLayout]) -> Result<(), ManifestValidationError> {
    let mut names = HashSet::new();
    for layout in layouts {
        if layout.name.is_empty() || !names.insert(layout.name.as_str()) {
            return Err(ManifestValidationError::DuplicateLayout { name: layout.name.clone() });
        }
        if layout.size == 0
            || layout.alignment == 0
            || !layout.alignment.is_power_of_two()
            || layout.fields.iter().any(|field| field.offset >= layout.size)
        {
            return Err(ManifestValidationError::InvalidLayout { name: layout.name.clone() });
        }
        let mut fields = HashSet::new();
        if layout.fields.iter().any(|field| field.name.is_empty() || !fields.insert(field.name.as_str())) {
            return Err(ManifestValidationError::InvalidLayout { name: layout.name.clone() });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestValidationError {
    WrongAbiVersion(u32),
    InvalidTarget(TargetValidationError),
    UnversionedRuntimeSymbol { symbol: String },
    DuplicateSymbol { symbol: String },
    DuplicateLayout { name: String },
    InvalidLayout { name: String },
    InvalidAssemblyExports { actual: Vec<AssemblyExport> },
    InvalidTrapSet { actual: Vec<u8> },
    InvalidTrapContract,
    DuplicateSourcePath { logical_path: String },
    UnauthorizedRuntimePackage { actual: RuntimePackageIdentity },
    InvalidRuntimeImportSet { actual: Vec<AbiFunction> },
    InvalidRuntimeExportSet { actual: Vec<AbiFunction> },
    InvalidRuntimeIntrinsicSet { actual: Vec<RuntimeIntrinsic> },
    InvalidPlatformImportSet { actual: Vec<PlatformImport> },
    InvalidRuntimeLayoutSet { actual: Vec<AbiLayout> },
    InvalidRuntimeAuditMetadata,
}

impl std::fmt::Display for ManifestValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUnit {
    pub logical_path: String,
    pub source: String,
}

pub fn canonical_layout_hash(layouts: &[AbiLayout]) -> String {
    let mut canonical = layouts.to_vec();
    canonical.sort_by(|left, right| left.name.cmp(&right.name));
    for layout in &mut canonical {
        layout.fields.sort_by(|left, right| left.offset.cmp(&right.offset).then_with(|| left.name.cmp(&right.name)));
    }
    let mut hasher = Sha256::new();
    for layout in canonical {
        hash_str(&mut hasher, &layout.name);
        hash_u64(&mut hasher, layout.size);
        hash_u64(&mut hasher, layout.alignment);
        hash_u64(&mut hasher, layout.fields.len() as u64);
        for field in layout.fields {
            hash_str(&mut hasher, &field.name);
            hash_u64(&mut hasher, field.offset);
            hash_str(&mut hasher, &field.ty);
        }
    }
    hex_digest(hasher.finalize())
}

pub fn canonical_source_hash(units: &[SourceUnit]) -> Result<String, ManifestValidationError> {
    let mut canonical = units.to_vec();
    canonical.sort_by(|left, right| left.logical_path.cmp(&right.logical_path));
    for pair in canonical.windows(2) {
        if pair[0].logical_path == pair[1].logical_path {
            return Err(ManifestValidationError::DuplicateSourcePath { logical_path: pair[0].logical_path.clone() });
        }
    }
    let mut hasher = Sha256::new();
    for unit in canonical {
        hash_str(&mut hasher, &unit.logical_path);
        hash_str(&mut hasher, &unit.source);
    }
    Ok(hex_digest(hasher.finalize()))
}

fn hash_str(hasher: &mut Sha256, value: &str) {
    hash_u64(hasher, value.len() as u64);
    hasher.update(value.as_bytes());
}

fn hash_u64(hasher: &mut Sha256, value: u64) {
    hasher.update(value.to_le_bytes());
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    let mut output = String::with_capacity(bytes.as_ref().len() * 2);
    for byte in bytes.as_ref() {
        use fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}
