use anyhow::Result;
use beskid_pipeline::{PipelineObserver, observe_phase, phases::SYNTAX_GENERATION};

use crate::services::parse_program_with_source_name;
use crate::syntax::{Program, Spanned};

use super::types::GeneratedSyntax;

pub(crate) fn reparse_if_needed(
    program: Spanned<Program>,
    generated: &GeneratedSyntax,
    source_name: &str,
    source: &str,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<Spanned<Program>> {
    if generated.has_typed_merge() || !generated.requires_reparse() {
        return Ok(program);
    }

    observe_phase(pipeline, SYNTAX_GENERATION, || {});

    let mut merged_source = source.to_string();
    for contribution in &generated.text_contributions {
        let contribution = contribution.trim();
        if contribution.is_empty() {
            continue;
        }
        if !merged_source.is_empty() && !merged_source.ends_with('\n') {
            merged_source.push('\n');
        }
        merged_source.push_str(contribution);
        if !contribution.ends_with('\n') {
            merged_source.push('\n');
        }
    }

    if generated.text_contributions.is_empty() {
        return Ok(program);
    }

    parse_program_with_source_name(source_name, &merged_source)
        .map_err(|err| anyhow::anyhow!("failed to reparse merged mod-generated syntax: {err}"))
}
