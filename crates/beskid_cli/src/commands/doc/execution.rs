use std::fs;

use anyhow::{Context, Result};
use beskid_analysis::doc::{
    API_JSON_NAVIGATION_MODEL_GRAPH_V1, API_JSON_SCHEMA_VERSION, API_JSON_SCHEMA_VERSION_BEFORE_GRAPH, ApiDocItem,
    ApiDocRoot, ApiLocation, apply_signature_to_item, assign_declaring_packages, build_item_signature,
    display_name_for_item, fill_member_ids_from_parents, link_api_doc_library_tree, qualified_names_for_items,
    relativize_api_doc_paths, resolve_item_tiers, validate_prelude_standard_tiers,
};
use beskid_analysis::services;

use super::links::{api_doc_link_context, docs_ref_link_context};
use super::model::{DocArgs, DocEntry};
use super::snapshot::{build_doc_snapshot, location_for_byte_range, location_for_item, visibility_stable};
use super::structure_tree::render_structure_tree;

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
