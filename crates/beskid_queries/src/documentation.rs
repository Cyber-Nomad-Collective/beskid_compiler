//! Documentation snapshots (`beskid doc`, `api.json`) through the query database.

use anyhow::{Result, anyhow};
use beskid_analysis::doc::DocRefLinkContext;
use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::services::{
    DocumentAnalysisSnapshot, PrepareOptions, ResolvedInput, build_api_documentation_snapshot,
};
use beskid_analysis::syntax::{Program, Spanned};

use crate::db::BeskidDatabase;
use crate::entry::assemble_resolved_input_with_db;

/// Assemble the entry import closure through the query database, resolve the entry program, and
/// build its documentation snapshot: item doc markdown and doc-comment diagnostics.
///
/// Fails when the input has no compile plan; there is no single-file documentation path.
pub fn api_documentation_snapshot(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    program: &Spanned<Program>,
    docs_ref_links: Option<&DocRefLinkContext>,
) -> Result<(DocumentAnalysisSnapshot, ProgramAssembly)> {
    if resolved.compile_plan.is_none() {
        return Err(anyhow!("documentation for {} requires a compile plan", resolved.source_path.display()));
    }
    let assembled = assemble_resolved_input_with_db(db, resolved, &PrepareOptions::without_mods())?;
    let assembly = assembled.assembly.ok_or_else(|| anyhow!("documentation requires an assembled program"))?;
    let resolution = assembly
        .module_index
        .resolve_entry_program(&assembly.entry_unit().program, Some(assembled.source_path.as_path()), &assembly)
        .map_err(|err| anyhow!("entry resolution for api.json: {err:?}"))?;
    let snapshot = build_api_documentation_snapshot(
        program,
        resolved.source_path.display().to_string(),
        &resolved.source,
        &resolved.source_path,
        resolution,
        &assembly,
        docs_ref_links,
    );
    Ok((snapshot, assembly))
}
