use anyhow::{Context, Result};
use beskid_analysis::doc::DocRefLinkContext;

use beskid_analysis::projects::assembly::ProgramAssembly;
use beskid_analysis::projects::assembly_options_for_prepare;
use beskid_analysis::resolve::ItemInfo;
use beskid_analysis::services::{self, PrepareOptions};
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
    assembly: Option<&ProgramAssembly>,
    entry_source: &str,
    entry_path: &str,
) -> LocationJson {
    if let Some(asm) = assembly
        && let Some(path) = &item.source_path
        && let Some(unit) = asm.units.iter().find(|u| u.path == *path)
    {
        return location_for_span(&unit.source, &path.to_string_lossy(), &item.span);
    }
    location_for_span(entry_source, entry_path, &item.span)
}

pub(super) fn build_doc_snapshot(
    resolved: &services::ResolvedInput,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    docs_ref: Option<&DocRefLinkContext>,
) -> Result<(services::DocumentAnalysisSnapshot, Option<ProgramAssembly>)> {
    let spanned = program;
    let source_name = resolved.source_path.display().to_string();

    if let Some(plan) = resolved.compile_plan.as_ref() {
        use beskid_queries::{BeskidDatabase, configure_db_for_project, entry_resolution_with_db};

        configure_db_for_project(&plan.project_root);
        let mut db = BeskidDatabase::with_persistence(&plan.project_root);
        let options = PrepareOptions::default();
        let shared = entry_resolution_with_db(&mut db, resolved, &options).context("entry resolution for api.json")?;
        let resolution = (*shared).clone();

        let assembly_options = assembly_options_for_prepare(plan, options.front_end.assembly_discovery);
        let assembly = beskid_queries::program_assembly(
            &mut db,
            plan,
            resolved.prepared_workspace.as_ref(),
            &resolved.source_path,
            Some(&resolved.source),
            &assembly_options,
        )
        .map_err(|err| anyhow::anyhow!("{err}"))?;

        let snap = services::build_api_documentation_snapshot(
            spanned,
            &source_name,
            &resolved.source,
            &resolved.source_path,
            resolution,
            &assembly,
            plan,
            docs_ref,
        );
        return Ok((snap, Some(assembly)));
    }

    let snap = services::build_document_analysis(spanned, &source_name, &resolved.source, docs_ref);
    Ok((snap, None))
}
