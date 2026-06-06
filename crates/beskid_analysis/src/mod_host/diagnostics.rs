//! Mod-host diagnostics for registration validation and contract scheduling.
//!
//! These diagnostics fire **before** `mod.collect` and abort scheduling per
//! `site/website/src/content/docs/platform-spec/compiler/compiler-mods/mod-host-bridge/`.
//! Codes follow the platform-spec **E1821–E1835** (load failures) and **E1851–E1870**
//! (cross-artifact / scheduling conflicts) bands. They do not carry Beskid source spans —
//! conflicts are anchored at the mod artifact descriptor or project manifest path.

use std::fmt;
use std::path::PathBuf;

/// A single mod-host issue surfaced by registration validation or scheduling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModHostIssue {
    /// E1828 — `entrySymbol` referenced by a registration is empty / missing in the
    /// descriptor.
    MissingEntrySymbol {
        package_id: String,
        contract_id: String,
        type_id: String,
        descriptor: PathBuf,
    },
    /// E1829 — Duplicate `(contractId, typeId)` registration in one artifact.
    DuplicateRegistrationInArtifact {
        package_id: String,
        contract_id: String,
        type_id: String,
        descriptor: PathBuf,
    },
    /// E1830 — `registrations` empty but mod package declared required contracts.
    EmptyRegistrationsForRequiredMod {
        package_id: String,
        manifest: PathBuf,
    },
    /// E1831 — Required capability missing for one or more registrations.
    MissingCapability {
        package_id: String,
        contract_id: String,
        capability: String,
        manifest: PathBuf,
    },
    /// E1851 — Same `(contractId, typeId)` is provided by multiple mod artifacts. Hosts
    /// must reject this rather than picking a winner non-deterministically.
    ConflictingRegistrationAcrossArtifacts {
        contract_id: String,
        type_id: String,
        package_ids: Vec<String>,
    },
    /// E1852 — Two distinct artifacts export the **same `entrySymbol`** for different
    /// `(contractId, typeId)` tuples; loader cannot disambiguate native exports.
    DuplicateEntrySymbolAcrossArtifacts {
        entry_symbol: String,
        package_ids: Vec<String>,
    },
    /// E1853 — Unknown `contractId` value in a registration (not a known SDK contract).
    UnknownContractId {
        package_id: String,
        contract_id: String,
        descriptor: PathBuf,
    },
    /// E1854 — `Rewriter` registration without an attached `Analyzer` registration in
    /// the same artifact. The host requires analyzer-driven registration of rewrites
    /// per `compiler-mod-sdk`.
    RewriterWithoutAnalyzer {
        package_id: String,
        type_id: String,
        descriptor: PathBuf,
    },
    /// E1855 — Catch-all scheduling-stage conflict / failure when no narrower code
    /// from the **E1851–E1870** band applies.
    SchedulingFailure { package_id: String, message: String },
}

impl ModHostIssue {
    pub fn code(&self) -> &'static str {
        match self {
            ModHostIssue::MissingEntrySymbol { .. } => "E1828",
            ModHostIssue::DuplicateRegistrationInArtifact { .. } => "E1829",
            ModHostIssue::EmptyRegistrationsForRequiredMod { .. } => "E1830",
            ModHostIssue::MissingCapability { .. } => "E1831",
            ModHostIssue::ConflictingRegistrationAcrossArtifacts { .. } => "E1851",
            ModHostIssue::DuplicateEntrySymbolAcrossArtifacts { .. } => "E1852",
            ModHostIssue::UnknownContractId { .. } => "E1853",
            ModHostIssue::RewriterWithoutAnalyzer { .. } => "E1854",
            ModHostIssue::SchedulingFailure { .. } => "E1855",
        }
    }

    pub fn message(&self) -> String {
        match self {
            ModHostIssue::MissingEntrySymbol {
                package_id,
                contract_id,
                type_id,
                ..
            } => format!(
                "mod `{package_id}` registration `{contract_id}` for type `{type_id}` has empty `entrySymbol`"
            ),
            ModHostIssue::DuplicateRegistrationInArtifact {
                package_id,
                contract_id,
                type_id,
                ..
            } => format!(
                "mod `{package_id}` declares duplicate registration `({contract_id}, {type_id})` in one artifact"
            ),
            ModHostIssue::EmptyRegistrationsForRequiredMod { package_id, .. } => format!(
                "mod `{package_id}` declares required contract capabilities but its artifact `registrations` array is empty"
            ),
            ModHostIssue::MissingCapability {
                package_id,
                contract_id,
                capability,
                ..
            } => format!(
                "mod `{package_id}` registers `{contract_id}` but is missing required capability `{capability}`"
            ),
            ModHostIssue::ConflictingRegistrationAcrossArtifacts {
                contract_id,
                type_id,
                package_ids,
            } => format!(
                "registration `({contract_id}, {type_id})` is provided by multiple mod artifacts: {}",
                package_ids.join(", ")
            ),
            ModHostIssue::DuplicateEntrySymbolAcrossArtifacts {
                entry_symbol,
                package_ids,
            } => format!(
                "entry symbol `{entry_symbol}` is exported by multiple mod artifacts: {}",
                package_ids.join(", ")
            ),
            ModHostIssue::UnknownContractId {
                package_id,
                contract_id,
                ..
            } => format!(
                "mod `{package_id}` registers unknown contract id `{contract_id}` (not a recognized SDK contract)"
            ),
            ModHostIssue::RewriterWithoutAnalyzer {
                package_id,
                type_id,
                ..
            } => format!(
                "mod `{package_id}` registers Rewriter `{type_id}` but provides no Analyzer to drive it"
            ),
            ModHostIssue::SchedulingFailure {
                package_id,
                message,
            } => format!("mod `{package_id}` scheduling failed: {message}"),
        }
    }
}

impl fmt::Display for ModHostIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code(), self.message())
    }
}

/// Aggregate error returned by `mod.load` / pre-collect validation.
///
/// Multiple mods can fail in one pass; this carries every issue so the host emits all
/// of them deterministically before `mod.collect`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModHostDiagnostics {
    pub issues: Vec<ModHostIssue>,
}

impl ModHostDiagnostics {
    pub fn new(issues: Vec<ModHostIssue>) -> Self {
        Self { issues }
    }

    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn codes(&self) -> Vec<&'static str> {
        self.issues.iter().map(ModHostIssue::code).collect()
    }
}

impl fmt::Display for ModHostDiagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "mod host scheduling aborted before `mod.collect`:")?;
        for issue in &self.issues {
            writeln!(f, "  {issue}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ModHostDiagnostics {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_in_artifact_uses_e1829() {
        let issue = ModHostIssue::DuplicateRegistrationInArtifact {
            package_id: "ModA".to_owned(),
            contract_id: "Beskid.Compiler.Collect.Generator".to_owned(),
            type_id: "ModA.Emit".to_owned(),
            descriptor: PathBuf::from("/tmp/desc.json"),
        };
        assert_eq!(issue.code(), "E1829");
        assert!(issue.message().contains("ModA.Emit"));
    }

    #[test]
    fn cross_artifact_conflict_uses_e1851() {
        let issue = ModHostIssue::ConflictingRegistrationAcrossArtifacts {
            contract_id: "Beskid.Compiler.Collect.Generator".to_owned(),
            type_id: "Shared.Type".to_owned(),
            package_ids: vec!["ModA".to_owned(), "ModB".to_owned()],
        };
        assert_eq!(issue.code(), "E1851");
    }

    #[test]
    fn diagnostics_aggregate_codes() {
        let diag = ModHostDiagnostics::new(vec![
            ModHostIssue::DuplicateRegistrationInArtifact {
                package_id: "ModA".to_owned(),
                contract_id: "Beskid.Compiler.Collect.Generator".to_owned(),
                type_id: "T".to_owned(),
                descriptor: PathBuf::from("/tmp/a"),
            },
            ModHostIssue::ConflictingRegistrationAcrossArtifacts {
                contract_id: "Beskid.Compiler.Collect.Analyzer".to_owned(),
                type_id: "U".to_owned(),
                package_ids: vec!["ModA".into(), "ModB".into()],
            },
        ]);
        assert_eq!(diag.codes(), vec!["E1829", "E1851"]);
        assert!(!diag.is_empty());
    }
}
