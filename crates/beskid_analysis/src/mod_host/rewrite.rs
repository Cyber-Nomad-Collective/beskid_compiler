use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MOD_REWRITE};

use crate::syntax::{Program, Spanned};

use super::invoker::{ContractInvoker, RewriterOutcome};
use super::types::{AnalyzedContracts, ContractRegistration, ModHostSession, RewriteResult};

/// Run every Rewriter contract registered by `mod.load`, scheduled by analyzer fixes.
/// Each invocation is dispatched through `invoker`; the returned `RewriteResult`
/// records how many rewrites were applied per contract type so engine and tooling
/// have a per-mod scoreboard for the pipeline.
pub(crate) fn run_rewriters(
    program: Spanned<Program>,
    session: &ModHostSession,
    analyzed: &AnalyzedContracts,
    invoker: &dyn ContractInvoker,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<RewriteResult> {
    observe_phase_result(pipeline, MOD_REWRITE, || {
        let mut registrations: Vec<ContractRegistration> = session
            .registrations()
            .filter(|registration| is_rewrite_registration(registration))
            .cloned()
            .collect();
        registrations.sort_by(|left, right| {
            left.contract_id
                .cmp(&right.contract_id)
                .then_with(|| left.type_id.cmp(&right.type_id))
                .then_with(|| left.entry_symbol.cmp(&right.entry_symbol))
        });

        let mut outcomes: Vec<RewriterOutcome> = Vec::with_capacity(registrations.len());
        for registration in &registrations {
            let outcome = invoker
                .invoke_rewriter(registration)
                .map_err(|err| anyhow::anyhow!(err.to_string()))?;
            outcomes.push(outcome);
        }

        // `analyzed` is preserved for future host-side fix dispatch; today the host
        // calls every Rewriter once and expects deterministic completion.
        let _analyzer_count = analyzed.outcomes.len();

        Ok(RewriteResult {
            program,
            registrations,
            outcomes,
        })
    })
}

fn is_rewrite_registration(registration: &ContractRegistration) -> bool {
    registration.contract_id.ends_with(".Rewriter")
}
