use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MOD_COLLECT};

use super::types::{CollectedContracts, ContractRegistration, LoadedModArtifact};

pub(crate) fn collect_contracts(
    loaded: &[LoadedModArtifact],
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<CollectedContracts> {
    observe_phase_result(pipeline, MOD_COLLECT, || {
        let registrations = loaded
            .iter()
            .flat_map(|artifact| artifact.registrations.iter())
            .filter(|registration| is_collect_registration(registration))
            .cloned()
            .collect();

        Ok(CollectedContracts { registrations })
    })
}

fn is_collect_registration(registration: &ContractRegistration) -> bool {
    registration.contract_id.ends_with(".Collector")
}
