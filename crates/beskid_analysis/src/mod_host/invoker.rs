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
//! * [`NativeContractInvoker`](super::native::NativeContractInvoker) — records artifact
//!   object paths and delegates to a stub until shared-library dlopen dispatch lands.
//!
//! Future implementations will dlopen the AOT object and
//! call `entry_symbol` with the Beskid → C ABI defined in `beskid_abi`. The trait
//! shape stays stable so production code can switch implementations without touching
//! `mod_host` orchestration.

use std::sync::Mutex;

use beskid_abi::{ModCollectRequest, ModGenerationRequest};

use crate::syntax::Spanned;

use super::emit_bridge;
use super::types::{ContractRegistration, ProgramItem};

/// Outcome from a `Collector.Collect` invocation. The MVP carries the `typeId` of the
/// contract that was invoked and any extra "scope narrowing" tokens it produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectorOutcome {
    pub type_id: String,
    pub narrowed_targets: Vec<String>,
}

/// Outcome from a `Generator.Generate` invocation: zero or more typed AST contributions
/// spliced directly into the host program by `merge::merge_generated_syntax`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GeneratorOutcome {
    pub type_id: String,
    pub typed_items: Vec<Spanned<ProgramItem>>,
    pub code_outputs: Vec<super::generate_output::CodeGenerateOutput>,
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnalyzerDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: AnalyzerSeverity,
    /// Byte offset range in the entry source: `(start, end)`.
    /// When `None`, the host falls back to a whole-file span so the diagnostic is
    /// still surfaced (e.g. for analyzers that report project-wide issues).
    pub span: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnalyzerSeverity {
    Error,
    #[default]
    Warning,
    Note,
}

/// One text edit produced by a Rewriter contract. Edits are byte-offset ranges into
/// the entry source; the host applies them right-to-left to preserve earlier offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewriteEdit {
    /// Insert `text` at byte `offset`.
    Insert { offset: usize, text: String },
    /// Replace bytes `start..end` with `text`.
    Replace { start: usize, end: usize, text: String },
    /// Delete bytes `start..end`.
    Delete { start: usize, end: usize },
}

/// Outcome from `Rewriter.Rewrite` — carries the `typeId`, a count of fixes the
/// rewriter applied internally, and the text edits it wants the host to apply to
/// the entry source. The host applies `edits` right-to-left after all rewriters
/// have run (see `rewrite::apply_edits`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RewriterOutcome {
    pub type_id: String,
    pub applied_fix_count: u32,
    /// Text edits the rewriter wants applied to the source.
    pub edits: Vec<RewriteEdit>,
}

/// Trait implemented by contract invokers. Each method is called once per scheduled
/// registration of the matching contract kind.
pub trait ContractInvoker: Send + Sync {
    fn invoke_collector(
        &self,
        registration: &ContractRegistration,
        request: &ModCollectRequest,
    ) -> Result<CollectorOutcome, ContractInvocationError>;

    fn invoke_generator(
        &self,
        registration: &ContractRegistration,
        request: &ModGenerationRequest,
    ) -> Result<GeneratorOutcome, ContractInvocationError>;

    fn invoke_analyzer(
        &self,
        registration: &ContractRegistration,
        request: &ModCollectRequest,
        snapshot: Option<&crate::services::SemanticSnapshot>,
    ) -> Result<AnalyzerOutcome, ContractInvocationError>;

    fn invoke_rewriter(
        &self,
        registration: &ContractRegistration,
        request: &ModCollectRequest,
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
        _request: &ModCollectRequest,
    ) -> Result<CollectorOutcome, ContractInvocationError> {
        self.record(InvocationKind::Collector {
            contract_id: registration.contract_id.clone(),
            type_id: registration.type_id.clone(),
            entry_symbol: registration.entry_symbol.clone(),
        });
        Ok(CollectorOutcome { type_id: registration.type_id.clone(), ..Default::default() })
    }

    fn invoke_generator(
        &self,
        registration: &ContractRegistration,
        _request: &ModGenerationRequest,
    ) -> Result<GeneratorOutcome, ContractInvocationError> {
        self.record(InvocationKind::Generator {
            contract_id: registration.contract_id.clone(),
            type_id: registration.type_id.clone(),
            entry_symbol: registration.entry_symbol.clone(),
        });
        Ok(GeneratorOutcome { type_id: registration.type_id.clone(), ..Default::default() })
    }

    fn invoke_analyzer(
        &self,
        registration: &ContractRegistration,
        _request: &ModCollectRequest,
        snapshot: Option<&crate::services::SemanticSnapshot>,
    ) -> Result<AnalyzerOutcome, ContractInvocationError> {
        self.record(InvocationKind::Analyzer {
            contract_id: registration.contract_id.clone(),
            type_id: registration.type_id.clone(),
            entry_symbol: registration.entry_symbol.clone(),
            snapshot_version: snapshot.map(|snap| snap.version),
            snapshot_staged_through: snapshot.map(|snap| snap.staged_through.to_owned()),
        });
        Ok(AnalyzerOutcome { type_id: registration.type_id.clone(), ..Default::default() })
    }

    fn invoke_rewriter(
        &self,
        registration: &ContractRegistration,
        _request: &ModCollectRequest,
    ) -> Result<RewriterOutcome, ContractInvocationError> {
        self.record(InvocationKind::Rewriter {
            contract_id: registration.contract_id.clone(),
            type_id: registration.type_id.clone(),
            entry_symbol: registration.entry_symbol.clone(),
        });
        Ok(RewriterOutcome { type_id: registration.type_id.clone(), ..Default::default() })
    }
}

/// Test-only invoker that lets tests script outcomes per `(contract_id, type_id)`
/// pair. Falls back to [`StubContractInvoker`] behavior when no script is registered.
#[derive(Debug, Default)]
pub struct ScriptedContractInvoker {
    pub collector_narrowed_targets: Mutex<Vec<(String, Vec<String>)>>,
    pub generator_typed_items: Mutex<Vec<(String, Vec<Spanned<ProgramItem>>)>>,
    pub generator_code_outputs: Mutex<Vec<(String, Vec<super::generate_output::CodeGenerateOutput>)>>,
    pub analyzer_diagnostics: Mutex<Vec<(String, Vec<AnalyzerDiagnostic>)>>,
    pub rewriter_edits: Mutex<Vec<(String, Vec<RewriteEdit>)>>,
    pub recorded: StubContractInvoker,
}

impl ScriptedContractInvoker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_collector_narrowed_targets(self, type_id: impl Into<String>, narrowed_targets: Vec<String>) -> Self {
        self.collector_narrowed_targets
            .lock()
            .expect("scripted collector targets")
            .push((type_id.into(), narrowed_targets));
        self
    }

    pub fn with_generator_typed_items(
        self,
        type_id: impl Into<String>,
        typed_items: Vec<Spanned<ProgramItem>>,
    ) -> Self {
        self.generator_typed_items.lock().expect("scripted typed items").push((type_id.into(), typed_items));
        self
    }

    pub fn with_generator_code_outputs(
        self,
        type_id: impl Into<String>,
        code_outputs: Vec<super::generate_output::CodeGenerateOutput>,
    ) -> Self {
        self.generator_code_outputs.lock().expect("scripted code outputs").push((type_id.into(), code_outputs));
        self
    }

    /// Evaluate Beskid-tagged code bodies and register them for generator dispatch tests.
    pub fn with_generator_code_contribution(
        self,
        type_id: impl Into<String>,
        module_path: impl Into<String>,
        file_name: impl Into<String>,
        language: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        let output = super::code_string::code_generate_output(
            &module_path.into(),
            &file_name.into(),
            &language.into(),
            &body.into(),
        )
        .expect("scripted code contribution must evaluate");
        self.with_generator_code_outputs(type_id, vec![output])
    }

    /// Parse canonical source fragments into typed items for tests and transitional callers.
    pub fn with_generator_contribution(self, type_id: impl Into<String>, contributions: Vec<String>) -> Self {
        let typed_items = emit_bridge::materialize_program_items(contributions)
            .expect("scripted generator contribution must parse as typed program items");
        self.with_generator_typed_items(type_id, typed_items)
    }

    pub fn with_analyzer_diagnostic(self, type_id: impl Into<String>, diagnostics: Vec<AnalyzerDiagnostic>) -> Self {
        self.analyzer_diagnostics.lock().expect("scripted diagnostics").push((type_id.into(), diagnostics));
        self
    }

    /// Script the text edits a Rewriter contract returns for `type_id`. The host
    /// applies scripted edits after the rewriter runs, mirroring how
    /// [`ScriptedContractInvoker::with_generator_typed_items`] overlays generator
    /// contributions.
    pub fn with_rewriter_edits(self, type_id: impl Into<String>, edits: Vec<RewriteEdit>) -> Self {
        self.rewriter_edits.lock().expect("scripted rewriter edits").push((type_id.into(), edits));
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
        request: &ModCollectRequest,
    ) -> Result<CollectorOutcome, ContractInvocationError> {
        let mut outcome = self.recorded.invoke_collector(registration, request)?;
        let scripted = self.collector_narrowed_targets.lock().expect("scripted collector targets");
        for (type_id, narrowed_targets) in scripted.iter() {
            if registration.type_id == *type_id {
                outcome.narrowed_targets.extend(narrowed_targets.iter().cloned());
            }
        }
        Ok(outcome)
    }

    fn invoke_generator(
        &self,
        registration: &ContractRegistration,
        request: &ModGenerationRequest,
    ) -> Result<GeneratorOutcome, ContractInvocationError> {
        let mut outcome = self.recorded.invoke_generator(registration, request)?;
        let scripted = self.generator_typed_items.lock().expect("scripted typed items");
        for (type_id, typed_items) in scripted.iter() {
            if registration.type_id == *type_id {
                outcome.typed_items.extend(typed_items.iter().cloned());
            }
        }
        drop(scripted);
        let scripted_code = self.generator_code_outputs.lock().expect("scripted code outputs");
        for (type_id, code_outputs) in scripted_code.iter() {
            if registration.type_id == *type_id {
                outcome.code_outputs.extend(code_outputs.iter().cloned());
            }
        }
        Ok(outcome)
    }

    fn invoke_analyzer(
        &self,
        registration: &ContractRegistration,
        request: &ModCollectRequest,
        snapshot: Option<&crate::services::SemanticSnapshot>,
    ) -> Result<AnalyzerOutcome, ContractInvocationError> {
        let mut outcome = self.recorded.invoke_analyzer(registration, request, snapshot)?;
        let scripted = self.analyzer_diagnostics.lock().expect("scripted diagnostics");
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
        request: &ModCollectRequest,
    ) -> Result<RewriterOutcome, ContractInvocationError> {
        let mut outcome = self.recorded.invoke_rewriter(registration, request)?;
        let scripted = self.rewriter_edits.lock().expect("scripted rewriter edits");
        for (type_id, edits) in scripted.iter() {
            if registration.type_id == *type_id {
                outcome.edits.extend(edits.iter().cloned());
            }
        }
        Ok(outcome)
    }
}

/// MVP native invoker lives in [`super::native`]; re-exported from the mod_host crate root.
#[cfg(test)]
mod tests {
    use super::super::context::ModInvocationContext;
    use super::*;

    fn r(contract: &str, ty: &str, sym: &str) -> ContractRegistration {
        ContractRegistration { contract_id: contract.to_owned(), type_id: ty.to_owned(), entry_symbol: sym.to_owned() }
    }

    fn empty_collect_request() -> ModInvocationContext {
        ModInvocationContext::empty()
    }

    #[test]
    fn stub_records_each_invocation_kind() {
        let invoker = StubContractInvoker::new();
        let mut context = empty_collect_request();
        invoker.invoke_collector(&r("Beskid.Compiler.Collect.Collector", "T1", "c"), &context.collect_request).unwrap();
        invoker
            .invoke_generator(&r("Beskid.Compiler.Collect.Generator", "T2", "g"), &context.generation_request(&[]))
            .unwrap();
        invoker
            .invoke_analyzer(&r("Beskid.Compiler.Collect.Analyzer", "T3", "a"), &context.collect_request, None)
            .unwrap();
        invoker.invoke_rewriter(&r("Beskid.Compiler.Collect.Rewriter", "T4", "r"), &context.collect_request).unwrap();
        let log = invoker.invocations();
        assert_eq!(log.len(), 4);
        assert!(matches!(log[0], InvocationKind::Collector { .. }));
        assert!(matches!(log[1], InvocationKind::Generator { .. }));
        assert!(matches!(log[2], InvocationKind::Analyzer { .. }));
        assert!(matches!(log[3], InvocationKind::Rewriter { .. }));
    }

    #[test]
    fn scripted_overlays_generator_contributions() {
        let invoker = ScriptedContractInvoker::new()
            .with_generator_contribution("T2", vec!["pub fn synthetic_generated() { return; }".into()]);
        let mut context = empty_collect_request();
        let outcome = invoker
            .invoke_generator(&r("Beskid.Compiler.Collect.Generator", "T2", "g"), &context.generation_request(&[]))
            .unwrap();
        assert_eq!(outcome.typed_items.len(), 1);
    }

    #[test]
    fn scripted_overlays_analyzer_diagnostics() {
        let invoker = ScriptedContractInvoker::new().with_analyzer_diagnostic(
            "TA",
            vec![AnalyzerDiagnostic {
                code: "ModA0001".into(),
                message: "test".into(),
                severity: AnalyzerSeverity::Warning,
                span: Some((4, 8)),
            }],
        );
        let context = empty_collect_request();
        let outcome = invoker
            .invoke_analyzer(&r("Beskid.Compiler.Collect.Analyzer", "TA", "a"), &context.collect_request, None)
            .unwrap();
        assert_eq!(outcome.diagnostics.len(), 1);
        assert_eq!(outcome.diagnostics[0].code, "ModA0001");
        assert_eq!(outcome.diagnostics[0].span, Some((4, 8)));
    }

    #[test]
    fn scripted_overlays_rewriter_edits() {
        let invoker = ScriptedContractInvoker::new().with_rewriter_edits(
            "TR",
            vec![
                RewriteEdit::Replace { start: 0, end: 4, text: "UNIT".into() },
                RewriteEdit::Insert { offset: 12, text: " // touched".into() },
            ],
        );
        let context = empty_collect_request();
        let outcome = invoker
            .invoke_rewriter(&r("Beskid.Compiler.Collect.Rewriter", "TR", "r"), &context.collect_request)
            .unwrap();
        assert_eq!(outcome.edits.len(), 2);
        assert!(matches!(outcome.edits[0], RewriteEdit::Replace { .. }));
        assert!(matches!(outcome.edits[1], RewriteEdit::Insert { .. }));
    }

    #[test]
    fn analyzer_diagnostic_defaults_to_warning_severity_and_no_span() {
        let diag = AnalyzerDiagnostic::default();
        assert_eq!(diag.severity, AnalyzerSeverity::Warning);
        assert_eq!(diag.span, None);
        assert!(diag.code.is_empty());
    }
}
