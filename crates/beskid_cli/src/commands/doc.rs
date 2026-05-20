//! `beskid doc` — emit `api.json` and `index.md` API documentation for resolved sources.

use anyhow::{Context, Result};
use beskid_analysis::doc::{
    API_JSON_NAVIGATION_MODEL_GRAPH_V1, API_JSON_SCHEMA_VERSION,
    API_JSON_SCHEMA_VERSION_BEFORE_GRAPH, ApiDocItem, ApiDocRoot, ApiLocation, DocRefLinkContext,
};
use beskid_analysis::hir::HirVisibility;
use beskid_analysis::projects::load_manifest_from_path;
use beskid_analysis::services;
use beskid_analysis::syntax::SpanInfo;
use clap::Args;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;

use crate::project_args::{LockfilePolicyArgs, ProjectResolveArgs};

#[derive(Args, Debug)]
pub struct DocArgs {
    /// Beskid source file (same resolution as `analyze` when combined with `--project`)
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
    LocationJson {
        file: file.to_string(),
        start_line: sl,
        start_column: sc,
        end_line: el,
        end_column: ec,
    }
}

fn location_for_byte_range(source: &str, file: &str, start: usize, end: usize) -> LocationJson {
    let span = SpanInfo::from_byte_range_in_source(source, start, end);
    location_for_span(source, file, &span)
}

fn fill_member_ids_from_parents(items: &mut [ApiDocItem]) {
    let mut by_parent: HashMap<usize, Vec<usize>> = HashMap::new();
    for it in items.iter() {
        if let (Some(child_id), Some(pid)) = (it.id, it.parent_id) {
            by_parent.entry(pid).or_default().push(child_id);
        }
    }
    for v in by_parent.values_mut() {
        v.sort_unstable();
    }
    for it in items.iter_mut() {
        if let Some(id) = it.id {
            it.member_ids = by_parent.remove(&id).unwrap_or_default();
        } else {
            it.member_ids.clear();
        }
    }
}

fn docs_ref_link_context(
    resolved: &beskid_analysis::services::ResolvedInput,
) -> Option<DocRefLinkContext> {
    let plan = resolved.compile_plan.as_ref()?;
    let manifest = load_manifest_from_path(&plan.manifest_path).ok()?;
    let name = manifest.project.name.trim();
    let ver = manifest.project.version.trim();
    if name.is_empty() || ver.is_empty() {
        return None;
    }
    Some(DocRefLinkContext {
        package_with_version: format!("{name}@{ver}"),
    })
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
    let program = services::parse_program_with_source_name(
        &resolved.source_path.display().to_string(),
        &resolved.source,
    )
    .with_context(|| format!("parse {}", resolved.source_path.display()))?;
    let docs_ref = docs_ref_link_context(&resolved);
    let snap = services::build_document_analysis(
        &program,
        resolved.source_path.display().to_string(),
        &resolved.source,
        docs_ref.as_ref(),
    );

    let source_path_str = resolved.source_path.to_string_lossy().into_owned();

    fs::create_dir_all(&args.out).with_context(|| format!("create {}", args.out.display()))?;

    let mut entries: Vec<DocEntry> = Vec::new();
    let mut api_items: Vec<ApiDocItem> = Vec::new();
    if let Some(res) = snap.resolution.as_ref() {
        for item in &res.items {
            let slot = snap.item_docs.get(item.id.0).and_then(|x| x.as_ref());
            let doc_markdown = slot
                .map(|d| d.markdown.clone())
                .filter(|s| !s.trim().is_empty());
            let doc = slot.and_then(|d| d.structured.clone());
            let loc = location_for_span(&resolved.source, &source_path_str, &item.span);
            entries.push(DocEntry {
                qualified_name: item.name.clone(),
                kind: item.kind.as_stable_doc_kind().to_string(),
                doc_markdown: doc_markdown.clone(),
            });
            api_items.push(ApiDocItem {
                id: Some(item.id.0),
                qualified_name: item.name.clone(),
                name: item.name.clone(),
                kind: item.kind.as_stable_doc_kind().to_string(),
                visibility: Some(visibility_stable(item.visibility).to_string()),
                parent_id: item.parent_id.map(|p| p.0),
                member_ids: Vec::new(),
                location: ApiLocation {
                    file: loc.file,
                    start_line: loc.start_line,
                    start_column: loc.start_column,
                    end_line: loc.end_line,
                    end_column: loc.end_column,
                },
                doc_markdown,
                doc,
                controls: vec![],
            });
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
                name: symbol.name.clone(),
                kind: services::symbol_kind_name(symbol.kind).to_string(),
                visibility: None,
                parent_id: None,
                member_ids: Vec::new(),
                location: ApiLocation {
                    file: loc.file,
                    start_line: loc.start_line,
                    start_column: loc.start_column,
                    end_line: loc.end_line,
                    end_column: loc.end_column,
                },
                doc_markdown: None,
                doc: None,
                controls: vec![],
            });
        }
    }
    entries.sort_by(|a, b| {
        a.qualified_name
            .cmp(&b.qualified_name)
            .then(a.kind.cmp(&b.kind))
    });
    api_items.sort_by(|a, b| {
        a.qualified_name
            .cmp(&b.qualified_name)
            .then(a.kind.cmp(&b.kind))
    });

    let had_resolution = snap.resolution.is_some();
    if had_resolution {
        fill_member_ids_from_parents(&mut api_items);
    }

    let api = if had_resolution {
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
    fs::write(
        args.out.join("api.json"),
        serde_json::to_string_pretty(&api).context("serialize api.json")?,
    )
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
            md.push_str(&format!(
                "### `{}` (`{}`)\n\n",
                entry.qualified_name, entry.kind
            ));
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

    println!(
        "Wrote {} and {}",
        args.out.join("api.json").display(),
        args.out.join("index.md").display()
    );
    Ok(())
}

#[cfg(test)]
mod member_doc_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn api_json_contains_member_doc_markdown() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
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
            project: crate::project_args::ProjectResolveArgs {
                project: None,
                target: None,
                workspace_member: None,
            },
            lockfile: crate::project_args::LockfilePolicyArgs {
                frozen: false,
                locked: false,
            },
            out: out_path.clone(),
        })
        .expect("execute doc");

        let api = std::fs::read_to_string(out_path.join("api.json")).expect("read api.json");
        assert!(
            api.contains("\"schemaVersion\": 3"),
            "api.json should declare schema v3: {api}"
        );
        assert!(
            api.contains("\"navigationModel\": \"graph-v1\""),
            "api.json should declare graph navigation model: {api}"
        );
        assert!(
            api.contains("\"parentId\":"),
            "api.json should include parentId for member rows: {api}"
        );
        assert!(
            api.contains("Display name of user."),
            "api.json should include member doc markdown: {api}"
        );

        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(out_path.join("api.json"));
        let _ = std::fs::remove_file(out_path.join("index.md"));
        let _ = std::fs::remove_dir(&out_path);
        let _ = std::fs::remove_dir(&root);
    }

    #[test]
    fn api_json_graph_links_type_field_enum_variant_and_method() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
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
            project: crate::project_args::ProjectResolveArgs {
                project: None,
                target: None,
                workspace_member: None,
            },
            lockfile: crate::project_args::LockfilePolicyArgs {
                frozen: false,
                locked: false,
            },
            out: out_path.clone(),
        })
        .expect("execute doc");

        let api: ApiDocRoot = serde_json::from_str(
            &std::fs::read_to_string(out_path.join("api.json")).expect("read"),
        )
        .expect("parse api.json");
        assert_eq!(api.schema_version, API_JSON_SCHEMA_VERSION);
        assert_eq!(
            api.navigation_model.as_deref(),
            Some(API_JSON_NAVIGATION_MODEL_GRAPH_V1)
        );

        let by_id = api
            .items
            .iter()
            .filter_map(|i| i.id.map(|id| (id, i)))
            .collect::<std::collections::HashMap<_, _>>();

        let type_row = api
            .items
            .iter()
            .find(|i| i.kind == "type" && i.name.contains("Widget"))
            .expect("type Widget");
        let type_id = type_row.id.expect("type id");
        let field = api.items.iter().find(|i| i.kind == "field").expect("field");
        assert_eq!(field.parent_id, Some(type_id));
        assert!(type_row.member_ids.contains(&field.id.unwrap()));

        let enum_row = api.items.iter().find(|i| i.kind == "enum").expect("enum");
        let enum_id = enum_row.id.expect("enum id");
        let variants: Vec<_> = api
            .items
            .iter()
            .filter(|i| i.kind == "enum_variant")
            .collect();
        assert_eq!(variants.len(), 2);
        for v in &variants {
            assert_eq!(v.parent_id, Some(enum_id));
        }

        let func = api
            .items
            .iter()
            .find(|i| i.kind == "function" && i.name.contains("Add"))
            .expect("function");
        assert!(func.parent_id.is_none());

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
    fn api_json_ref_markdown_is_backtick_without_project_context() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("beskid-doc-ref-{nonce}"));
        std::fs::create_dir_all(&root).expect("create root");
        let source_path = root.join("Refs.bd");
        let out_path = root.join("out");
        let source = r#"
/// See @ref(helper) for details.
unit main() { return 1; }

unit helper() { return 0; }
"#;
        std::fs::write(&source_path, source).expect("write source");

        execute(DocArgs {
            input: Some(source_path.clone()),
            project: crate::project_args::ProjectResolveArgs {
                project: None,
                target: None,
                workspace_member: None,
            },
            lockfile: crate::project_args::LockfilePolicyArgs {
                frozen: false,
                locked: false,
            },
            out: out_path.clone(),
        })
        .expect("execute doc");

        let api = std::fs::read_to_string(out_path.join("api.json")).expect("read api.json");
        assert!(
            api.contains("`helper`") || api.contains("helper"),
            "resolved @ref should appear in doc markdown: {api}"
        );
        assert!(
            !api.contains("/docs/"),
            "single-file doc without Project.proj must not emit pckg routes: {api}"
        );

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
        let segments: Vec<&str> = entry
            .qualified_name
            .split("::")
            .filter(|s| !s.is_empty())
            .collect();
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
        out.push_str(&format!(
            "{indent}- `{}` (`{}`)\n",
            entry.qualified_name, entry.kind
        ));
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
            DocEntry {
                qualified_name: "util::math::Vec2".to_string(),
                kind: "type".to_string(),
                doc_markdown: None,
            },
        ];

        let tree = render_structure_tree(&entries);
        assert!(tree.contains("- `util`"));
        assert!(tree.contains("- `math`"));
        assert!(tree.contains("`util::math::sum` (`function`)"));
        assert!(tree.contains("`util::math::Vec2` (`type`)"));
    }

    #[test]
    fn location_from_byte_range_matches_line_col() {
        let src = "a\nbc\ndef";
        // "d" is third line
        let span = SpanInfo::from_byte_range_in_source(src, 5, 6);
        assert_eq!(span.line_col_start, (3, 1));
        assert_eq!(span.line_col_end, (3, 2));
    }
}
