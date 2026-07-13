//! Typed, validated contracts for the direct-call ABI v5 boundary.
//!
//! The generated ABI-v4 dispatch tables remain a compile-only bridge for the
//! current workspace. They are removed when runtime/codegen migration lands;
//! this module does not provide a fallback or dispatch path to them.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const ABI_V5: u32 = 5;
pub const RUNTIME_SYMBOL_PREFIX: &str = "beskid_rt_v5_";
pub const APPROVED_ASSEMBLY_SYMBOLS: [&str; 2] = [
    "beskid_arch_v5_context_init",
    "beskid_arch_v5_context_switch",
];

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetTriple {
    X86_64UnknownLinuxGnu,
    Aarch64AppleDarwin,
    X86_64PcWindowsMsvc,
    Other(String),
}

impl TargetTriple {
    pub fn as_str(&self) -> &str {
        match self {
            Self::X86_64UnknownLinuxGnu => "x86_64-unknown-linux-gnu",
            Self::Aarch64AppleDarwin => "aarch64-apple-darwin",
            Self::X86_64PcWindowsMsvc => "x86_64-pc-windows-msvc",
            Self::Other(value) => value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Endianness {
    Little,
    Big,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallingConvention {
    SystemV,
    AppleAarch64,
    WindowsX64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetMetadata {
    pub triple: TargetTriple,
    pub endianness: Endianness,
    pub pointer_width: u8,
    pub calling_convention: CallingConvention,
}

impl TargetMetadata {
    pub const SUPPORTED: [Self; 3] = [
        Self {
            triple: TargetTriple::X86_64UnknownLinuxGnu,
            endianness: Endianness::Little,
            pointer_width: 64,
            calling_convention: CallingConvention::SystemV,
        },
        Self {
            triple: TargetTriple::Aarch64AppleDarwin,
            endianness: Endianness::Little,
            pointer_width: 64,
            calling_convention: CallingConvention::AppleAarch64,
        },
        Self {
            triple: TargetTriple::X86_64PcWindowsMsvc,
            endianness: Endianness::Little,
            pointer_width: 64,
            calling_convention: CallingConvention::WindowsX64,
        },
    ];

    pub fn validate(&self) -> Result<(), TargetValidationError> {
        let expected = Self::SUPPORTED
            .into_iter()
            .find(|candidate| candidate.triple == self.triple)
            .ok_or_else(|| TargetValidationError::UnsupportedTriple(self.triple.as_str().into()))?;
        if self.endianness != Endianness::Little {
            return Err(TargetValidationError::UnsupportedEndianness(
                self.endianness,
            ));
        }
        if self.pointer_width != 64 {
            return Err(TargetValidationError::UnsupportedPointerWidth(
                self.pointer_width,
            ));
        }
        if self.calling_convention != expected.calling_convention {
            return Err(TargetValidationError::CallingConventionMismatch {
                triple: self.triple.as_str().into(),
                expected: expected.calling_convention,
                actual: self.calling_convention,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetValidationError {
    UnsupportedTriple(String),
    UnsupportedEndianness(Endianness),
    UnsupportedPointerWidth(u8),
    CallingConventionMismatch {
        triple: String,
        expected: CallingConvention,
        actual: CallingConvention,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbiType {
    Void,
    Pointer,
    USize,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
}

impl AbiType {
    fn canonical_name(self) -> &'static str {
        match self {
            Self::Void => "void",
            Self::Pointer => "pointer",
            Self::USize => "usize",
            Self::I8 => "i8",
            Self::U8 => "u8",
            Self::I16 => "i16",
            Self::U16 => "u16",
            Self::I32 => "i32",
            Self::U32 => "u32",
            Self::I64 => "i64",
            Self::U64 => "u64",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbiFunction {
    pub symbol: String,
    pub params: Vec<AbiType>,
    pub result: AbiType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbiFieldLayout {
    pub name: String,
    pub offset: u64,
    pub ty: AbiType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbiLayout {
    pub name: String,
    pub size: u64,
    pub alignment: u64,
    pub fields: Vec<AbiFieldLayout>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeIntrinsic {
    pub name: String,
    pub capability: String,
    pub params: Vec<AbiType>,
    pub result: AbiType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformImport {
    pub symbol: String,
    pub library: String,
    pub params: Vec<AbiType>,
    pub result: AbiType,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrapCode {
    NullDereference = 1,
    BoundsViolation = 2,
    IntegerDivisionByZero = 3,
    IntegerOverflow = 4,
    InvalidCast = 5,
    OutOfMemory = 6,
    StackOverflow = 7,
    AssertionFailed = 8,
    Unreachable = 9,
    PlatformError = 10,
}

impl TrapCode {
    pub const ALL: [Self; 10] = [
        Self::NullDereference,
        Self::BoundsViolation,
        Self::IntegerDivisionByZero,
        Self::IntegerOverflow,
        Self::InvalidCast,
        Self::OutOfMemory,
        Self::StackOverflow,
        Self::AssertionFailed,
        Self::Unreachable,
        Self::PlatformError,
    ];
}

impl From<TrapCode> for u8 {
    fn from(value: TrapCode) -> Self {
        value as Self
    }
}

impl TryFrom<u8> for TrapCode {
    type Error = InvalidTrapCode;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|code| u8::from(*code) == value)
            .ok_or(InvalidTrapCode(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidTrapCode(pub u8);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbiManifestV5 {
    pub abi_version: u32,
    pub target: TargetMetadata,
    pub imports: Vec<AbiFunction>,
    pub exports: Vec<AbiFunction>,
    pub layouts: Vec<AbiLayout>,
    pub trusted_runtime_intrinsics: Vec<RuntimeIntrinsic>,
    pub platform_imports: Vec<PlatformImport>,
    pub assembly_symbols: Vec<String>,
    pub traps: Vec<TrapCode>,
}

impl AbiManifestV5 {
    pub fn validate(&self) -> Result<(), ManifestValidationError> {
        if self.abi_version != ABI_V5 {
            return Err(ManifestValidationError::WrongAbiVersion(self.abi_version));
        }
        self.target
            .validate()
            .map_err(ManifestValidationError::InvalidTarget)?;

        let mut symbols = HashSet::new();
        for function in self.imports.iter().chain(&self.exports) {
            if !function.symbol.starts_with(RUNTIME_SYMBOL_PREFIX) {
                return Err(ManifestValidationError::UnversionedRuntimeSymbol {
                    symbol: function.symbol.clone(),
                });
            }
            if !symbols.insert(function.symbol.clone()) {
                return Err(ManifestValidationError::DuplicateSymbol {
                    symbol: function.symbol.clone(),
                });
            }
        }

        validate_named_contracts(
            self.trusted_runtime_intrinsics
                .iter()
                .map(|entry| entry.name.as_str()),
        )?;
        validate_named_contracts(
            self.platform_imports
                .iter()
                .map(|entry| entry.symbol.as_str()),
        )?;
        validate_layouts(&self.layouts)?;

        let actual_assembly: HashSet<_> =
            self.assembly_symbols.iter().map(String::as_str).collect();
        let expected_assembly: HashSet<_> = APPROVED_ASSEMBLY_SYMBOLS.into_iter().collect();
        if self.assembly_symbols.len() != APPROVED_ASSEMBLY_SYMBOLS.len()
            || actual_assembly != expected_assembly
        {
            return Err(ManifestValidationError::InvalidAssemblySymbols {
                actual: self.assembly_symbols.clone(),
            });
        }

        let actual_traps: HashSet<_> = self.traps.iter().copied().collect();
        let expected_traps: HashSet<_> = TrapCode::ALL.into_iter().collect();
        if self.traps.len() != TrapCode::ALL.len() || actual_traps != expected_traps {
            return Err(ManifestValidationError::InvalidTrapSet {
                actual: self.traps.iter().copied().map(u8::from).collect(),
            });
        }
        Ok(())
    }

    pub fn layout_hash(&self) -> String {
        canonical_layout_hash(&self.layouts)
    }
}

fn validate_named_contracts<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> Result<(), ManifestValidationError> {
    let mut seen = HashSet::new();
    for name in names {
        if name.is_empty() || !seen.insert(name) {
            return Err(ManifestValidationError::DuplicateSymbol {
                symbol: name.into(),
            });
        }
    }
    Ok(())
}

fn validate_layouts(layouts: &[AbiLayout]) -> Result<(), ManifestValidationError> {
    let mut names = HashSet::new();
    for layout in layouts {
        if layout.name.is_empty() || !names.insert(layout.name.as_str()) {
            return Err(ManifestValidationError::DuplicateLayout {
                name: layout.name.clone(),
            });
        }
        if layout.size == 0
            || layout.alignment == 0
            || !layout.alignment.is_power_of_two()
            || layout
                .fields
                .iter()
                .any(|field| field.offset >= layout.size)
        {
            return Err(ManifestValidationError::InvalidLayout {
                name: layout.name.clone(),
            });
        }
        let mut fields = HashSet::new();
        if layout
            .fields
            .iter()
            .any(|field| field.name.is_empty() || !fields.insert(field.name.as_str()))
        {
            return Err(ManifestValidationError::InvalidLayout {
                name: layout.name.clone(),
            });
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
    InvalidAssemblySymbols { actual: Vec<String> },
    InvalidTrapSet { actual: Vec<u8> },
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
        layout.fields.sort_by(|left, right| {
            left.offset
                .cmp(&right.offset)
                .then_with(|| left.name.cmp(&right.name))
        });
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
            hash_str(&mut hasher, field.ty.canonical_name());
        }
    }
    hex_digest(hasher.finalize())
}

pub fn canonical_source_hash(units: &[SourceUnit]) -> String {
    let mut canonical = units.to_vec();
    canonical.sort_by(|left, right| left.logical_path.cmp(&right.logical_path));
    let mut hasher = Sha256::new();
    for unit in canonical {
        hash_str(&mut hasher, &unit.logical_path);
        hash_str(&mut hasher, &unit.source);
    }
    hex_digest(hasher.finalize())
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
