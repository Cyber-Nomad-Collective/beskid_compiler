use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MOD_GENERATE};

use super::types::{CollectedContracts, ContractRegistration, GeneratedSyntax, LoadedModArtifact};

pub(crate) fn run_generators(
    loaded: &[LoadedModArtifact],
    collected: &CollectedContracts,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<GeneratedSyntax> {
    observe_phase_result(pipeline, MOD_GENERATE, || {
        let mut registrations = collected.registrations.clone();
        registrations.extend(
            loaded
                .iter()
                .flat_map(|artifact| artifact.registrations.iter())
                .filter(|registration| is_generate_registration(registration))
                .cloned(),
        );
        registrations.sort_by(|left, right| {
            left.contract_id
                .cmp(&right.contract_id)
                .then_with(|| left.type_id.cmp(&right.type_id))
                .then_with(|| left.entry_symbol.cmp(&right.entry_symbol))
        });
        registrations.dedup();

        Ok(GeneratedSyntax {
            registrations,
            contributions: Vec::new(),
        })
    })
}

fn is_generate_registration(registration: &ContractRegistration) -> bool {
    registration.contract_id.ends_with(".Generator")
        || registration.contract_id.ends_with(".AttributeGenerator")
}
