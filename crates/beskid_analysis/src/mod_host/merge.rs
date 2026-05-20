use anyhow::Result;

use crate::syntax::{Program, Spanned};

use super::types::GeneratedSyntax;

pub(crate) fn merge_generated_syntax(
    program: Spanned<Program>,
    generated: &GeneratedSyntax,
) -> Result<Spanned<Program>> {
    let mut contributions = generated.contributions.clone();
    contributions.sort();
    contributions.dedup();

    // MVP: descriptors are processed and sorted, but typed syntax contributions are not invoked yet.
    Ok(program)
}
