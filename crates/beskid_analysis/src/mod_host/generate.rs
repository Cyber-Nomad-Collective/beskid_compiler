use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MOD_GENERATE};

use super::context::ModInvocationContext;
use super::invoker::{ContractInvoker, GeneratorOutcome};
use super::query_bridge::PipelineOp;
use super::types::{
    CollectedContracts, ContractRegistration, GeneratedSyntax, LoadedModArtifact, ModHostInput,
    ProgramItem,
};

/// Maximum generator rounds allowed across all loaded mod artifacts.
pub(crate) fn resolved_max_generator_rounds(loaded: &[LoadedModArtifact]) -> u32 {
    loaded
        .iter()
        .filter_map(|artifact| artifact.discovered.mod_section.as_ref())
        .map(|section| section.resolved_max_generator_rounds())
        .max()
        .unwrap_or(4)
}

/// Run every Generator / AttributeGenerator contract registered by `mod.load`.
/// Generator outcomes (typed AST contributions) are merged into [`GeneratedSyntax`] for
/// `merge::merge_generated_syntax` and, when only legacy text remains,
/// `reparse::reparse_if_needed`.
pub(crate) fn run_generators(
    loaded: &[LoadedModArtifact],
    collected: &CollectedContracts,
    input: &ModHostInput<'_>,
    invoker: &dyn ContractInvoker,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<GeneratedSyntax> {
    observe_phase_result(pipeline, MOD_GENERATE, || {
        let mut registrations: Vec<ContractRegistration> = collected.registrations.clone();
        let mut outcomes: Vec<GeneratorOutcome> = Vec::new();
        let mut typed_items: Vec<crate::syntax::Spanned<ProgramItem>> = Vec::new();
        let pipeline_ops: Vec<PipelineOp> = Vec::new();
        let mut context = ModInvocationContext::build(input, loaded);
        let target_ids: Vec<String> = collected
            .outcomes
            .iter()
            .flat_map(|outcome| outcome.narrowed_targets.iter().cloned())
            .collect();
        let generation_request = context.generation_request(&target_ids);

        for artifact in loaded {
            for registration in &artifact.registrations {
                if !is_generate_registration(registration) {
                    continue;
                }
                let outcome = invoker
                    .invoke_generator(registration, &generation_request)
                    .map_err(|err| anyhow::anyhow!(err.to_string()))?;
                typed_items.extend(outcome.typed_items.iter().cloned());
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

        Ok(GeneratedSyntax {
            registrations,
            typed_items,
            pipeline_ops,
            text_contributions: Vec::new(),
            outcomes,
        })
    })
}

pub(crate) fn is_generate_registration(registration: &ContractRegistration) -> bool {
    registration.contract_id.ends_with(".Generator")
        || registration.contract_id.ends_with(".AttributeGenerator")
}
