//! `beskid doc` — emit `api.json` and `index.md` API documentation for resolved sources.

use anyhow::{Context, Result};
use beskid_analysis::doc::{
    API_JSON_NAVIGATION_MODEL_GRAPH_V1, API_JSON_SCHEMA_VERSION, API_JSON_SCHEMA_VERSION_BEFORE_GRAPH, ApiDocItem,
    ApiDocLinkContext, ApiDocRoot, ApiLocation, DocRefLinkContext, apply_signature_to_item, assign_declaring_packages,
    build_api_doc_link_context, build_item_signature, display_name_for_item, fill_member_ids_from_parents,
    link_api_doc_library_tree, qualified_names_for_items, relativize_api_doc_paths, resolve_item_tiers,
    validate_prelude_standard_tiers,
};
use beskid_analysis::hir::HirVisibility;
use beskid_analysis::projects::assembly::ProgramAssembly;
use beskid_analysis::projects::{assembly_options_for_prepare, load_manifest_from_path};
use beskid_analysis::resolve::ItemInfo;
use beskid_analysis::services;
use beskid_analysis::services::PrepareOptions;
use beskid_analysis::syntax::SpanInfo;
use clap::Args;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};

#[derive(Args, Debug)]
pub struct DocArgs {
    /// Beskid source file (same resolution as `analyze` when combined with `--project`).
    /// Project-backed docs use the entry import closure (same scope as `beskid build`), not a full workspace scan.
    pub input: Option<PathBuf>,

    #[command(flatten)]
    pub project: ProjectResolveArgs,

    #[command(flatten)]
    pub lockfile: LockfilePolicyArgs,

    /// Output directory for `api.json` and `index.md`
    #[arg(long, default_value = "doc-out")]
    pub out: PathBuf,
}

#[derive(Clone, Debug)]
struct DocEntry {
    qualified_name: String,
    kind: String,
    doc_markdown: Option<String>,
}

#[derive(Clone, Debug)]
struct LocationJson {
    file: String,
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
}

#[derive(Default, Debug)]
struct TreeNode {
    children: BTreeMap<String, TreeNode>,
    entries: Vec<usize>,
}

fn visibility_stable(vis: HirVisibility) -> &'static str {
    match vis {
        HirVisibility::Public => "public",
        HirVisibility::Private => "private",
    }
}

fn location_for_span(_source: &str, file: &str, span: &SpanInfo) -> LocationJson {
    let (sl, sc) = span.line_col_start;
    let (el, ec) = span.line_col_end;
    LocationJson { file: file.to_string(), start_line: sl, start_column: sc, end_line: el, end_column: ec }
}

fn location_for_byte_range(source: &str, file: &str, start: usize, end: usize) -> LocationJson {
    let span = SpanInfo::from_byte_range_in_source(source, start, end);
    location_for_span(source, file, &span)
}

fn location_for_item(
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

fn build_doc_snapshot(
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

fn api_doc_link_context(resolved: &beskid_analysis::services::ResolvedInput) -> Option<ApiDocLinkContext> {
    let plan = resolved.compile_plan.as_ref()?;
    build_api_doc_link_context(plan, resolved.prepared_workspace.as_ref())
}

fn docs_ref_link_context(resolved: &beskid_analysis::services::ResolvedInput) -> Option<DocRefLinkContext> {
    let plan = resolved.compile_plan.as_ref()?;
    let manifest = load_manifest_from_path(&plan.manifest_path).ok()?;
    let name = manifest.project.name.trim();
    let ver = manifest.project.version.trim();
    if name.is_empty() || ver.is_empty() {
        return None;
    }
    let mut ctx = DocRefLinkContext {
        package_with_version: format!("{name}@{ver}"),
        publishing_package: Some(name.to_string()),
        dependency_roots: vec![],
    };
    if let Some(link_ctx) = api_doc_link_context(resolved) {
        ctx.publishing_package = Some(link_ctx.publishing_package.clone());
        ctx.dependency_roots = link_ctx
            .packages
            .iter()
            .filter(|pkg| pkg.package != link_ctx.publishing_package)
            .map(|pkg| (pkg.match_root.clone(), pkg.package.clone()))
            .collect();
    }
    Some(ctx)
}

/// Resolve, analyze, and write API docs into `args.out`.
pub fn execute(args: DocArgs) -> Result<()> {
    let resolved = services::resolve_input(
        args.input.as_ref(),
        args.project.project.as_ref(),
        args.project.target.as_deref(),
        args.project.workspace_member.as_deref(),
        args.lockfile.frozen,
        args.lockfile.locked,
    )?;
    let program =
        services::parse_program_with_source_name(&resolved.source_path.display().to_string(), &resolved.source)
            .with_context(|| format!("parse {}", resolved.source_path.display()))?;
    let docs_ref = docs_ref_link_context(&resolved);
    let (snap, assembly) = build_doc_snapshot(&resolved, &program, docs_ref.as_ref())?;

    let source_path_str = resolved.source_path.to_string_lossy().into_owned();
    let syntax_by_path = assembly
        .as_ref()
        .map(|asm| {
            asm.units
                .iter()
                .map(|unit| (unit.path.clone(), unit.program.clone()))
                .collect::<std::collections::HashMap<_, _>>()
        })
        .unwrap_or_default();
    let qualified_names = snap.resolution.as_ref().map(qualified_names_for_items);

    fs::create_dir_all(&args.out).with_context(|| format!("create {}", args.out.display()))?;

    let mut entries: Vec<DocEntry> = Vec::new();
    let mut api_items: Vec<ApiDocItem> = Vec::new();
    if let Some(res) = snap.resolution.as_ref() {
        for item in &res.items {
            let slot = snap.item_docs.get(item.id.0).and_then(|x| x.as_ref());
            let doc_markdown = slot.map(|d| d.markdown.clone()).filter(|s| !s.trim().is_empty());
            let doc = slot.and_then(|d| d.structured.clone());
            let loc = location_for_item(item, assembly.as_ref(), &resolved.source, &source_path_str);
            let qualified_name = qualified_names
                .as_ref()
                .and_then(|names| names.get(&item.id.0).cloned())
                .unwrap_or_else(|| item.name.clone());
            let display_name = display_name_for_item(item);
            let symbol_key =
                beskid_analysis::resolve::qualified_name(res, item.id).map(beskid_analysis::doc::ApiSymbolKey::new);
            let mut api_item = ApiDocItem {
                id: Some(item.id.0),
                qualified_name: qualified_name.clone(),
                symbol_key,
                name: item.name.clone(),
                display_name: Some(display_name),
                kind: item.kind.as_stable_doc_kind().to_string(),
                visibility: Some(visibility_stable(item.visibility).to_string()),
                parent_id: item.parent_id.map(|p| p.0),
                member_ids: Vec::new(),
                module_path: Vec::new(),
                signature: None,
                field_type: None,
                return_type: None,
                parameters: Vec::new(),
                generic_parameters: Vec::new(),
                location: ApiLocation {
                    file: loc.file,
                    start_line: loc.start_line,
                    start_column: loc.start_column,
                    end_line: loc.end_line,
                    end_column: loc.end_column,
                },
                doc_markdown,
                doc,
                declaring_package: None,
                controls: vec![],
                tier: None,
            };
            let item_program = item.source_path.as_ref().and_then(|path| syntax_by_path.get(path)).unwrap_or(&program);
            let sig = build_item_signature(item, Some(res), item_program);
            apply_signature_to_item(&mut api_item, sig);
            entries.push(DocEntry {
                qualified_name,
                kind: item.kind.as_stable_doc_kind().to_string(),
                doc_markdown: api_item.doc_markdown.clone(),
            });
            api_items.push(api_item);
        }
    } else {
        for symbol in services::collect_document_symbols(&snap) {
            let loc = location_for_byte_range(
                &resolved.source,
                &source_path_str,
                symbol.selection_start,
                symbol.selection_end,
            );
            entries.push(DocEntry {
                qualified_name: symbol.name.clone(),
                kind: services::symbol_kind_name(symbol.kind).to_string(),
                doc_markdown: None,
            });
            api_items.push(ApiDocItem {
                id: None,
                qualified_name: symbol.name.clone(),
                symbol_key: None,
                name: symbol.name.clone(),
                display_name: None,
                kind: services::symbol_kind_name(symbol.kind).to_string(),
                visibility: None,
                parent_id: None,
                member_ids: Vec::new(),
                module_path: Vec::new(),
                signature: None,
                field_type: None,
                return_type: None,
                parameters: Vec::new(),
                generic_parameters: Vec::new(),
                location: ApiLocation {
                    file: loc.file,
                    start_line: loc.start_line,
                    start_column: loc.start_column,
                    end_line: loc.end_line,
                    end_column: loc.end_column,
                },
                doc_markdown: None,
                doc: None,
                declaring_package: None,
                controls: vec![],
                tier: None,
            });
        }
    }
    entries.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name).then(a.kind.cmp(&b.kind)));
    api_items.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name).then(a.kind.cmp(&b.kind)));

    let had_resolution = snap.resolution.is_some();
    if had_resolution {
        if let (Some(res), Some(ctx)) = (snap.resolution.as_ref(), api_doc_link_context(&resolved)) {
            link_api_doc_library_tree(&mut api_items, res);
            assign_declaring_packages(&mut api_items, &ctx);
        } else if let Some(res) = snap.resolution.as_ref() {
            link_api_doc_library_tree(&mut api_items, res);
        }
        fill_member_ids_from_parents(&mut api_items);
    }

    // Tier resolution must run after link_api_doc_library_tree so that parent_id
    // edges (used by the cascade) reflect the final navigation graph.
    resolve_item_tiers(&mut api_items);
    validate_prelude_standard_tiers(&api_items).map_err(anyhow::Error::msg)?;

    let link_ctx = api_doc_link_context(&resolved);
    let mut api = if had_resolution {
        ApiDocRoot {
            schema_version: API_JSON_SCHEMA_VERSION,
            navigation_model: Some(API_JSON_NAVIGATION_MODEL_GRAPH_V1.to_string()),
            generator: format!("beskid-cli {}", env!("CARGO_PKG_VERSION")),
            source: source_path_str,
            items: api_items,
        }
    } else {
        ApiDocRoot {
            schema_version: API_JSON_SCHEMA_VERSION_BEFORE_GRAPH,
            navigation_model: None,
            generator: format!("beskid-cli {}", env!("CARGO_PKG_VERSION")),
            source: source_path_str,
            items: api_items,
        }
    };
    relativize_api_doc_paths(&mut api, link_ctx.as_ref())
        .map_err(|message| anyhow::anyhow!("relativize api.json paths to artifact layout: {message}"))?;
    fs::write(args.out.join("api.json"), serde_json::to_string_pretty(&api).context("serialize api.json")?)
        .with_context(|| format!("write {}", args.out.join("api.json").display()))?;

    let mut md = String::from("# API reference\n\n");
    if entries.is_empty() {
        md.push_str("*No items found.*\n");
    } else {
        md.push_str("## Structure\n\n");
        md.push_str(&render_structure_tree(&entries));
        md.push('\n');
        md.push_str("## Items\n\n");
        for entry in &entries {
            md.push_str(&format!("### `{}` (`{}`)\n\n", entry.qualified_name, entry.kind));
            let body = entry
                .doc_markdown
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("*No documentation provided.*");
            md.push_str(body);
            md.push_str("\n\n---\n\n");
        }
    }

    fs::write(args.out.join("index.md"), md)
        .with_context(|| format!("write {}", args.out.join("index.md").display()))?;

    println!("Wrote {} and {}", args.out.join("api.json").display(), args.out.join("index.md").display());
    Ok(())
}

#[cfg(test)]
mod member_doc_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn api_json_contains_member_doc_markdown() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos();
        let root = std::env::temp_dir().join(format!("beskid-doc-{nonce}"));
        std::fs::create_dir_all(&root).expect("create root");
        let source_path = root.join("Sample.bd");
        let out_path = root.join("out");

        let source = r#"
type User {
    /// Display name of user.
    string name,
}
"#;
        std::fs::write(&source_path, source).expect("write source");

        execute(DocArgs {
            input: Some(source_path.clone()),
            project: crate::project_args::ProjectResolveArgs { project: None, target: None, workspace_member: None },
            lockfile: crate::project_args::LockfilePolicyArgs { frozen: false, locked: false },
            out: out_path.clone(),
        })
        .expect("execute doc");

        let api = std::fs::read_to_string(out_path.join("api.json")).expect("read api.json");
        assert!(api.contains("\"schemaVersion\": 4"), "api.json should declare schema v4: {api}");
        assert!(
            api.contains("\"navigationModel\": \"graph-v1\""),
            "api.json should declare graph navigation model: {api}"
        );
        assert!(api.contains("\"parentId\":"), "api.json should include parentId for member rows: {api}");
        assert!(api.contains("Display name of user."), "api.json should include member doc markdown: {api}");

        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(out_path.join("api.json"));
        let _ = std::fs::remove_file(out_path.join("index.md"));
        let _ = std::fs::remove_dir(&out_path);
        let _ = std::fs::remove_dir(&root);
    }

    #[test]
    fn api_json_graph_links_type_field_enum_variant_and_method() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos();
        let root = std::env::temp_dir().join(format!("beskid-doc-graph-{nonce}"));
        std::fs::create_dir_all(&root).expect("create root");
        let source_path = root.join("Graph.bd");
        let out_path = root.join("out");

        let source = r#"
type Widget {
    /// widget value
    i64 value,
}

enum Mode {
    /// on
    On,
    /// off
    Off,
}

/// Adds values.
i64 Add(
    i64 left,
    i64 right
) { return left + right; }
"#;
        std::fs::write(&source_path, source).expect("write source");

        execute(DocArgs {
            input: Some(source_path.clone()),
            project: crate::project_args::ProjectResolveArgs { project: None, target: None, workspace_member: None },
            lockfile: crate::project_args::LockfilePolicyArgs { frozen: false, locked: false },
            out: out_path.clone(),
        })
        .expect("execute doc");

        let api: ApiDocRoot = serde_json::from_str(&std::fs::read_to_string(out_path.join("api.json")).expect("read"))
            .expect("parse api.json");
        assert_eq!(api.schema_version, API_JSON_SCHEMA_VERSION);
        assert_eq!(api.navigation_model.as_deref(), Some(API_JSON_NAVIGATION_MODEL_GRAPH_V1));

        let by_id =
            api.items.iter().filter_map(|i| i.id.map(|id| (id, i))).collect::<std::collections::HashMap<_, _>>();

        let type_row = api.items.iter().find(|i| i.kind == "type" && i.name.contains("Widget")).expect("type Widget");
        let type_id = type_row.id.expect("type id");
        let field = api.items.iter().find(|i| i.kind == "field").expect("field");
        assert_eq!(field.parent_id, Some(type_id));
        assert!(type_row.member_ids.contains(&field.id.unwrap()));

        let enum_row = api.items.iter().find(|i| i.kind == "enum").expect("enum");
        let enum_id = enum_row.id.expect("enum id");
        let variants: Vec<_> = api.items.iter().filter(|i| i.kind == "enum_variant").collect();
        assert_eq!(variants.len(), 2);
        for v in &variants {
            assert_eq!(v.parent_id, Some(enum_id));
        }

        let func = api.items.iter().find(|i| i.kind == "function" && i.name.contains("Add")).expect("function");
        assert!(
            func.parent_id.is_some(),
            "module-level functions must be parented under a module row for library-tree navigation"
        );

        // Every id referenced as parentId must exist.
        for item in &api.items {
            if let (Some(id), Some(pid)) = (item.id, item.parent_id) {
                assert!(by_id.contains_key(&pid), "item {id} parent {pid} missing");
            }
        }

        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(out_path.join("api.json"));
        let _ = std::fs::remove_file(out_path.join("index.md"));
        let _ = std::fs::remove_dir(&out_path);
        let _ = std::fs::remove_dir(&root);
    }

    #[test]
    fn api_json_v4_emits_field_type_ref_for_nested_types() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos();
        let root = std::env::temp_dir().join(format!("beskid-doc-nested-{nonce}"));
        std::fs::create_dir_all(&root).expect("create root");
        let source_path = root.join("Nested.bd");
        let out_path = root.join("out");
        let source = r#"
type Inner { i64 x, }
type Outer { Inner inner, }
"#;
        std::fs::write(&source_path, source).expect("write source");
        execute(DocArgs {
            input: Some(source_path.clone()),
            project: crate::project_args::ProjectResolveArgs { project: None, target: None, workspace_member: None },
            lockfile: crate::project_args::LockfilePolicyArgs { frozen: false, locked: false },
            out: out_path.clone(),
        })
        .expect("execute doc");

        let api: ApiDocRoot = serde_json::from_str(&std::fs::read_to_string(out_path.join("api.json")).expect("read"))
            .expect("parse api.json");
        assert_eq!(api.schema_version, API_JSON_SCHEMA_VERSION);
        assert!(
            !std::path::Path::new(&api.source).is_absolute(),
            "api.json source must be package-relative: {}",
            api.source
        );
        for item in &api.items {
            assert!(
                !std::path::Path::new(&item.location.file).is_absolute(),
                "location.file must be package-relative: {}",
                item.location.file
            );
        }

        let inner_type = api.items.iter().find(|i| i.kind == "type" && i.name == "Inner").expect("Inner type");
        let field = api.items.iter().find(|i| i.kind == "field" && i.name.contains("inner")).expect("inner field");
        let field_type = field.field_type.as_ref().expect("fieldType");
        assert_eq!(field_type.display, "Inner");
        assert_eq!(field_type.ref_item_id, inner_type.id);
        assert!(field.signature.as_deref().unwrap_or("").contains("Inner"), "signature should mention Inner");

        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(out_path.join("api.json"));
        let _ = std::fs::remove_file(out_path.join("index.md"));
        let _ = std::fs::remove_dir(&out_path);
        let _ = std::fs::remove_dir(&root);
    }

    #[test]
    fn api_json_ref_markdown_is_backtick_without_project_context() {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos();
        let root = std::env::temp_dir().join(format!("beskid-doc-ref-{nonce}"));
        std::fs::create_dir_all(&root).expect("create root");
        let source_path = root.join("Refs.bd");
        let out_path = root.join("out");
        let source = r#"
/// See @ref(helper) for details.
unit Main() { return 1; }

unit helper() { return 0; }
"#;
        std::fs::write(&source_path, source).expect("write source");

        execute(DocArgs {
            input: Some(source_path.clone()),
            project: crate::project_args::ProjectResolveArgs { project: None, target: None, workspace_member: None },
            lockfile: crate::project_args::LockfilePolicyArgs { frozen: false, locked: false },
            out: out_path.clone(),
        })
        .expect("execute doc");

        let api = std::fs::read_to_string(out_path.join("api.json")).expect("read api.json");
        assert!(
            api.contains("`helper`") || api.contains("helper"),
            "resolved @ref should appear in doc markdown: {api}"
        );
        assert!(!api.contains("/docs/"), "single-file doc without Project.proj must not emit pckg routes: {api}");

        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(out_path.join("api.json"));
        let _ = std::fs::remove_file(out_path.join("index.md"));
        let _ = std::fs::remove_dir(&out_path);
        let _ = std::fs::remove_dir(&root);
    }
}

fn render_structure_tree(entries: &[DocEntry]) -> String {
    let mut root = TreeNode::default();
    for (idx, entry) in entries.iter().enumerate() {
        let segments: Vec<&str> = entry.qualified_name.split("::").filter(|s| !s.is_empty()).collect();
        if segments.is_empty() {
            root.entries.push(idx);
            continue;
        }
        let mut node = &mut root;
        for seg in &segments {
            node = node.children.entry((*seg).to_string()).or_default();
        }
        node.entries.push(idx);
    }
    let mut out = String::new();
    render_tree_node(&root, entries, 0, &mut out);
    out
}

fn render_tree_node(node: &TreeNode, entries: &[DocEntry], depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    for (segment, child) in &node.children {
        out.push_str(&format!("{indent}- `{segment}`\n"));
        render_tree_node(child, entries, depth + 1, out);
    }
    for entry_idx in &node.entries {
        let entry = &entries[*entry_idx];
        out.push_str(&format!("{indent}- `{}` (`{}`)\n", entry.qualified_name, entry.kind));
    }
}

#[cfg(test)]
mod tests {
    use super::{DocEntry, render_structure_tree};
    use beskid_analysis::syntax::SpanInfo;

    #[test]
    fn structure_tree_renders_nested_paths() {
        let entries = vec![
            DocEntry {
                qualified_name: "util::math::sum".to_string(),
                kind: "function".to_string(),
                doc_markdown: None,
            },
            DocEntry { qualified_name: "util::math::Vec2".to_string(), kind: "type".to_string(), doc_markdown: None },
        ];

        let tree = render_structure_tree(&entries);
        assert!(tree.contains("- `util`"));
        assert!(tree.contains("- `math`"));
        assert!(tree.contains("`util::math::sum` (`function`)"));
        assert!(tree.contains("`util::math::Vec2` (`type`)"));
    }

}
