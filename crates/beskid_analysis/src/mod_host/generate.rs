use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MOD_GENERATE};

use super::invoker::{ContractInvoker, GeneratorOutcome};
use super::types::{CollectedContracts, ContractRegistration, GeneratedSyntax, LoadedModArtifact};

/// Run every Generator / AttributeGenerator contract registered by `mod.load`.
/// Generator outcomes (typed AST contributions, currently expressed as canonical
/// strings) are merged into [`GeneratedSyntax`] for `merge::merge_generated_syntax`
/// and `reparse::reparse_if_needed`.
pub(crate) fn run_generators(
    loaded: &[LoadedModArtifact],
    collected: &CollectedContracts,
    invoker: &dyn ContractInvoker,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<GeneratedSyntax> {
    observe_phase_result(pipeline, MOD_GENERATE, || {
        let mut registrations: Vec<ContractRegistration> = collected.registrations.clone();
        let mut outcomes: Vec<GeneratorOutcome> = Vec::new();
        let mut contributions: Vec<String> = Vec::new();

        for artifact in loaded {
            for registration in &artifact.registrations {
                if !is_generate_registration(registration) {
                    continue;
                }
                let outcome = invoker
                    .invoke_generator(registration)
                    .map_err(|err| anyhow::anyhow!(err.to_string()))?;
                contributions.extend(outcome.contributions.iter().cloned());
                outcomes.push(outcome);
                registrations.push(registration.clone());
            }
        }

        registrations.sort_by(|left, right| {
            left.contract_id
                .cmp(&right.contract_id)
                .then_with(|| left.type_id.cmp(&right.type_id))
                .then_with(|| left.entry_symbol.cmp(&right.entry_symbol))
        });
        registrations.dedup();

        contributions.sort();
        contributions.dedup();

        Ok(GeneratedSyntax {
            registrations,
            contributions,
            outcomes,
        })
    })
}

fn is_generate_registration(registration: &ContractRegistration) -> bool {
    registration.contract_id.ends_with(".Generator")
        || registration.contract_id.ends_with(".AttributeGenerator")
}
