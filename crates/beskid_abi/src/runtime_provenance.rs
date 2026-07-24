//! Deterministic, manifest-derived input for ABI-v5 runtime provenance checks.
//!
//! This module deliberately consumes explicit symbol-list files.  Extracting symbol tables from
//! Mach-O, ELF, or COFF binaries is a separate platform-adapter concern; keeping that parsing out
//! of the release contract makes the policy identical on every host.

use std::fmt;

use serde::Serialize;

use crate::abi_v5::{AbiManifestV5, ManifestValidationError, RuntimeAuditMetadata, TargetMetadata};
use crate::runtime_source::canonical_runtime_source_hash;

/// Imports emitted by the ELF linker for a shared object before application code is linked.
///
/// These are intentionally not part of the static runtime manifest: they describe the Linux
/// dynamic-loader/toolchain boundary, never a runtime dependency. In particular,
/// `__tls_get_addr` resolves dynamic TLS for a runtime `.so` that can be loaded with `dlopen`;
/// initial-exec TLS would make the runtime require unavailable static TLS.
/// Keep this list exact so the shared-artifact audit continues to reject Rust runtime linkage and
/// other undeclared imports.
const LINUX_ELF_SHARED_LOADER_IMPORTS: &[&str] =
    &["_ITM_deregisterTMCloneTable", "_ITM_registerTMCloneTable", "__cxa_finalize", "__gmon_start__", "__tls_get_addr"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeProvenanceAudit {
    pub target: String,
    pub required_exports: Vec<String>,
    pub allowed_imports: Vec<String>,
    pub allowed_exports: Vec<String>,
    pub forbidden_symbol_families: Vec<String>,
}

impl RuntimeProvenanceAudit {
    /// Builds the only audit policy valid for a supported ABI-v5 target.
    pub fn canonical(target: TargetMetadata) -> Result<Self, ManifestValidationError> {
        let manifest = AbiManifestV5::canonical_runtime(target.clone());
        let metadata = RuntimeAuditMetadata::for_manifest(&manifest, &canonical_runtime_source_hash())?;
        let required_exports = metadata.loader_required_exports.clone();
        Ok(Self {
            target: target.triple.as_str().into(),
            required_exports,
            allowed_imports: metadata.allowed_imports,
            allowed_exports: metadata.allowed_exports,
            forbidden_symbol_families: metadata.forbidden_rust_symbols,
        })
    }

    /// Serialize a stable, machine-readable release-policy artifact.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Produce a portable symbol-list fixture in the spelling emitted by this target's object
    /// tools. This is also the contract platform adapters must reproduce before publication.
    pub fn fixture_symbol_list(&self) -> Result<SymbolList, SymbolListError> {
        let metadata = supported_target(&self.target)?;
        let prefix = metadata.symbol_prefix;
        Ok(SymbolList {
            target: self.target.clone(),
            defined: self.allowed_exports.iter().map(|symbol| format!("{prefix}{symbol}")).collect(),
            undefined: self.allowed_imports.iter().map(|symbol| format!("{prefix}{symbol}")).collect(),
        })
    }

    /// Verify an explicit symbol-list file against this target's manifest-derived policy.
    pub fn verify(&self, symbols: &SymbolList) -> Result<(), SymbolListError> {
        self.verify_with_additional_imports(symbols, &[])
    }

    /// Verify an ELF shared-runtime symbol list against its manifest-derived policy.
    ///
    /// The accepted Linux imports are emitted by the dynamic-loader/toolchain boundary.
    pub fn verify_shared(&self, symbols: &SymbolList) -> Result<(), SymbolListError> {
        let additional_imports = match self.target.as_str() {
            "x86_64-unknown-linux-gnu" => LINUX_ELF_SHARED_LOADER_IMPORTS,
            _ => &[],
        };
        self.verify_with_additional_imports(symbols, additional_imports)
    }

    /// Verify a host platform static archive that still contains `platform_tls.o`.
    ///
    /// The archive is not a final linked image; GD TLS leaves `__tls_get_addr` undefined
    /// until the consumer links the shared loader boundary. Allow only that import so
    /// other undeclared static-archive imports continue to fail closed.
    pub fn verify_static_archive(&self, symbols: &SymbolList) -> Result<(), SymbolListError> {
        let additional_imports = match self.target.as_str() {
            "x86_64-unknown-linux-gnu" => &["__tls_get_addr"][..],
            _ => &[],
        };
        self.verify_with_additional_imports(symbols, additional_imports)
    }

    fn verify_with_additional_imports(
        &self,
        symbols: &SymbolList,
        additional_imports: &[&str],
    ) -> Result<(), SymbolListError> {
        if symbols.target != self.target {
            return Err(SymbolListError::TargetMismatch {
                expected: self.target.clone(),
                actual: symbols.target.clone(),
            });
        }
        let mut allowed_imports = self.allowed_imports.clone();
        allowed_imports.extend(additional_imports.iter().map(|symbol| (*symbol).into()));
        allowed_imports.sort();
        allowed_imports.dedup();
        RuntimeAuditMetadata {
            allowed_imports,
            allowed_exports: self.allowed_exports.clone(),
            loader_required_exports: self.required_exports.clone(),
            forbidden_rust_symbols: self.forbidden_symbol_families.clone(),
            object_format: target_object_format(&self.target)?,
            symbol_prefix: target_symbol_prefix(&self.target)?,
            layout_hash: String::new(),
            runtime_source_hash: String::new(),
        }
        .audit_linked_runtime_symbol_tables(
            &self.required_exports,
            symbols.defined.iter().map(String::as_str),
            symbols.undefined.iter().map(String::as_str),
        )
        .map_err(SymbolListError::Policy)
    }
}

fn target_object_format(target: &str) -> Result<String, SymbolListError> {
    supported_target(target).map(|metadata| metadata.object_format.as_str().into())
}

fn target_symbol_prefix(target: &str) -> Result<String, SymbolListError> {
    supported_target(target).map(|metadata| metadata.symbol_prefix)
}

fn supported_target(target: &str) -> Result<TargetMetadata, SymbolListError> {
    TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple.as_str() == target)
        .ok_or_else(|| SymbolListError::UnsupportedTarget(target.into()))
}

/// A host-independent representation of defined and undefined object symbols.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolList {
    pub target: String,
    pub defined: Vec<String>,
    pub undefined: Vec<String>,
}

/// Parse the deliberately small line-oriented release input format:
/// `target=<triple>`, `defined=<symbol>`, and `undefined=<symbol>`.
pub fn parse_symbol_list(input: &str) -> Result<SymbolList, SymbolListError> {
    let mut target = None;
    let mut defined = Vec::new();
    let mut undefined = Vec::new();
    for (line_number, raw) in input.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(SymbolListError::InvalidLine { line: line_number + 1 });
        };
        if value.is_empty() {
            return Err(SymbolListError::InvalidLine { line: line_number + 1 });
        }
        match key {
            "target" if target.is_none() => target = Some(value.into()),
            "defined" => defined.push(value.into()),
            "undefined" => undefined.push(value.into()),
            _ => {
                return Err(SymbolListError::InvalidLine { line: line_number + 1 });
            }
        }
    }
    Ok(SymbolList { target: target.ok_or(SymbolListError::MissingTarget)?, defined, undefined })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolListError {
    MissingTarget,
    InvalidLine { line: usize },
    UnsupportedTarget(String),
    TargetMismatch { expected: String, actual: String },
    Policy(String),
}

impl fmt::Display for SymbolListError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTarget => write!(formatter, "symbol list is missing target=<triple>"),
            Self::InvalidLine { line } => write!(formatter, "invalid symbol-list line {line}"),
            Self::UnsupportedTarget(target) => {
                write!(formatter, "unsupported ABI-v5 target `{target}`")
            }
            Self::TargetMismatch { expected, actual } => {
                write!(formatter, "symbol-list target mismatch: expected `{expected}`, got `{actual}`")
            }
            Self::Policy(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for SymbolListError {}
