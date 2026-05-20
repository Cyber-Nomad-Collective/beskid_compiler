use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MOD_REWRITE};

use crate::syntax::{Program, Spanned};

use super::types::{ContractRegistration, ModHostSession};

pub(crate) fn run_rewriters(
    program: Spanned<Program>,
    session: &ModHostSession,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<Spanned<Program>> {
    observe_phase_result(pipeline, MOD_REWRITE, || {
        let mut registrations = session
            .registrations()
            .filter(|registration| is_rewrite_registration(registration))
            .cloned()
            .collect::<Vec<_>>();
        registrations.sort_by(|left, right| {
            left.contract_id
                .cmp(&right.contract_id)
                .then_with(|| left.type_id.cmp(&right.type_id))
                .then_with(|| left.entry_symbol.cmp(&right.entry_symbol))
        });

        Ok(program)
    })
}

fn is_rewrite_registration(registration: &ContractRegistration) -> bool {
    registration.contract_id.ends_with(".Rewriter")
}
