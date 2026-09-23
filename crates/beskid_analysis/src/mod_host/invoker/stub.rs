use std::sync::Mutex;

use beskid_abi::{ModCollectRequest, ModGenerationRequest};

use super::super::types::ContractRegistration;
use super::contract::{ContractInvocationError, ContractInvoker};
use super::outcomes::{AnalyzerOutcome, CollectorOutcome, GeneratorOutcome, RewriterOutcome};

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
