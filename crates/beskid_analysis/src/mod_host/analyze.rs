use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase_result, phases::MOD_ANALYZE};

use super::types::{ContractRegistration, ModHostSession};

pub(crate) fn run_analyzers(
    session: &ModHostSession,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<Vec<ContractRegistration>> {
    observe_phase_result(pipeline, MOD_ANALYZE, || {
        Ok(session
            .registrations()
            .filter(|registration| registration.contract_id.ends_with(".Analyzer"))
            .cloned()
            .collect())
    })
}
