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

use std::path::PathBuf;

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
    /// Explicit path to the tool binary. When set, the probe resolves exactly
    /// this path with no search. Mutually exclusive with `prefix`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Installed prefix of the toolchain. The probe resolves the binary at
    /// `<prefix>/bin/<name>`, mirroring the `runtime_kit` layout. Mutually
    /// exclusive with `path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
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
    /// Observed version string, when the probe captured one. The 0.4 probe
    /// never captures a version, so this is `None` until 0.5 lands version
    /// interrogation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Observed target triple, when the probe captured one. The 0.4 probe
    /// never captures a target, so this is `None` until 0.5 lands target
    /// validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_triple: Option<String>,
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

/// Resolve a tool by exact-path discovery.
///
/// This is the 0.4 scaffold of the `Beskid.Glue.ToolchainProbe` contract: it
/// verifies the tool exists at the expected path and is a regular file. Full
/// version, sha256, and target-triple comparison lands in 0.5; the returned
/// `ResolvedTool` carries `version: None`, `sha256: None`, and
/// `target_triple: None`.
///
/// Discovery is fail-closed and exact-prefix, mirroring `runtime_kit`: there
/// is no `PATH` search and no nearest-tool fallback. The probe resolves the
/// binary from `spec.path` (an explicit binary path) or
/// `<spec.prefix>/bin/<spec.name>` (the installed-prefix layout). If neither
/// is supplied, the probe fails closed with `NotFound`.
pub fn probe(spec: &ToolSpec) -> Result<ResolvedTool, ToolchainError> {
    let path = resolve_tool_path(spec)?;
    let metadata = std::fs::metadata(&path).map_err(|_| ToolchainError::NotFound { tool: spec.name.clone() })?;
    if !metadata.is_file() {
        return Err(ToolchainError::NotARegularFile { path });
    }
    Ok(ResolvedTool {
        name: spec.name.clone(),
        capability: spec.capability.clone(),
        path,
        version: None,
        sha256: None,
        target_triple: None,
    })
}

/// Resolve the exact binary path for `spec` without any search fallback.
fn resolve_tool_path(spec: &ToolSpec) -> Result<String, ToolchainError> {
    if let Some(path) = &spec.path {
        return Ok(path.clone());
    }
    if let Some(prefix) = &spec.prefix {
        let binary = PathBuf::from(prefix).join("bin").join(&spec.name);
        return Ok(binary.to_string_lossy().into_owned());
    }
    Err(ToolchainError::NotFound { tool: spec.name.clone() })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_with_path(name: &str, path: &str) -> ToolSpec {
        ToolSpec {
            name: name.to_owned(),
            capability: ToolCapability::Rustc,
            path: Some(path.to_owned()),
            prefix: None,
            minimum_version: None,
            target_triple: None,
            expected_sha256: None,
        }
    }

    fn spec_with_prefix(name: &str, prefix: &str) -> ToolSpec {
        ToolSpec {
            name: name.to_owned(),
            capability: ToolCapability::Rustc,
            path: None,
            prefix: Some(prefix.to_owned()),
            minimum_version: None,
            target_triple: None,
            expected_sha256: None,
        }
    }

    fn unique_temp_dir(test_name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("beskid_abi_toolchain_probe_{test_name}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn probe_resolves_explicit_path_regular_file() {
        let dir = unique_temp_dir("explicit_path");
        let binary = dir.join("rustc");
        std::fs::write(&binary, b"#!/bin/sh\n").expect("write binary");

        let spec = spec_with_path("rustc", binary.to_str().unwrap());
        let resolved = probe(&spec).expect("probe resolves regular file");
        assert_eq!(resolved.name, "rustc");
        assert_eq!(resolved.capability, ToolCapability::Rustc);
        assert_eq!(resolved.path, binary.to_string_lossy());
        assert_eq!(resolved.version, None);
        assert_eq!(resolved.sha256, None);
        assert_eq!(resolved.target_triple, None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn probe_resolves_prefix_bin_layout() {
        let dir = unique_temp_dir("prefix_layout");
        let bin_dir = dir.join("bin");
        std::fs::create_dir_all(&bin_dir).expect("create bin dir");
        let binary = bin_dir.join("cargo");
        std::fs::write(&binary, b"#!/bin/sh\n").expect("write binary");

        let spec = spec_with_prefix("cargo", dir.to_str().unwrap());
        let resolved = probe(&spec).expect("probe resolves prefix/bin layout");
        assert_eq!(resolved.name, "cargo");
        assert_eq!(resolved.path, binary.to_string_lossy());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn probe_fails_closed_when_path_missing() {
        let spec = spec_with_path("rustc", "/nonexistent/path/to/rustc");
        let error = probe(&spec).expect_err("missing path must fail closed");
        assert!(matches!(error, ToolchainError::NotFound { tool } if tool == "rustc"));
    }

    #[test]
    fn probe_rejects_non_regular_file() {
        let dir = unique_temp_dir("non_regular");
        // The directory itself is not a regular file.
        let spec = spec_with_path("rustc", dir.to_str().unwrap());
        let error = probe(&spec).expect_err("directory must be rejected as non-regular");
        assert!(matches!(error, ToolchainError::NotARegularFile { .. }));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn probe_fails_closed_without_path_or_prefix() {
        let spec = ToolSpec {
            name: "rustc".to_owned(),
            capability: ToolCapability::Rustc,
            path: None,
            prefix: None,
            minimum_version: None,
            target_triple: None,
            expected_sha256: None,
        };
        let error = probe(&spec).expect_err("no path or prefix must fail closed");
        assert!(matches!(error, ToolchainError::NotFound { tool } if tool == "rustc"));
    }

    #[test]
    fn probe_prefers_explicit_path_over_prefix() {
        let dir = unique_temp_dir("path_precedence");
        let binary = dir.join("rustc");
        std::fs::write(&binary, b"#!/bin/sh\n").expect("write binary");

        let mut spec = spec_with_path("rustc", binary.to_str().unwrap());
        spec.prefix = Some("/nonexistent/prefix".to_owned());
        let resolved = probe(&spec).expect("explicit path wins over prefix");
        assert_eq!(resolved.path, binary.to_string_lossy());

        std::fs::remove_dir_all(&dir).ok();
    }
}
