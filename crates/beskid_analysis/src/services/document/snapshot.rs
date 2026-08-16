use std::collections::HashSet;
use std::path::Path;

use crate::compilation_context::ProjectSessionHandle;
use crate::doc::DocRefLinkContext;
use crate::projects::assembly::{AssemblyError, ProgramAssembly};
use crate::projects::{
    AssemblyDiscovery, CompilePlan, PreparedProjectWorkspace, assemble_program, assembly_options_for_prepare,
};
use crate::resolve::Resolution;
use crate::syntax::{Program, Spanned};

use super::super::composition;
use super::model::DocumentAnalysisSnapshot;

/// Assemble the entry import closure for `api.json` (same discovery as prepare / `beskid build`).
pub fn assemble_for_api_documentation(
    plan: &CompilePlan,
    workspace: Option<&PreparedProjectWorkspace>,
    entry_path: &Path,
    entry_source: Option<&str>,
) -> Result<ProgramAssembly, AssemblyError> {
    let mut options = assembly_options_for_prepare(plan, AssemblyDiscovery::ImportClosure);
    options.skip_parse_errors = true;
    assemble_program(plan, workspace, entry_path, entry_source, &options, None)
}

/// Build a documentation snapshot from prepare-spine entry resolution and assembled units.
pub fn build_api_documentation_snapshot(
    program: &Spanned<Program>,
    source_name: impl AsRef<str>,
    source_text: &str,
    path: &Path,
    resolution: Resolution,
    assembly: &ProgramAssembly,
    compile_plan: &CompilePlan,
    docs_ref_links: Option<&DocRefLinkContext>,
) -> DocumentAnalysisSnapshot {
    let module_paths = assembly.module_index.known_module_path_strings();
    build_document_snapshot(
        program,
        source_name.as_ref(),
        source_text,
        path,
        Some(resolution),
        module_paths,
        Some(assembly),
        Some(compile_plan),
        docs_ref_links,
    )
}

/// Full-project resolution for `api.json`: prefetch symbols from every unit, resolve entry, then merge type/value tables from each unit.
pub fn resolve_assembly_for_api_documentation(assembly: &ProgramAssembly, _entry_path: &Path) -> Option<Resolution> {
    assembly.module_index.resolve_for_api_documentation(&assembly.entry_unit().program, assembly)
}

fn build_document_snapshot(
    program: &Spanned<Program>,
    source_name: &str,
    source_text: &str,
    path: &Path,
    resolution: Option<Resolution>,
    assembly_module_paths: HashSet<String>,
    assembly: Option<&ProgramAssembly>,
    compile_plan: Option<&CompilePlan>,
    docs_ref_links: Option<&DocRefLinkContext>,
) -> DocumentAnalysisSnapshot {
    let item_docs = if let Some(res) = resolution.as_ref() {
        if let Some(asm) = assembly {
            let programs: Vec<(&Path, &Program)> =
                asm.units.iter().map(|unit| (unit.path.as_path(), &unit.program.node)).collect();
            crate::doc::build_item_docs_for_resolution(res, &programs, docs_ref_links)
        } else {
            crate::doc::build_item_docs_markdown(&program.node, res, docs_ref_links)
        }
    } else {
        Vec::new()
    };

    let doc_diagnostics = resolution
        .as_ref()
        .map(|r| crate::doc::collect_doc_diagnostics(&program.node, r, source_name, source_text))
        .unwrap_or_default();
    let composition_diagnostics =
        composition::composition_diagnostics_for_program(program, compile_plan, source_name, source_text)
            .unwrap_or_default();

    DocumentAnalysisSnapshot {
        program: program.clone(),
        resolution,
        item_docs,
        doc_diagnostics,
        composition_diagnostics,
        source_path: path.to_path_buf(),
        assembly_module_paths,
    }
}

/// Build an IDE snapshot from entry resolution produced by the prepare spine
/// (for example [`beskid_queries::entry_resolution_with_db`]).
pub fn build_document_analysis_from_resolution(
    program: &Spanned<Program>,
    source_name: impl AsRef<str>,
    source_text: &str,
    path: &Path,
    resolution: Option<Resolution>,
    assembly_module_paths: HashSet<String>,
    compile_plan: Option<&CompilePlan>,
    docs_ref_links: Option<&DocRefLinkContext>,
) -> DocumentAnalysisSnapshot {
    build_document_snapshot(
        program,
        source_name.as_ref(),
        source_text,
        path,
        resolution,
        assembly_module_paths,
        None,
        compile_plan,
        docs_ref_links,
    )
}

pub fn build_document_analysis(
    program: &Spanned<Program>,
    source_name: impl AsRef<str>,
    source_text: &str,
    docs_ref_links: Option<&DocRefLinkContext>,
) -> DocumentAnalysisSnapshot {
    let source_path = Path::new(source_name.as_ref());
    build_document_analysis_for_resolved(program, source_name.as_ref(), source_text, source_path, None, docs_ref_links)
}

/// Like [`build_document_analysis`], with optional [`ProgramAssembly`] for multi-unit docs and resolution.
pub fn build_document_analysis_for_resolved(
    program: &Spanned<Program>,
    source_name: impl AsRef<str>,
    source_text: &str,
    path: &Path,
    assembly: Option<&ProgramAssembly>,
    docs_ref_links: Option<&DocRefLinkContext>,
) -> DocumentAnalysisSnapshot {
    let (resolution, assembly_module_paths) = assembly
        .and_then(|asm| {
            resolve_assembly_for_api_documentation(asm, path)
                .map(|resolution| (Some(resolution), asm.module_index.known_module_path_strings()))
        })
        .unwrap_or((None, HashSet::new()));

    build_document_snapshot(
        program,
        source_name.as_ref(),
        source_text,
        path,
        resolution,
        assembly_module_paths,
        assembly,
        None,
        docs_ref_links,
    )
}

/// Build an IDE snapshot using project session metadata (composition diagnostics only).
///
/// For entry resolution and multi-unit docs, callers must use
/// [`beskid_queries::entry_resolution_with_db`] (or the prepare spine) and then
/// [`build_document_analysis_from_resolution`].
pub fn build_document_analysis_with_context(
    program: &Spanned<Program>,
    source_name: impl AsRef<str>,
    source_text: &str,
    path: &Path,
    ctx: Option<&ProjectSessionHandle>,
    docs_ref_links: Option<&DocRefLinkContext>,
) -> DocumentAnalysisSnapshot {
    let compile_plan = ctx.and_then(|handle| handle.compile_plan.as_ref());

    build_document_snapshot(
        program,
        source_name.as_ref(),
        source_text,
        path,
        None,
        HashSet::new(),
        None,
        compile_plan,
        docs_ref_links,
    )
}
