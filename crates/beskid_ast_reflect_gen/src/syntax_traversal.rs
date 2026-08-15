//! Emit contract-first syntax traversal surfaces (`Node`, `NodeRef`, `NodeKind`, manifest).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use syn::{Attribute, Fields, Item, Meta};

use crate::syntax_helpers::SYNTAX_NODES_MODULE_PREFIX;
use crate::syntax_nodes::{reflect_sdk_node_kind_names, BANNER};

/// Rust item enum in `syntax/items/node.rs` — host-only; Mod SDK uses `Node` contract + `NodeRef`.
pub const HOST_ONLY_SDK_TYPE_NAMES: &[&str] = &["Node"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AstFieldRole {
    Child,
    Children,
}

#[derive(Debug, Clone)]
pub struct TraversalSlot {
    pub rust_field: String,
    pub beskid_field: String,
    pub beskid_ty: String,
    pub role: AstFieldRole,
}

#[derive(Debug, Clone)]
pub struct TraversalTypeEntry {
    pub type_name: String,
    pub source_rel_path: String,
    pub slots: Vec<TraversalSlot>,
}

fn parse_ast_field_role(attrs: &[Attribute]) -> Option<AstFieldRole> {
    for attr in attrs {
        if !attr.path().is_ident("ast") {
            continue;
        }
        let Meta::List(list) = &attr.meta else {
            continue;
        };
        let tokens = list.tokens.to_string();
        if tokens.contains("children") {
            return Some(AstFieldRole::Children);
        }
        if tokens.contains("child") && !tokens.contains("children") {
            return Some(AstFieldRole::Child);
        }
    }
    None
}

fn field_has_ast_skip(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        if !a.path().is_ident("ast") {
            return false;
        }
        match &a.meta {
            Meta::List(list) => list.tokens.to_string().contains("skip"),
            _ => false,
        }
    })
}

/// Load `#[ast]` child slots from Rust syntax sources (authoritative over shape-only mirrors).
pub fn collect_traversal_entries(analysis_src: &Path) -> Result<Vec<TraversalTypeEntry>, std::io::Error> {
    let files = crate::syntax_helpers::load_syntax_files(analysis_src)?;
    let mut out = Vec::new();
    for (rel, file) in &files {
        for item in &file.items {
            let Some(entry) = traversal_entry_from_item(item, rel) else {
                continue;
            };
            if HOST_ONLY_SDK_TYPE_NAMES.contains(&entry.type_name.as_str()) {
                continue;
            }
            out.push(entry);
        }
    }
    out.sort_by(|a, b| a.type_name.cmp(&b.type_name));
    Ok(out)
}

fn traversal_entry_from_item(item: &Item, source_rel_path: &str) -> Option<TraversalTypeEntry> {
    match item {
        Item::Struct(s) if s.generics.params.is_empty() => {
            let slots = struct_traversal_slots(&s.fields, &s.ident.to_string());
            if slots.is_empty() {
                return None;
            }
            Some(TraversalTypeEntry {
                type_name: s.ident.to_string(),
                source_rel_path: source_rel_path.to_string(),
                slots,
            })
        }
        Item::Enum(e) if e.generics.params.is_empty() => {
            let mut slots = Vec::new();
            for v in &e.variants {
                let vname = v.ident.to_string();
                match &v.fields {
                    Fields::Named(nf) => {
                        for f in &nf.named {
                            if let Some(slot) = field_to_slot(f, &format!("{vname}::{}", f.ident.as_ref()?)) {
                                slots.push(slot);
                            }
                        }
                    }
                    Fields::Unnamed(uf) => {
                        for (i, f) in uf.unnamed.iter().enumerate() {
                            if let Some(slot) = field_to_slot(f, &format!("{vname}::field_{i}")) {
                                slots.push(slot);
                            }
                        }
                    }
                    Fields::Unit => {}
                }
            }
            if slots.is_empty() {
                return None;
            }
            Some(TraversalTypeEntry {
                type_name: e.ident.to_string(),
                source_rel_path: source_rel_path.to_string(),
                slots,
            })
        }
        _ => None,
    }
}

fn struct_traversal_slots(fields: &Fields, type_name: &str) -> Vec<TraversalSlot> {
    match fields {
        Fields::Named(named) => named
            .named
            .iter()
            .filter_map(|f| {
                field_to_slot(f, &f.ident.as_ref().map(|i| i.to_string()).unwrap_or_else(|| "field".into()))
            })
            .collect(),
        Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .enumerate()
            .filter_map(|(i, f)| field_to_slot(f, &format!("{type_name}::field_{i}")))
            .collect(),
        Fields::Unit => Vec::new(),
    }
}

fn field_to_slot(f: &syn::Field, rust_field: &str) -> Option<TraversalSlot> {
    if field_has_ast_skip(&f.attrs) {
        return None;
    }
    let role = parse_ast_field_role(&f.attrs)?;
    let field_key = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_else(|| rust_field.to_string());
    let beskid_field = crate::emit_idents::rust_snake_to_beskid_field_camel(field_key.as_str());
    let beskid_ty = type_name_for_manifest(&f.ty);
    Some(TraversalSlot { rust_field: rust_field.to_string(), beskid_field, beskid_ty, role })
}

fn type_name_for_manifest(ty: &syn::Type) -> String {
    let t = crate::syntax_helpers::peel_type(ty);
    if let Some(inner) = crate::syntax_helpers::spanned_inner_type(t) {
        return type_name_for_manifest(inner);
    }
    if let Some(el) = crate::syntax_helpers::vec_element_type(t) {
        let inner = type_name_for_manifest(el);
        return format!("Vec<{inner}>");
    }
    if let Some(inner) = crate::syntax_helpers::option_inner_type(t) {
        return format!("Option<{}>", type_name_for_manifest(inner));
    }
    crate::syntax_helpers::path_last_ident(t).unwrap_or_else(|| "?".into())
}

pub fn emit_node_kind_bd(reflect_rs: &Path) -> Result<String, std::io::Error> {
    let kinds = reflect_sdk_node_kind_names(reflect_rs)?;
    let mut lines = vec![
        format!("{BANNER}"),
        "/// Classification tokens for syntax queries (mirrors `beskid_analysis::syntax_query::NodeKind`).".into(),
        format!("pub enum NodeKind"),
        "{".into(),
    ];
    for k in &kinds {
        lines.push(format!("    {k},"));
    }
    lines.push("}".into());
    Ok(lines.join("\n") + "\n")
}

pub fn emit_node_ref_bd() -> String {
    format!(
        r#"{BANNER}
/// Opaque stable handle for a syntax node within one `syntaxGenerationId` window.
pub type NodeRef {{
    i64 syntaxGenerationId,
    i64 nodeId,
}}
"#
    )
}

pub fn emit_node_span_bd() -> String {
    format!(
        r#"{BANNER}
/// Source span for one syntax node in one generation.
pub type NodeSpan {{
    i64 start,
    i64 end,
    i64 lineStart,
    i64 columnStart,
    i64 lineEnd,
    i64 columnEnd,
}}
"#
    )
}

pub fn emit_node_contract_bd() -> String {
    format!(
        r#"{BANNER}
/// Sole navigation/query contract for syntax nodes in Mod SDK code.
pub contract Node {{
    {prefix}.NodeKind Kind();
    {prefix}.NodeRef Ref();
    {prefix}.NodeSpan Span();
    void PushChildren({prefix}.NodeChildSink sink);
}}

pub contract NodeChildSink {{
    void Push({prefix}.NodeRef child);
}}
"#,
        prefix = SYNTAX_NODES_MODULE_PREFIX
    )
}

pub fn emit_node_list_bd() -> String {
    let prefix = SYNTAX_NODES_MODULE_PREFIX;
    format!(
        r#"{BANNER}
/// Cons-list of module items as `NodeRef` handles (replaces host-only `Vec<syntax::Node>` enum payloads).
///
/// @variant(Empty) Empty list (length 0).
/// @variant(Cons) Non-empty list: `head` is first (`{prefix}.NodeRef`), `tail` is the remainder (`{prefix}.NodeList`).
pub enum NodeList {{
    Empty,
    Cons(
        {prefix}.NodeRef head,
        {prefix}.NodeList tail,
    ),
}}
"#
    )
}

pub fn emit_traversal_manifest_bd(entries: &[TraversalTypeEntry]) -> String {
    let mut lines = vec![
        BANNER.to_string(),
        "/// Machine-readable child-slot table from Rust `#[ast(child|children|skip)]` (host + verification).".into(),
        "pub type TraversalManifestEntry {".into(),
        "    string typeName,".into(),
        "    string sourceRelPath,".into(),
        "    string rustField,".into(),
        "    string beskidField,".into(),
        "    string beskidType,".into(),
        "    string role,".into(),
        "}".into(),
        "".into(),
        "pub TraversalManifestEntry[] TraversalManifestRows() {".into(),
        "    return [".into(),
    ];
    for e in entries {
        for slot in &e.slots {
            let role = match slot.role {
                AstFieldRole::Child => "child",
                AstFieldRole::Children => "children",
            };
            lines.push(format!(
                "        TraversalManifestEntry {{ typeName: \"{}\", sourceRelPath: \"{}\", rustField: \"{}\", beskidField: \"{}\", beskidType: \"{}\", role: \"{role}\" }},",
                e.type_name, e.source_rel_path, slot.rust_field, slot.beskid_field, slot.beskid_ty
            ));
        }
    }
    lines.push("    ];".into());
    lines.push("}".into());
    lines.join("\n") + "\n"
}

pub fn emit_descendants_contract_bd() -> String {
    format!(
        r#"{BANNER}
/// Pre-order descendant iterator contract (mirrors `beskid_analysis::syntax_query::Descendants`).
pub contract Descendants {{
    bool MoveNext();
    {prefix}.NodeRef Current();
}}
"#,
        prefix = SYNTAX_NODES_MODULE_PREFIX
    )
}

pub fn emit_visit_contract_bd() -> String {
    format!(
        r#"{BANNER}
/// Depth-first visitor contract (mirrors `beskid_analysis::syntax_query::Visit`).
pub contract SyntaxVisitor {{
    void Enter({prefix}.NodeRef node);
    void Exit({prefix}.NodeRef node);
}}
"#,
        prefix = SYNTAX_NODES_MODULE_PREFIX
    )
}

/// Write traversal + contract `.bd` files and return extra barrel module names.
pub fn emit_traversal_sdk(
    nodes_dir: &Path,
    analysis_src: &Path,
    reflect_rs: &Path,
) -> Result<BTreeSet<String>, std::io::Error> {
    let entries = collect_traversal_entries(analysis_src)?;
    let names: BTreeSet<String> =
        ["Node", "NodeRef", "NodeSpan", "NodeKind", "NodeList", "TraversalManifest", "Descendants", "Visit"]
            .into_iter()
            .map(str::to_string)
            .collect();

    fs::write(nodes_dir.join("Node.bd"), emit_node_contract_bd())?;
    fs::write(nodes_dir.join("NodeRef.bd"), emit_node_ref_bd())?;
    fs::write(nodes_dir.join("NodeSpan.bd"), emit_node_span_bd())?;
    fs::write(nodes_dir.join("NodeKind.bd"), emit_node_kind_bd(reflect_rs)?)?;
    fs::write(nodes_dir.join("NodeList.bd"), emit_node_list_bd())?;
    fs::write(nodes_dir.join("TraversalManifest.bd"), emit_traversal_manifest_bd(&entries))?;
    fs::write(nodes_dir.join("Descendants.bd"), emit_descendants_contract_bd())?;
    fs::write(nodes_dir.join("Visit.bd"), emit_visit_contract_bd())?;

    Ok(names)
}

pub fn is_host_only_type(name: &str) -> bool {
    HOST_ONLY_SDK_TYPE_NAMES.contains(&name)
}

/// Kinds that exist in `NodeKind` but have no standalone mirrored shape type for `As*`.
const SKIP_AS_PROJECTION: &[&str] = &["Node", "AssignOp", "FieldKind"];

pub fn emit_query_as_projections(inventory: &[String]) -> String {
    let mut lines = Vec::new();
    for name in inventory {
        if is_host_only_type(name) || SKIP_AS_PROJECTION.contains(&name.as_str()) {
            continue;
        }
        lines.push(format!("pub Option<{name}> As{name}(Beskid.Syntax.Nodes.NodeRef node);"));
    }
    lines.join("\n") + "\n"
}

pub fn emit_query_facade_body(inventory: &[String]) -> String {
    let as_projections = emit_query_as_projections(inventory);
    format!(
        r#"
pub type QueryBounds {{
    i64 maxNodes,
    i64 maxDepth,
}}

pub type SyntaxQuery {{
    Beskid.Syntax.Nodes.NodeRef start,
    QueryBounds bounds,
}}

pub type SyntaxSelection {{
    Beskid.Syntax.Nodes.NodeRef[] nodes,
    QueryBounds bounds,
}}

pub type SyntaxPipeline {{
    Beskid.Syntax.Nodes.NodeRef root,
    QueryBounds bounds,
}}

pub SyntaxQuery At(Beskid.Syntax.Nodes.NodeRef root);
pub SyntaxQuery AtProgram(Beskid.Syntax.Nodes.Program program);

pub Beskid.Syntax.Nodes.NodeRef[] Descendants(SyntaxQuery q);
pub Beskid.Syntax.Nodes.NodeRef[] Children(Beskid.Syntax.Nodes.NodeRef node);
pub Option<Beskid.Syntax.Nodes.NodeRef> Parent(Beskid.Syntax.Nodes.NodeRef node);
pub Beskid.Syntax.Nodes.NodeRef[] Ancestors(Beskid.Syntax.Nodes.NodeRef node);
pub Beskid.Syntax.Nodes.NodeSpan Span(Beskid.Syntax.Nodes.NodeRef node);
pub Option<Beskid.Syntax.Nodes.NodeSpan> TrySpan(Beskid.Syntax.Nodes.NodeRef node);

pub Beskid.Syntax.Nodes.NodeRef[] OfKind(SyntaxQuery q, Beskid.Syntax.Nodes.NodeKind kind);
pub Option<Beskid.Syntax.Nodes.NodeRef> FindFirst(SyntaxQuery q, Beskid.Syntax.Nodes.NodeKind kind);
pub SyntaxSelection Select(SyntaxQuery q);
pub SyntaxSelection WhereKind(SyntaxSelection selection, Beskid.Syntax.Nodes.NodeKind kind);
pub SyntaxPipeline Pipeline(Beskid.Syntax.Nodes.NodeRef root, QueryBounds bounds);
pub SyntaxPipeline Replace(SyntaxPipeline pipeline, Beskid.Syntax.Nodes.NodeRef target, Beskid.Syntax.Nodes.NodeRef replacement);
pub SyntaxPipeline Remove(SyntaxPipeline pipeline, Beskid.Syntax.Nodes.NodeRef target);
pub SyntaxPipeline InsertBefore(SyntaxPipeline pipeline, Beskid.Syntax.Nodes.NodeRef anchor, Beskid.Syntax.Nodes.NodeRef node);
pub SyntaxPipeline InsertAfter(SyntaxPipeline pipeline, Beskid.Syntax.Nodes.NodeRef anchor, Beskid.Syntax.Nodes.NodeRef node);
pub Beskid.Syntax.Nodes.NodeRef Apply(SyntaxPipeline pipeline);

{as_projections}
pub contract SyntaxVisitor {{
    void Enter(Beskid.Syntax.Nodes.NodeRef node);
    void Exit(Beskid.Syntax.Nodes.NodeRef node);
}}

pub void Walk(Beskid.Syntax.Nodes.NodeRef root, SyntaxVisitor visitor);

pub string QueryFacadeVersion() {{
    return "0.4.0";
}}
"#
    )
}

/// Append traversal note to generated shape docs.
pub fn shape_traversal_doc_line() -> &'static str {
    "/// Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`."
}
