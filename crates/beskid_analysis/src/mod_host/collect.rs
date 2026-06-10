use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MOD_COLLECT};

use super::context::ModInvocationContext;
use super::invoker::{CollectorOutcome, ContractInvoker};
use super::types::{
    CollectedContracts, ContractRegistration, LoadedModArtifact, ModHostInput,
};

/// Stable fingerprint of collector-observed targets across all registrations.
///
/// Sorted, deduplicated target ids from every `Collector` outcome. A change in
/// this fingerprint (target added, removed, or identity delta) invalidates
/// `mod.generate` and optional disk materialization.
pub fn capture_target_fingerprint(outcomes: &[CollectorOutcome]) -> String {
    let mut tokens: Vec<String> = outcomes
        .iter()
        .flat_map(|outcome| outcome.narrowed_targets.iter().cloned())
        .collect();
    tokens.sort();
    tokens.dedup();
    tokens.join("|")
}

/// Returns `true` when generator execution should run because targets changed.
pub fn targets_changed(previous: Option<&str>, current: &str) -> bool {
    previous != Some(current)
}

/// Run every Collector contract registered by `mod.load`. Each registration is
/// dispatched through `invoker` once; outcomes (narrowed targets per contract type)
/// are merged back into [`CollectedContracts`] for downstream phases.
pub(crate) fn collect_contracts(
    loaded: &[LoadedModArtifact],
    input: &ModHostInput<'_>,
    invoker: &dyn ContractInvoker,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<CollectedContracts> {
    observe_phase_result(pipeline, MOD_COLLECT, || {
        let mut registrations: Vec<ContractRegistration> = Vec::new();
        let mut outcomes: Vec<CollectorOutcome> = Vec::new();
        let context = ModInvocationContext::build(input, loaded);

        for artifact in loaded {
            for registration in &artifact.registrations {
                if !is_collect_registration(registration) {
                    continue;
                }
                let outcome = invoker
                    .invoke_collector(registration, &context.collect_request)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_target_fingerprint_is_order_independent() {
        let left = capture_target_fingerprint(&[
            CollectorOutcome {
                type_id: "A".into(),
                narrowed_targets: vec!["t2".into(), "t1".into()],
            },
            CollectorOutcome {
                type_id: "B".into(),
                narrowed_targets: vec!["t1".into()],
            },
        ]);
        let right = capture_target_fingerprint(&[
            CollectorOutcome {
                type_id: "B".into(),
                narrowed_targets: vec!["t1".into()],
            },
            CollectorOutcome {
                type_id: "A".into(),
                narrowed_targets: vec!["t1".into(), "t2".into()],
            },
        ]);
        assert_eq!(left, "t1|t2");
        assert_eq!(left, right);
    }

    #[test]
    fn targets_changed_detects_add_remove_and_reorder() {
        assert!(!targets_changed(Some("a|b"), "a|b"));
        assert!(targets_changed(Some("a|b"), "a|b|c"));
        assert!(targets_changed(Some("a|b|c"), "a|b"));
        assert!(targets_changed(None, "a"));
    }
}
