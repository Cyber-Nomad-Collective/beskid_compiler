use std::sync::Mutex;

use beskid_abi::{ModCollectRequest, ModGenerationRequest};

use crate::syntax::Spanned;

use super::super::emit_bridge;
use super::super::types::{ContractRegistration, ProgramItem};
use super::contract::{ContractInvocationError, ContractInvoker};
use super::outcomes::{AnalyzerDiagnostic, AnalyzerFix, AnalyzerOutcome, CollectorOutcome, GeneratorOutcome, RewriteEdit, RewriterOutcome};
use super::stub::StubContractInvoker;

/// Test-only invoker that lets tests script outcomes per `(contract_id, type_id)`
/// pair. Falls back to [`StubContractInvoker`] behavior when no script is registered.
#[derive(Debug, Default)]
pub struct ScriptedContractInvoker {
    pub collector_narrowed_targets: Mutex<Vec<(String, Vec<String>)>>,
    pub generator_typed_items: Mutex<Vec<(String, Vec<Spanned<ProgramItem>>)>>,
    pub generator_code_outputs: Mutex<Vec<(String, Vec<super::super::generate_output::CodeGenerateOutput>)>>,
    pub analyzer_diagnostics: Mutex<Vec<(String, Vec<AnalyzerDiagnostic>)>>,
    pub analyzer_fixes: Mutex<Vec<(String, Vec<AnalyzerFix>)>>,
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
        code_outputs: Vec<super::super::generate_output::CodeGenerateOutput>,
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
        let output = super::super::code_string::code_generate_output(
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

    /// Script the quick-fixes a `Analyzer` contract returns for `type_id`. Each fix carries
    /// a `diagnostic_index` into the scripted diagnostics for the same `type_id`; the host
    /// bounds-checks the index against the outcome's diagnostics length (fail-closed).
    pub fn with_analyzer_fix(self, type_id: impl Into<String>, fixes: Vec<AnalyzerFix>) -> Self {
        self.analyzer_fixes.lock().expect("scripted analyzer fixes").push((type_id.into(), fixes));
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

    pub fn invocations(&self) -> Vec<super::stub::InvocationKind> {
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
        drop(scripted);
        let scripted_fixes = self.analyzer_fixes.lock().expect("scripted analyzer fixes");
        for (type_id, fixes) in scripted_fixes.iter() {
            if registration.type_id == *type_id {
                outcome.fixes.extend(fixes.iter().cloned());
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
