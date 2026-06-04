use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MOD_ANALYZE};

use super::invoker::{AnalyzerOutcome, ContractInvocationError, ContractInvoker};
use super::types::{AnalyzedContracts, ModHostSession};
use crate::services::SemanticSnapshot;

/// Run every Analyzer contract registered by `mod.load`. Diagnostics produced by each
/// analyzer are surfaced through [`AnalyzedContracts::outcomes`] so the caller can
/// merge them with host semantic diagnostics; fix targets feed `mod.rewrite`.
pub(crate) fn run_analyzers(
    session: &ModHostSession,
    invoker: &dyn ContractInvoker,
    snapshot: Option<&SemanticSnapshot>,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<AnalyzedContracts> {
    observe_phase_result(pipeline, MOD_ANALYZE, || {
        let mut outcomes: Vec<AnalyzerOutcome> = Vec::new();

        for registration in session.registrations() {
            if !registration.contract_id.ends_with(".Analyzer") {
                continue;
            }
            ensure_snapshot_for_analyzer(snapshot, registration)?;
            let outcome = invoker
                .invoke_analyzer(registration, snapshot)
                .map_err(|err| anyhow::anyhow!(err.to_string()))?;
            outcomes.push(outcome);
        }

        Ok(AnalyzedContracts { outcomes })
    })
}

fn ensure_snapshot_for_analyzer(
    snapshot: Option<&SemanticSnapshot>,
    registration: &super::types::ContractRegistration,
) -> Result<(), ContractInvocationError> {
    let Some(snapshot) = snapshot else {
        return Err(ContractInvocationError {
            package_id: registration.type_id.clone(),
            contract_id: registration.contract_id.clone(),
            type_id: registration.type_id.clone(),
            message: "semantic snapshot not available for Analyzer (requires composition stage)"
                .to_owned(),
        });
    };
    if !snapshot.satisfies_minimum("composition") {
        return Err(ContractInvocationError {
            package_id: registration.type_id.clone(),
            contract_id: registration.contract_id.clone(),
            type_id: registration.type_id.clone(),
            message: format!(
                "semantic snapshot staged through `{}` does not satisfy minimum `composition`",
                snapshot.staged_through
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_host::types::ContractRegistration;
    use crate::services::SemanticSnapshot;

    fn analyzer_registration() -> ContractRegistration {
        ContractRegistration {
            contract_id: "Beskid.Compiler.Collect.Analyzer".to_owned(),
            type_id: "Test.Analyzer".to_owned(),
            entry_symbol: "analyze".to_owned(),
        }
    }

    #[test]
    fn analyzer_requires_composition_stage_snapshot() {
        let registration = analyzer_registration();
        let semantic_only = SemanticSnapshot::from_diagnostics(&[], 1, "semantic");
        let err = ensure_snapshot_for_analyzer(Some(&semantic_only), &registration)
            .expect_err("semantic-only snapshot");
        assert!(err.message.contains("composition"));

        let composition = semantic_only.with_composition(&crate::composition::CompositionSnapshot::default());
        ensure_snapshot_for_analyzer(Some(&composition), &registration).expect("composition snapshot");
    }

    #[test]
    fn analyzer_errors_when_snapshot_missing() {
        let registration = analyzer_registration();
        let err =
            ensure_snapshot_for_analyzer(None, &registration).expect_err("missing snapshot");
        assert!(err.message.contains("not available"));
    }
}
