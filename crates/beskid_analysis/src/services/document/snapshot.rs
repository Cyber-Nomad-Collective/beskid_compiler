use std::collections::HashSet;
use std::path::Path;

use crate::doc::DocRefLinkContext;
use crate::projects::assembly::ProgramAssembly;
use crate::resolve::Resolution;
use crate::syntax::{Program, Spanned};

use super::model::DocumentAnalysisSnapshot;

/// Build a documentation snapshot from prepare-spine entry resolution and assembled units.
pub fn build_api_documentation_snapshot(
    program: &Spanned<Program>,
    source_name: impl AsRef<str>,
    source_text: &str,
    path: &Path,
    resolution: Resolution,
    assembly: &ProgramAssembly,
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
        docs_ref_links,
    )
}

fn build_document_snapshot(
    program: &Spanned<Program>,
    source_name: &str,
    source_text: &str,
    path: &Path,
    resolution: Option<Resolution>,
    assembly_module_paths: HashSet<String>,
    assembly: Option<&ProgramAssembly>,
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

    DocumentAnalysisSnapshot {
        program: program.clone(),
        resolution,
        item_docs,
        doc_diagnostics,
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
        docs_ref_links,
    )
}
