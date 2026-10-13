use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MOD_GENERATE};

use super::context::ModInvocationContext;
use super::invoker::{ContractInvoker, GeneratorOutcome};
use super::query_bridge::PipelineOp;
use super::types::{
    CollectedContracts, ContractRegistration, GeneratedSyntax, LoadedModArtifact, ModHostInput, ProgramItem,
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
/// Each invocation's typed AST contributions pass through a fresh structural arena that
/// bounds them and attaches their origin lineage; the admitted items are merged into
/// [`GeneratedSyntax`] for `merge::merge_generated_syntax`. Generators contribute typed
/// syntax only: there is no source-text contribution path.
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
        let mut provenance: Vec<super::StructuralProvenance> = Vec::new();
        let pipeline_ops: Vec<PipelineOp> = Vec::new();
        let mut context = ModInvocationContext::build(input, loaded);
        let target_ids: Vec<String> =
            collected.outcomes.iter().flat_map(|outcome| outcome.narrowed_targets.iter().cloned()).collect();
        let generation_request = context.generation_request(&target_ids);

        for artifact in loaded {
            for registration in &artifact.registrations {
                if !is_generate_registration(registration) {
                    continue;
                }
                let outcome = invoker
                    .invoke_generator(registration, &generation_request, input.semantic_authority)
                    .map_err(|err| anyhow::anyhow!(err.attributed_to(artifact.package_id()).to_string()))?;
                let outcome = admit_outcome(registration, outcome, input)
                    .map_err(|err| anyhow::anyhow!("{}: {err}", artifact.package_id()))?;
                typed_items.extend(outcome.typed_items.iter().cloned());
                provenance.extend(outcome.provenance.iter().cloned());
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

        Ok(GeneratedSyntax { registrations, typed_items, provenance, pipeline_ops, outcomes })
    })
}

/// Admit one invocation's typed items through the structural arena of its registration and
/// the actual syntax generation the invocation observed. An outcome that names another
/// registration, carries lineage the arena did not issue, or contributes items without actual
/// generation authority is rejected before anything is published.
fn admit_outcome(
    registration: &ContractRegistration,
    mut outcome: GeneratorOutcome,
    input: &ModHostInput<'_>,
) -> Result<GeneratorOutcome> {
    if outcome.type_id != registration.type_id {
        anyhow::bail!(
            "generator outcome for `{}` was returned by registration `{}`",
            outcome.type_id,
            registration.type_id
        );
    }
    if !outcome.provenance.is_empty() {
        anyhow::bail!("generator `{}` supplied lineage the host arena did not issue", registration.type_id);
    }
    if outcome.typed_items.is_empty() {
        return Ok(outcome);
    }
    let generation = input.syntax_generation_id.filter(|generation| generation.0 != 0).ok_or_else(|| {
        anyhow::anyhow!(
            "generator `{}` contributed syntax without actual syntax generation authority",
            registration.type_id
        )
    })?;
    let items = std::mem::take(&mut outcome.typed_items);
    let admitted = super::structural::admit_generated_items(&registration.type_id, generation.0, input.source_name, items)
        .map_err(|err| anyhow::anyhow!("generator `{}` contribution rejected: {err}", registration.type_id))?;
    for item in admitted {
        outcome.typed_items.push(item.item);
        outcome.provenance.push(item.provenance);
    }
    Ok(outcome)
}

pub(crate) fn is_generate_registration(registration: &ContractRegistration) -> bool {
    registration.contract_id.ends_with(".Generator")
        || registration.contract_id.ends_with(".GrammarGenerator")
        || registration.contract_id.ends_with(".AttributeGenerator")
}
