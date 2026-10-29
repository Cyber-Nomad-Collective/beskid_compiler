use anyhow::{Context, Result};
use beskid_analysis::doc::DocRefLinkContext;

use beskid_analysis::projects::assembly::ProgramAssembly;
use beskid_analysis::resolve::ItemInfo;
use beskid_analysis::services;
use beskid_analysis::syntax::SpanInfo;

use super::model::LocationJson;

fn location_for_span(_source: &str, file: &str, span: &SpanInfo) -> LocationJson {
    let (sl, sc) = span.line_col_start;
    let (el, ec) = span.line_col_end;
    LocationJson { file: file.to_string(), start_line: sl, start_column: sc, end_line: el, end_column: ec }
}

pub(super) fn location_for_byte_range(source: &str, file: &str, start: usize, end: usize) -> LocationJson {
    let span = SpanInfo::from_byte_range_in_source(source, start, end);
    location_for_span(source, file, &span)
}

pub(super) fn location_for_item(
    item: &ItemInfo,
    assembly: &ProgramAssembly,
    entry_source: &str,
    entry_path: &str,
) -> LocationJson {
    if let Some(path) = &item.source_path
        && let Some(unit) = assembly.units.iter().find(|u| u.path == *path)
    {
        return location_for_span(&unit.source, &path.to_string_lossy(), &item.span);
    }
    location_for_span(entry_source, entry_path, &item.span)
}

pub(super) fn build_doc_snapshot(
    resolved: &services::ResolvedInput,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    docs_ref: Option<&DocRefLinkContext>,
) -> Result<(services::DocumentAnalysisSnapshot, ProgramAssembly)> {
    use beskid_queries::{BeskidDatabase, api_documentation_snapshot, configure_db_for_project};

    let plan = resolved
        .compile_plan
        .as_ref()
        .with_context(|| format!("documentation for {} requires a compile plan", resolved.source_path.display()))?;
    configure_db_for_project(&plan.project_root);
    let mut db = BeskidDatabase::with_persistence(&plan.project_root);
    api_documentation_snapshot(&mut db, resolved, program, docs_ref)
}
