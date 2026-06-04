//! Contract invocation abstraction for `mod.collect` / `mod.generate` / `mod.analyze` /
//! `mod.rewrite` phases.
//!
//! The host calls one of [`ContractInvoker`]'s methods for every scheduled
//! `(contractId, typeId, entrySymbol)` tuple discovered by `mod.load`. Implementations
//! decide how to reach the Beskid-side contract instance — current implementations are:
//!
//! * [`StubContractInvoker`] — default for tests and pre-AOT bring-up. Records
//!   invocations and returns empty results so `mod.collect`-`mod.rewrite` complete
//!   deterministically.
//! * [`ScriptedContractInvoker`] — test helper that scripts per-`typeId` outcomes for
//!   assertions in beskid_engine and beskid_tests.
//!
//! Future implementations (`NativeContractInvoker`) will dlopen the AOT object and
//! call `entry_symbol` with the Beskid → C ABI defined in `beskid_abi`. The trait
//! shape stays stable so production code can switch implementations without touching
//! `mod_host` orchestration.

use std::sync::Mutex;

use super::types::ContractRegistration;

/// Outcome from a `Collector.Collect` invocation. The MVP carries the `typeId` of the
/// contract that was invoked and any extra "scope narrowing" tokens it produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectorOutcome {
    pub type_id: String,
    pub narrowed_targets: Vec<String>,
}

/// Outcome from a `Generator.Generate` invocation: zero or more typed AST contributions
/// represented as canonical strings (the host re-parses them when present). The MVP
/// keeps this string-shaped because typed merge is still scaffolded; concrete typed
/// shapes will replace `contributions` once `merge::merge_generated_syntax` lands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GeneratorOutcome {
    pub type_id: String,
    pub contributions: Vec<String>,
}

/// Outcome from `Analyzer.Analyze` — diagnostics it wants the host to emit and rewrite
/// fixes it wants `Rewriter` to run on its behalf.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnalyzerOutcome {
    pub type_id: String,
    pub diagnostics: Vec<AnalyzerDiagnostic>,
    pub fix_targets: Vec<String>,
}

/// One diagnostic emitted by an Analyzer contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzerDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: AnalyzerSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyzerSeverity {
    Error,
    Warning,
    Note,
}

/// Outcome from `Rewriter.Rewrite` — record-only for the MVP since structural rewrite
/// application is performed by the Rust-side typed pipeline (see `query_bridge.rs`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RewriterOutcome {
    pub type_id: String,
    pub applied_fix_count: u32,
}

/// Trait implemented by contract invokers. Each method is called once per scheduled
/// registration of the matching contract kind.
pub trait ContractInvoker: Send + Sync {
    fn invoke_collector(
        &self,
        registration: &ContractRegistration,
    ) -> Result<CollectorOutcome, ContractInvocationError>;

    fn invoke_generator(
        &self,
        registration: &ContractRegistration,
    ) -> Result<GeneratorOutcome, ContractInvocationError>;

    fn invoke_analyzer(
        &self,
        registration: &ContractRegistration,
        snapshot: Option<&crate::services::SemanticSnapshot>,
    ) -> Result<AnalyzerOutcome, ContractInvocationError>;

    fn invoke_rewriter(
        &self,
        registration: &ContractRegistration,
    ) -> Result<RewriterOutcome, ContractInvocationError>;
}

/// Error returned when a contract invocation cannot be dispatched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractInvocationError {
    pub package_id: String,
    pub contract_id: String,
    pub type_id: String,
    pub message: String,
}

impl std::fmt::Display for ContractInvocationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "mod `{}` contract `{}` for `{}` failed to invoke: {}",
            self.package_id, self.contract_id, self.type_id, self.message
        )
    }
}

impl std::error::Error for ContractInvocationError {}

/// Default invoker for tests and the v0.3 MVP. Records each invocation and returns
/// empty outcomes. Production hosts will swap in an AOT-dlopen invoker; the host
/// pipeline never assumes it has a real native callable.
#[derive(Debug, Default)]
pub struct StubContractInvoker {
    log: Mutex<Vec<InvocationKind>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvocationKind {
    Collector {
        contract_id: String,
        type_id: String,
        entry_symbol: String,
    },
    Generator {
        contract_id: String,
        type_id: String,
        entry_symbol: String,
    },
    Analyzer {
        contract_id: String,
        type_id: String,
        entry_symbol: String,
        snapshot_version: Option<u32>,
        snapshot_staged_through: Option<String>,
    },
    Rewriter {
        contract_id: String,
        type_id: String,
        entry_symbol: String,
    },
}

impl InvocationKind {
    pub fn type_id(&self) -> &str {
        match self {
            InvocationKind::Collector { type_id, .. }
            | InvocationKind::Generator { type_id, .. }
            | InvocationKind::Analyzer { type_id, .. }
            | InvocationKind::Rewriter { type_id, .. } => type_id,
        }
    }

    pub fn contract_id(&self) -> &str {
        match self {
            InvocationKind::Collector { contract_id, .. }
            | InvocationKind::Generator { contract_id, .. }
            | InvocationKind::Analyzer { contract_id, .. }
            | InvocationKind::Rewriter { contract_id, .. } => contract_id,
        }
    }
}

impl StubContractInvoker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of every invocation seen so far in the order they were dispatched.
    pub fn invocations(&self) -> Vec<InvocationKind> {
        self.log.lock().expect("invoker log").clone()
    }

    fn record(&self, kind: InvocationKind) {
        self.log.lock().expect("invoker log").push(kind);
    }
}

impl ContractInvoker for StubContractInvoker {
    fn invoke_collector(
        &self,
        registration: &ContractRegistration,
    ) -> Result<CollectorOutcome, ContractInvocationError> {
        self.record(InvocationKind::Collector {
            contract_id: registration.contract_id.clone(),
            type_id: registration.type_id.clone(),
            entry_symbol: registration.entry_symbol.clone(),
        });
        Ok(CollectorOutcome {
            type_id: registration.type_id.clone(),
            ..Default::default()
        })
    }

    fn invoke_generator(
        &self,
        registration: &ContractRegistration,
    ) -> Result<GeneratorOutcome, ContractInvocationError> {
        self.record(InvocationKind::Generator {
            contract_id: registration.contract_id.clone(),
            type_id: registration.type_id.clone(),
            entry_symbol: registration.entry_symbol.clone(),
        });
        Ok(GeneratorOutcome {
            type_id: registration.type_id.clone(),
            ..Default::default()
        })
    }

    fn invoke_analyzer(
        &self,
        registration: &ContractRegistration,
        snapshot: Option<&crate::services::SemanticSnapshot>,
    ) -> Result<AnalyzerOutcome, ContractInvocationError> {
        self.record(InvocationKind::Analyzer {
            contract_id: registration.contract_id.clone(),
            type_id: registration.type_id.clone(),
            entry_symbol: registration.entry_symbol.clone(),
            snapshot_version: snapshot.map(|snap| snap.version),
            snapshot_staged_through: snapshot.map(|snap| snap.staged_through.to_owned()),
        });
        Ok(AnalyzerOutcome {
            type_id: registration.type_id.clone(),
            ..Default::default()
        })
    }

    fn invoke_rewriter(
        &self,
        registration: &ContractRegistration,
    ) -> Result<RewriterOutcome, ContractInvocationError> {
        self.record(InvocationKind::Rewriter {
            contract_id: registration.contract_id.clone(),
            type_id: registration.type_id.clone(),
            entry_symbol: registration.entry_symbol.clone(),
        });
        Ok(RewriterOutcome {
            type_id: registration.type_id.clone(),
            ..Default::default()
        })
    }
}

/// Test-only invoker that lets tests script outcomes per `(contract_id, type_id)`
/// pair. Falls back to [`StubContractInvoker`] behavior when no script is registered.
#[derive(Debug, Default)]
pub struct ScriptedContractInvoker {
    pub generator_contributions: Mutex<Vec<(String, Vec<String>)>>,
    pub analyzer_diagnostics: Mutex<Vec<(String, Vec<AnalyzerDiagnostic>)>>,
    pub recorded: StubContractInvoker,
}

impl ScriptedContractInvoker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_generator_contribution(
        self,
        type_id: impl Into<String>,
        contributions: Vec<String>,
    ) -> Self {
        self.generator_contributions
            .lock()
            .expect("scripted contributions")
            .push((type_id.into(), contributions));
        self
    }

    pub fn with_analyzer_diagnostic(
        self,
        type_id: impl Into<String>,
        diagnostics: Vec<AnalyzerDiagnostic>,
    ) -> Self {
        self.analyzer_diagnostics
            .lock()
            .expect("scripted diagnostics")
            .push((type_id.into(), diagnostics));
        self
    }

    pub fn invocations(&self) -> Vec<InvocationKind> {
        self.recorded.invocations()
    }
}

impl ContractInvoker for ScriptedContractInvoker {
    fn invoke_collector(
        &self,
        registration: &ContractRegistration,
    ) -> Result<CollectorOutcome, ContractInvocationError> {
        self.recorded.invoke_collector(registration)
    }

    fn invoke_generator(
        &self,
        registration: &ContractRegistration,
    ) -> Result<GeneratorOutcome, ContractInvocationError> {
        let mut outcome = self.recorded.invoke_generator(registration)?;
        let scripted = self
            .generator_contributions
            .lock()
            .expect("scripted contributions");
        for (type_id, contributions) in scripted.iter() {
            if registration.type_id == *type_id {
                outcome.contributions.extend(contributions.iter().cloned());
            }
        }
        Ok(outcome)
    }

    fn invoke_analyzer(
        &self,
        registration: &ContractRegistration,
        snapshot: Option<&crate::services::SemanticSnapshot>,
    ) -> Result<AnalyzerOutcome, ContractInvocationError> {
        let mut outcome = self.recorded.invoke_analyzer(registration, snapshot)?;
        let scripted = self
            .analyzer_diagnostics
            .lock()
            .expect("scripted diagnostics");
        for (type_id, diagnostics) in scripted.iter() {
            if registration.type_id == *type_id {
                outcome.diagnostics.extend(diagnostics.iter().cloned());
            }
        }
        Ok(outcome)
    }

    fn invoke_rewriter(
        &self,
        registration: &ContractRegistration,
    ) -> Result<RewriterOutcome, ContractInvocationError> {
        self.recorded.invoke_rewriter(registration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(contract: &str, ty: &str, sym: &str) -> ContractRegistration {
        ContractRegistration {
            contract_id: contract.to_owned(),
            type_id: ty.to_owned(),
            entry_symbol: sym.to_owned(),
        }
    }

    #[test]
    fn stub_records_each_invocation_kind() {
        let invoker = StubContractInvoker::new();
        invoker
            .invoke_collector(&r("Beskid.Compiler.Collect.Collector", "T1", "c"))
            .unwrap();
        invoker
            .invoke_generator(&r("Beskid.Compiler.Collect.Generator", "T2", "g"))
            .unwrap();
        invoker
            .invoke_analyzer(&r("Beskid.Compiler.Collect.Analyzer", "T3", "a"), None)
            .unwrap();
        invoker
            .invoke_rewriter(&r("Beskid.Compiler.Collect.Rewriter", "T4", "r"))
            .unwrap();
        let log = invoker.invocations();
        assert_eq!(log.len(), 4);
        assert!(matches!(log[0], InvocationKind::Collector { .. }));
        assert!(matches!(log[1], InvocationKind::Generator { .. }));
        assert!(matches!(log[2], InvocationKind::Analyzer { .. }));
        assert!(matches!(log[3], InvocationKind::Rewriter { .. }));
    }

    #[test]
    fn scripted_overlays_generator_contributions() {
        let invoker = ScriptedContractInvoker::new().with_generator_contribution(
            "T2",
            vec!["pub fn synthetic_generated() { return; }".into()],
        );
        let outcome = invoker
            .invoke_generator(&r("Beskid.Compiler.Collect.Generator", "T2", "g"))
            .unwrap();
        assert_eq!(outcome.contributions.len(), 1);
    }

    #[test]
    fn scripted_overlays_analyzer_diagnostics() {
        let invoker = ScriptedContractInvoker::new().with_analyzer_diagnostic(
            "TA",
            vec![AnalyzerDiagnostic {
                code: "ModA0001".into(),
                message: "test".into(),
                severity: AnalyzerSeverity::Warning,
            }],
        );
        let outcome = invoker
            .invoke_analyzer(&r("Beskid.Compiler.Collect.Analyzer", "TA", "a"), None)
            .unwrap();
        assert_eq!(outcome.diagnostics.len(), 1);
        assert_eq!(outcome.diagnostics[0].code, "ModA0001");
    }
}
