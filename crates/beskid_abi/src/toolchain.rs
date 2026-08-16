//! Fail-closed toolchain discovery and validation for external tools.
//!
//! This module mirrors the ABI-v5 runtime-kit validation discipline
//! (`runtime_kit`) but targets external toolchain tools (rustc, cargo,
//! dotnet, linkers) rather than the runtime ABI. It is the host-side
//! scaffold for the `Beskid.Glue.ToolchainProbe` contract: it declares the
//! typed `ToolSpec`/`ResolvedTool`/`ToolchainError` model and a fail-closed
//! validation surface. Full implementation for rustc/cargo/dotnet/linkers
//! lands in the 0.5 delivery; the 0.4 delivery ships only the contract
//! scaffold and the typed model.
//!
//! Like the runtime kit, there is no search-path or nearest-tool fallback:
//! discovery is exact-prefix and validation rejects any drift before link
//! or load.

use serde::{Deserialize, Serialize};

/// The capability tag a tool provides. A glue backend declares which
/// capabilities it requires and the probe resolves exactly one tool per
/// capability before the backend runs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCapability {
    /// Compile Rust source to a native library (`rustc`).
    Rustc,
    /// Build a Rust crate from a manifest (`cargo`).
    Cargo,
    /// Compile .NET source / build a .NET project (`dotnet`).
    Dotnet,
    /// Link native objects into a shared library or executable (`cc`/`cl`).
    Linker,
    /// Read .NET ECMA-335 signatures (`dotscope`).
    Dotscope,
}

/// A tool specification: what the glue layer is looking for. The probe
/// resolves exactly one `ResolvedTool` per `ToolSpec`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolSpec {
    pub name: String,
    pub capability: ToolCapability,
    /// The minimum accepted semver version string (e.g. `"1.80.0"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_version: Option<String>,
    /// The target triple the tool must target, when relevant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_triple: Option<String>,
    /// The expected sha256 of the resolved tool binary, when pinning is
    /// required. Absent means the probe verifies presence and version only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sha256: Option<String>,
}

/// A resolved tool: the probe's success result. Carries the exact path,
/// observed version, and (when pinned) the verified sha256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedTool {
    pub name: String,
    pub capability: ToolCapability,
    pub path: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

impl ResolvedTool {
    /// Validate that a resolved tool satisfies a specification. This is the
    /// fail-closed gate the glue backends consult before emitting or
    /// linking. The 0.4 scaffold performs structural validation only; full
    /// sha256/version comparison lands in 0.5.
    pub fn satisfies(&self, spec: &ToolSpec) -> Result<(), ToolchainError> {
        if self.name != spec.name {
            return Err(ToolchainError::NameMismatch { expected: spec.name.clone(), actual: self.name.clone() });
        }
        if self.capability != spec.capability {
            return Err(ToolchainError::CapabilityMismatch {
                expected: spec.capability.clone(),
                actual: self.capability.clone(),
            });
        }
        if let Some(expected) = &spec.expected_sha256 {
            match &self.sha256 {
                Some(actual) if actual == expected => {}
                Some(actual) => {
                    return Err(ToolchainError::HashMismatch {
                        tool: self.name.clone(),
                        expected: expected.clone(),
                        actual: actual.clone(),
                    });
                }
                None => return Err(ToolchainError::MissingHash { tool: self.name.clone() }),
            }
        }
        Ok(())
    }
}

/// The atomized toolchain-probe result enum. Each variant carries the
/// failing field, mirroring the `Status`/`FiberJoinStatus` pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolchainError {
    NameMismatch { expected: String, actual: String },
    CapabilityMismatch { expected: ToolCapability, actual: ToolCapability },
    HashMismatch { tool: String, expected: String, actual: String },
    MissingHash { tool: String },
    NotFound { tool: String },
    NotARegularFile { path: String },
    VersionTooOld { tool: String, minimum: String, actual: String },
    TargetMismatch { tool: String, expected: String, actual: String },
}

impl std::fmt::Display for ToolchainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NameMismatch { expected, actual } => {
                write!(f, "tool name mismatch: expected `{expected}`, actual `{actual}`")
            }
            Self::CapabilityMismatch { expected, actual } => {
                write!(f, "tool capability mismatch: expected {expected:?}, actual {actual:?}")
            }
            Self::HashMismatch { tool, expected, actual } => {
                write!(f, "tool `{tool}` sha256 mismatch: expected {expected}, actual {actual}")
            }
            Self::MissingHash { tool } => {
                write!(f, "tool `{tool}` was resolved without a hash but a pin was required")
            }
            Self::NotFound { tool } => write!(f, "tool `{tool}` was not found"),
            Self::NotARegularFile { path } => write!(f, "tool path `{path}` is not a regular file"),
            Self::VersionTooOld { tool, minimum, actual } => {
                write!(f, "tool `{tool}` version {actual} is older than the minimum {minimum}")
            }
            Self::TargetMismatch { tool, expected, actual } => {
                write!(f, "tool `{tool}` targets {actual}, expected {expected}")
            }
        }
    }
}

impl std::error::Error for ToolchainError {}
