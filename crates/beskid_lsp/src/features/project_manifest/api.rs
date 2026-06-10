use beskid_analysis::projects::{parse_bsol_document, project_manifest_for_member_dir, BsolBlock, BsolDocument, BsolItem, BsolSpan, BsolValue};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tower_lsp_server::ls_types::*;

pub use crate::manifest_uri::{is_manifest_uri, is_workspace_manifest_uri};

use crate::position::offset_range_to_lsp;

fn is_ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

pub fn completion_prefix_at_offset(text: &str, offset: usize) -> &str {
    let safe_offset = offset.min(text.len());
    let mut start = safe_offset;
    while start > 0 {
        let Some(ch) = text[..start].chars().next_back() else {
            break;
        };
        if ch.is_alphanumeric() || ch == '_' {
            start -= ch.len_utf8();
            continue;
        }
        break;
    }
    &text[start..safe_offset]
}

pub fn token_at_offset(text: &str, offset: usize) -> Option<&str> {
    let safe_offset = offset.min(text.len());
    let mut start = safe_offset;
    while start > 0 {
        let ch = text[..start].chars().next_back()?;
        if ch.is_alphanumeric() || ch == '_' {
            start -= ch.len_utf8();
            continue;
        }
        break;
    }

    let mut end = safe_offset;
    while end < text.len() {
        let ch = text[end..].chars().next()?;
        if ch.is_alphanumeric() || ch == '_' {
            end += ch.len_utf8();
            continue;
        }
        break;
    }

    if start == end {
        None
    } else {
        Some(&text[start..end])
    }
}

pub fn token_references(text: &str, offset: usize) -> Vec<(usize, usize)> {
    let Some(token) = token_at_offset(text, offset) else {
        return Vec::new();
    };

    let mut references = Vec::new();
    let mut cursor = 0usize;
    while cursor < text.len() {
        let Some(local_idx) = text[cursor..].find(token) else {
            break;
        };
        let start = cursor + local_idx;
        let end = start + token.len();

        let boundary_before = start == 0
            || text[..start]
                .chars()
                .next_back()
                .is_none_or(|ch| !is_ident_char(ch));
        let boundary_after = end >= text.len()
            || text[end..]
                .chars()
                .next()
                .is_none_or(|ch| !is_ident_char(ch));
        if boundary_before && boundary_after {
            references.push((start, end));
        }

        cursor = end;
    }

    references
}

fn span_to_range(text: &str, span: BsolSpan) -> Range {
    offset_range_to_lsp(text, span.start, span.end.max(span.start + 1))
}

const PROJECT_RESERVED: &[&str] = &[
    "target", "dependency", "link", "workspace", "member", "override", "registry", "project",
];

fn is_project_root_block(kind: &str) -> bool {
    !PROJECT_RESERVED.contains(&kind)
}

fn block_label_name(block: &BsolBlock) -> String {
    block
        .label
        .as_ref()
        .map(|label| label.value.clone())
        .unwrap_or_else(|| block.kind.clone())
}

fn assignment_string_value(block: &BsolBlock, key: &str) -> Option<String> {
    for item in &block.items {
        let BsolItem::Assignment(assign) = item else {
            continue;
        };
        if assign.key != key {
            continue;
        }
        if let BsolValue::QuotedString(qs) = &assign.value {
            return Some(qs.value.clone());
        }
    }
    None
}

fn build_document_symbol(
    name: String,
    detail: Option<String>,
    kind: SymbolKind,
    tags: Option<Vec<SymbolTag>>,
    range: Range,
    selection_range: Range,
) -> DocumentSymbol {
    serde_json::from_value(json!({
        "name": name,
        "detail": detail,
        "kind": kind,
        "tags": tags,
        "range": range,
        "selectionRange": selection_range,
        "children": null
    }))
    .expect("valid DocumentSymbol payload")
}

pub fn document_symbols(uri: &Uri, text: &str) -> Vec<DocumentSymbol> {
    let Ok(document) = parse_bsol_document(text) else {
        return Vec::new();
    };

    if is_workspace_manifest_uri(uri) {
        return workspace_document_symbols_from_ast(text, &document);
    }

    project_document_symbols_from_ast(text, &document)
}

fn project_document_symbols_from_ast(text: &str, document: &BsolDocument) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();
    for block in &document.blocks {
        let range = span_to_range(text, block.span);
        if is_project_root_block(&block.kind) {
            let name = assignment_string_value(block, "name")
                .unwrap_or_else(|| block.kind.clone());
            symbols.push(build_document_symbol(
                name,
                Some("project".to_string()),
                SymbolKind::MODULE,
                None,
                range,
                range,
            ));
            continue;
        }

        match block.kind.as_str() {
            "target" => symbols.push(build_document_symbol(
                block_label_name(block),
                Some("target".to_string()),
                SymbolKind::CLASS,
                None,
                range,
                range,
            )),
            "dependency" => symbols.push(build_document_symbol(
                block_label_name(block),
                Some("dependency".to_string()),
                SymbolKind::NAMESPACE,
                None,
                range,
                range,
            )),
            "link" => symbols.push(build_document_symbol(
                "link".to_string(),
                Some("link".to_string()),
                SymbolKind::INTERFACE,
                None,
                range,
                range,
            )),
            _ => {}
        }
    }
    symbols
}

fn workspace_document_symbols_from_ast(text: &str, document: &BsolDocument) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();
    for block in &document.blocks {
        let range = span_to_range(text, block.span);
        match block.kind.as_str() {
            "workspace" => {
                let name = assignment_string_value(block, "name")
                    .unwrap_or_else(|| "workspace".to_string());
                symbols.push(build_document_symbol(
                    name,
                    Some("workspace".to_string()),
                    SymbolKind::MODULE,
                    None,
                    range,
                    range,
                ));
            }
            "member" => symbols.push(build_document_symbol(
                block_label_name(block),
                Some("member".to_string()),
                SymbolKind::MODULE,
                None,
                range,
                range,
            )),
            "override" => symbols.push(build_document_symbol(
                block_label_name(block),
                Some("override".to_string()),
                SymbolKind::CONSTANT,
                None,
                range,
                range,
            )),
            "registry" => symbols.push(build_document_symbol(
                block_label_name(block),
                Some("registry".to_string()),
                SymbolKind::INTERFACE,
                None,
                range,
                range,
            )),
            _ => {}
        }
    }
    symbols
}

type CompletionTriple = (&'static str, CompletionItemKind, &'static str);

const PROJECT_MANIFEST_KEYWORDS: &[CompletionTriple] = &[
    (
        "target",
        CompletionItemKind::MODULE,
        "Top-level target block",
    ),
    (
        "dependency",
        CompletionItemKind::MODULE,
        "Top-level dependency block",
    ),
    (
        "link",
        CompletionItemKind::MODULE,
        "Top-level link block",
    ),
    (
        "name",
        CompletionItemKind::FIELD,
        "Project or dependency name",
    ),
    ("version", CompletionItemKind::FIELD, "Version string"),
    ("root", CompletionItemKind::FIELD, "Source root folder"),
    (
        "type",
        CompletionItemKind::FIELD,
        "Project type: Mod, Meta, Template, Aggregate, or Bsol",
    ),
    (
        "kind",
        CompletionItemKind::FIELD,
        "Target kind: App, Lib, or Test (unquoted or quoted)",
    ),
    ("entry", CompletionItemKind::FIELD, "Target entry file path"),
    (
        "source",
        CompletionItemKind::FIELD,
        "Dependency source: path, git, or registry",
    ),
    ("path", CompletionItemKind::FIELD, "Local dependency path"),
    ("url", CompletionItemKind::FIELD, "Git dependency URL"),
    ("rev", CompletionItemKind::FIELD, "Git dependency revision"),
    (
        "App",
        CompletionItemKind::ENUM_MEMBER,
        "Application target kind",
    ),
    (
        "Lib",
        CompletionItemKind::ENUM_MEMBER,
        "Library target kind",
    ),
    ("Test", CompletionItemKind::ENUM_MEMBER, "Test target kind"),
    ("Mod", CompletionItemKind::ENUM_MEMBER, "Mod project type"),
    ("Meta", CompletionItemKind::ENUM_MEMBER, "Meta project type"),
    ("Template", CompletionItemKind::ENUM_MEMBER, "Template project type"),
    ("Aggregate", CompletionItemKind::ENUM_MEMBER, "Aggregate project type"),
    ("Bsol", CompletionItemKind::ENUM_MEMBER, "Bsol project type"),
];

const WORKSPACE_MANIFEST_KEYWORDS: &[CompletionTriple] = &[
    (
        "workspace",
        CompletionItemKind::MODULE,
        "Top-level workspace block",
    ),
    ("member", CompletionItemKind::MODULE, "Workspace member"),
    (
        "override",
        CompletionItemKind::MODULE,
        "Dependency version override",
    ),
    (
        "registry",
        CompletionItemKind::MODULE,
        "Named package registry",
    ),
    (
        "name",
        CompletionItemKind::FIELD,
        "Workspace or member name",
    ),
    ("path", CompletionItemKind::FIELD, "Member project path"),
    ("url", CompletionItemKind::FIELD, "Registry URL"),
    ("version", CompletionItemKind::FIELD, "Override version"),
    (
        "resolver",
        CompletionItemKind::FIELD,
        "Workspace resolver (e.g. v1)",
    ),
];

pub fn manifest_keyword_completions(uri: &Uri) -> &'static [CompletionTriple] {
    if is_workspace_manifest_uri(uri) {
        WORKSPACE_MANIFEST_KEYWORDS
    } else {
        PROJECT_MANIFEST_KEYWORDS
    }
}

#[derive(Clone, Copy)]
enum EnumFieldAtCursor {
    TargetKind,
    DependencySource,
    WorkspaceResolver,
    ProjectType,
}

fn line_key_value_suffix<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let spaced = format!("{key} = ");
    let tight = format!("{key}=");
    if let Some(pos) = line.rfind(&spaced) {
        return line.get(pos + spaced.len()..);
    }
    if let Some(pos) = line.rfind(&tight) {
        return line.get(pos + tight.len()..);
    }
    None
}

fn manifest_enum_field_at_cursor(text: &str, offset: usize) -> Option<EnumFieldAtCursor> {
    let before = text.get(..offset)?;
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line = before.get(line_start..)?;

    if let Some(rest) = line_key_value_suffix(line, "kind") {
        let t = rest.trim_start();
        if !t.starts_with('"') && (t.is_empty() || token_prefix_chars(t)) {
            return Some(EnumFieldAtCursor::TargetKind);
        }
    }
    if let Some(rest) = line_key_value_suffix(line, "source") {
        let t = rest.trim_start();
        if !t.starts_with('"') && (t.is_empty() || token_prefix_chars(t)) {
            return Some(EnumFieldAtCursor::DependencySource);
        }
    }
    if let Some(rest) = line_key_value_suffix(line, "resolver") {
        let t = rest.trim_start();
        if !t.starts_with('"') && (t.is_empty() || token_prefix_chars(t)) {
            return Some(EnumFieldAtCursor::WorkspaceResolver);
        }
    }
    if let Some(rest) = line_key_value_suffix(line, "type") {
        let t = rest.trim_start();
        if !t.starts_with('"') && (t.is_empty() || token_prefix_chars(t)) {
            return Some(EnumFieldAtCursor::ProjectType);
        }
    }
    None
}

fn token_prefix_chars(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn manifest_enum_completion_items(text: &str, offset: usize) -> Option<Vec<CompletionItem>> {
    let field = manifest_enum_field_at_cursor(text, offset)?;
    let variants: &[(&str, &str)] = match field {
        EnumFieldAtCursor::TargetKind => &[
            ("App", "Application target"),
            ("Lib", "Library target"),
            ("Test", "Test target"),
        ],
        EnumFieldAtCursor::DependencySource => &[
            ("path", "Local path dependency"),
            ("git", "Git dependency (schema only in v1)"),
            ("registry", "Registry dependency (schema only in v1)"),
        ],
        EnumFieldAtCursor::WorkspaceResolver => &[("v1", "Default workspace resolver")],
        EnumFieldAtCursor::ProjectType => &[
            ("Mod", "Compiler mod project"),
            ("Meta", "Meta mod project"),
            ("Template", "Template project"),
            ("Aggregate", "Aggregate project"),
            ("Bsol", "BSOL schema project"),
        ],
    };

    let prefix = completion_prefix_at_offset(text, offset).to_lowercase();
    let mut items: Vec<CompletionItem> = variants
        .iter()
        .filter(|(label, _)| prefix.is_empty() || label.to_lowercase().starts_with(&prefix))
        .map(|&(label, detail)| CompletionItem {
            label: label.to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            detail: Some(detail.to_string()),
            ..CompletionItem::default()
        })
        .collect();

    if items.is_empty() {
        return None;
    }

    items.sort_by(|left, right| left.label.cmp(&right.label));
    Some(items)
}

pub fn hover_markdown(token: &str) -> Option<&'static str> {
    match token {
        "target" => Some("`target \"Name\" { ... }` defines a build target."),
        "dependency" => Some("`dependency \"Alias\" { ... }` defines a dependency."),
        "link" => Some("`link { ... }` declares native library link metadata."),
        "name" => Some("`name` is required in the project root block."),
        "version" => Some("`version` is required in the project root block."),
        "root" => Some("`root` is optional and defaults to `Src`."),
        "type" => Some(
            "`type` is optional (`Mod`, `Meta`, `Template`, `Aggregate`, `Bsol`); Host is the default when omitted.",
        ),
        "kind" => Some(
            "`kind` must be `App`, `Lib`, or `Test` (recommended: unquoted, e.g. `kind = Lib`).",
        ),
        "entry" => Some("`entry` is required and relative to `project.root`."),
        "source" => Some(
            "`source` must be `path`, `git`, or `registry` (recommended: unquoted, e.g. `source = path`).",
        ),
        "path" => Some("`path` is required when `source = path`."),
        "url" => Some("`url` is required when `source = git`."),
        "rev" => Some("`rev` is required when `source = git`."),
        _ => None,
    }
}

fn file_path_from_uri(uri: &Uri) -> Option<PathBuf> {
    let raw = uri.to_string();
    raw.strip_prefix("file://").map(PathBuf::from)
}

fn file_uri_from_path(path: &Path) -> Option<Uri> {
    let raw = format!("file://{}", path.display());
    Uri::from_str(&raw).ok()
}

pub fn dependency_path_location(uri: &Uri, text: &str, offset: usize) -> Option<Location> {
    let document = parse_bsol_document(text).ok()?;
    for block in &document.blocks {
        if block.kind != "dependency" {
            continue;
        }
        for item in &block.items {
            let BsolItem::Assignment(assign) = item else {
                continue;
            };
            if assign.key != "path" {
                continue;
            }
            let BsolValue::QuotedString(path_value) = &assign.value else {
                continue;
            };
            if offset < path_value.span.start || offset > path_value.span.end {
                continue;
            }

            let current = file_path_from_uri(uri)?;
            let parent = current.parent()?;
            let dep_dir = parent.join(&path_value.value);
            let member_manifest = project_manifest_for_member_dir(&dep_dir).ok()?;
            let dep_uri = file_uri_from_path(&member_manifest)?;
            return Some(Location {
                uri: dep_uri,
                range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            });
        }
    }
    None
}
