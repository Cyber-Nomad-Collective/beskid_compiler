use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase, phases::SYNTAX_GENERATION};

use crate::syntax::{Program, Spanned};

use super::types::GeneratedSyntax;

pub(crate) fn reparse_if_needed(
    program: Spanned<Program>,
    generated: &GeneratedSyntax,
    _source_name: &str,
    _source: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<Spanned<Program>> {
    if generated.requires_reparse() {
        observe_phase(pipeline, SYNTAX_GENERATION, || {});
    }

    Ok(program)
}
