use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MOD_ANALYZE};

use super::invoker::{AnalyzerOutcome, ContractInvoker};
use super::types::{AnalyzedContracts, ContractRegistration, ModHostSession};

/// Run every Analyzer contract registered by `mod.load`. Diagnostics produced by each
/// analyzer are surfaced through [`AnalyzedContracts::outcomes`] so the caller can
/// merge them with host semantic diagnostics; fix targets feed `mod.rewrite`.
pub(crate) fn run_analyzers(
    session: &ModHostSession,
    invoker: &dyn ContractInvoker,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<AnalyzedContracts> {
    observe_phase_result(pipeline, MOD_ANALYZE, || {
        let mut registrations: Vec<ContractRegistration> = Vec::new();
        let mut outcomes: Vec<AnalyzerOutcome> = Vec::new();

        for registration in session.registrations() {
            if !registration.contract_id.ends_with(".Analyzer") {
                continue;
            }
            let outcome = invoker
                .invoke_analyzer(registration)
                .map_err(|err| anyhow::anyhow!(err.to_string()))?;
            outcomes.push(outcome);
            registrations.push(registration.clone());
        }

        Ok(AnalyzedContracts {
            registrations,
            outcomes,
        })
    })
}
