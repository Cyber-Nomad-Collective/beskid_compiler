use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MOD_COLLECT};

use super::invoker::{CollectorOutcome, ContractInvoker};
use super::types::{CollectedContracts, ContractRegistration, LoadedModArtifact};

/// Run every Collector contract registered by `mod.load`. Each registration is
/// dispatched through `invoker` once; outcomes (narrowed targets per contract type)
/// are merged back into [`CollectedContracts`] for downstream phases.
pub(crate) fn collect_contracts(
    loaded: &[LoadedModArtifact],
    invoker: &dyn ContractInvoker,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<CollectedContracts> {
    observe_phase_result(pipeline, MOD_COLLECT, || {
        let mut registrations: Vec<ContractRegistration> = Vec::new();
        let mut outcomes: Vec<CollectorOutcome> = Vec::new();

        for artifact in loaded {
            for registration in &artifact.registrations {
                if !is_collect_registration(registration) {
                    continue;
                }
                let outcome = invoker
                    .invoke_collector(registration)
                    .map_err(|err| anyhow::anyhow!(err.to_string()))?;
                registrations.push(registration.clone());
                outcomes.push(outcome);
            }
        }

        Ok(CollectedContracts {
            registrations,
            outcomes,
        })
    })
}

fn is_collect_registration(registration: &ContractRegistration) -> bool {
    registration.contract_id.ends_with(".Collector")
}
