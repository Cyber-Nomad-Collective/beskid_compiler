//! Doc-comment diagnostics, doc markdown and hover through the query-backed documentation
//! snapshot that `beskid doc` uses.

use beskid_analysis::doc::DocRefLinkContext;
use beskid_analysis::projects::{CompilePlan, Target, TargetKind};
use beskid_analysis::services::{
    DocumentAnalysisSnapshot, ResolvedInput, hover_at_offset, parse_program_with_source_name,
};
use beskid_queries::{BeskidDatabase, api_documentation_snapshot};

/// Build the documentation snapshot of a one-file library project with `source` as its entry.
fn documentation_snapshot(source: &str, docs_ref_links: Option<&DocRefLinkContext>) -> DocumentAnalysisSnapshot {
    let directory = tempfile::tempdir().expect("create documentation project");
    let root = directory.path().canonicalize().expect("canonical project root");
    let src = root.join("src");
    std::fs::create_dir(&src).expect("create source root");
    let path = src.join("Main.bd");
    std::fs::write(&path, source).expect("write entry source");
    let plan = CompilePlan {
        project_root: root.clone(),
        manifest_path: root.join("Docs.bproj"),
        project_name: "Docs".into(),
        source_root: src,
        target: Target { name: "Docs".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: vec![],
        unresolved_dependencies: vec![],
        has_core_dependency: false,
    };
    let resolved = ResolvedInput {
        source_path: path.clone(),
        source: source.into(),
        compile_plan: Some(plan),
        prepared_workspace: None,
        workspace_summary: None,
        assembly: None,
    };
    let program = parse_program_with_source_name(&path.display().to_string(), source).expect("parse entry source");
    let mut db = BeskidDatabase::default();
    let (snapshot, _) =
        api_documentation_snapshot(&mut db, &resolved, &program, docs_ref_links).expect("documentation snapshot");
    assert!(snapshot.resolution.is_some(), "the query-backed snapshot resolves the entry program");
    snapshot
}

fn assert_doc_diagnostic(source: &str, code: &str) {
    let snapshot = documentation_snapshot(source, None);
    assert!(
        snapshot.doc_diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some(code)),
        "expected {code}, got {:?}",
        snapshot.doc_diagnostics
    );
}

#[test]
fn doc_diagnostics_flag_an_unknown_variant_name() {
    assert_doc_diagnostic("/// @variant(Ghost) nope\nenum Color {\n    Red,\n    Blue,\n}\n", "W1621");
}

#[test]
fn doc_diagnostics_flag_a_variant_tag_on_a_type() {
    assert_doc_diagnostic("/// @variant(x) bad\ntype Point { i64 x; }\n", "W1620");
}

#[test]
fn doc_diagnostics_flag_a_par_tag_on_a_function_without_generics() {
    assert_doc_diagnostic("/// @par(T) bad\nunit Main() { return; }\n", "W1623");
}

#[test]
fn doc_diagnostics_flag_an_unknown_arg_name() {
    assert_doc_diagnostic(
        "/// @arg(nope) bad\ni64 Sum(\n    i64 left,\n    i64 right\n) { return left + right; }\n",
        "W1610",
    );
}

#[test]
fn hover_shows_the_item_doc_markdown() {
    let source = "/// Hello **doc**\nunit Main() { return; }\n";
    let snapshot = documentation_snapshot(source, None);
    let hover = hover_at_offset(&snapshot, source.find("Main").expect("Main")).expect("hover on Main");
    assert!(hover.markdown.contains("Hello **doc**"), "hover markdown: {:?}", hover.markdown);
}

#[test]
fn hover_shows_the_member_doc_markdown() {
    let source = "type User {\n    /// Display name of the user.\n    string name;\n}\n";
    let snapshot = documentation_snapshot(source, None);
    let hover = hover_at_offset(&snapshot, source.rfind("name;").expect("field name")).expect("hover on field");
    assert!(hover.markdown.contains("Display name of the user."), "hover markdown: {:?}", hover.markdown);
}

#[test]
fn resolved_ref_links_to_the_package_docs_route() {
    let source = "/// See @ref(Main) for details.\nunit Other() { return; }\nunit Main() { return; }\n";
    let context = DocRefLinkContext {
        package_with_version: "demo-pkg@1.0.0".into(),
        publishing_package: Some("demo-pkg".into()),
        dependency_roots: vec![],
    };
    let snapshot = documentation_snapshot(source, Some(&context));
    let markdown = snapshot
        .item_docs
        .iter()
        .filter_map(|slot| slot.as_ref().map(|doc| doc.markdown.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(markdown.contains("/docs/demo-pkg%401.0.0/api/"), "expected a package docs link, got {markdown:?}");
    assert!(markdown.contains("](") && markdown.contains("Main"), "expected a markdown link to Main, got {markdown:?}");
}
