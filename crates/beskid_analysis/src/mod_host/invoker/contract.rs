use beskid_abi::{ModCollectRequest, ModGenerationRequest};

use super::super::types::ContractRegistration;
use super::outcomes::{AnalyzerOutcome, CollectorOutcome, GeneratorOutcome, RewriterOutcome};

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
