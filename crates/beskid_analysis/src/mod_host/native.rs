//! Unqualified native dispatch cannot obtain a callable from a filesystem path.
//! Executable dispatch is injected by the AOT producer that owns its private witness.
use super::{
    ContractRegistration,
    context::{ModCollectRequest, ModGenerationRequest},
    invoker::{
        AnalyzerOutcome, CollectorOutcome, ContractInvocationError, ContractInvoker, GeneratorOutcome, RewriterOutcome,
    },
};
#[derive(Debug, Default)]
pub struct NativeContractInvoker;
impl NativeContractInvoker {
    /// Explicit absence of producer-issued native execution authority.
    pub fn unavailable() -> Self {
        Self
    }
    fn deny(&self, r: &ContractRegistration) -> ContractInvocationError {
        ContractInvocationError {
            package_id: String::new(),
            contract_id: r.contract_id.clone(),
            type_id: r.type_id.clone(),
            message: "required native Mod has no producer-issued executable authority".into(),
        }
    }
}
impl ContractInvoker for NativeContractInvoker {
    fn invoke_collector(
        &self,
        r: &ContractRegistration,
        _: &ModCollectRequest,
        _: Option<&dyn super::ModSemanticAuthority>,
    ) -> Result<CollectorOutcome, ContractInvocationError> {
        Err(self.deny(r))
    }
    fn invoke_generator(
        &self,
        r: &ContractRegistration,
        _: &ModGenerationRequest,
        _: Option<&dyn super::ModSemanticAuthority>,
    ) -> Result<GeneratorOutcome, ContractInvocationError> {
        Err(self.deny(r))
    }
    fn invoke_analyzer(
        &self,
        r: &ContractRegistration,
        _: &ModCollectRequest,
        _: Option<&crate::services::SemanticSnapshot>,
        _: Option<&dyn super::ModSemanticAuthority>,
    ) -> Result<AnalyzerOutcome, ContractInvocationError> {
        Err(self.deny(r))
    }
    fn invoke_rewriter(
        &self,
        r: &ContractRegistration,
        _: &ModCollectRequest,
        _: Option<&dyn super::ModSemanticAuthority>,
    ) -> Result<RewriterOutcome, ContractInvocationError> {
        Err(self.deny(r))
    }
}
