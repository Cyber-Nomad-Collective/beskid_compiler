use beskid_analysis::projects::{parse_manifest, parse_workspace_manifest};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tower_lsp_server::ls_types::*;

use crate::position::offset_range_to_lsp;

pub fn is_manifest_uri(uri: &Uri) -> bool {
    uri.to_string().to_lowercase().ends_with(".proj")
}

pub fn is_workspace_manifest_uri(uri: &Uri) -> bool {
    let path = uri.path().as_str();
    path.rsplit_once('/')
        .is_some_and(|(_, tail)| tail.eq_ignore_ascii_case("workspace.proj"))
}

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

fn first_match_range(text: &str, needle: &str) -> Option<Range> {
    let start = text.find(needle)?;
    let end = start + needle.len();
    Some(offset_range_to_lsp(text, start, end))
}

fn block_header_range(text: &str, block: &str, label: &str) -> Option<Range> {
    let quoted = format!("{block} \"{label}\"");
    first_match_range(text, &quoted)
        .or_else(|| first_match_range(text, &format!("{block} {label}")))
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
    if is_workspace_manifest_uri(uri) {
        return workspace_document_symbols(text);
    }

    let Ok(manifest) = parse_manifest(text) else {
        return Vec::new();
    };

    let mut symbols = Vec::new();
    if let Some(range) = first_match_range(text, "project") {
        symbols.push(build_document_symbol(
            manifest.project.name.clone(),
            Some("project".to_string()),
            SymbolKind::MODULE,
            None,
            range,
            range,
        ));
    }

    for target in manifest.targets {
        let range = block_header_range(text, "target", &target.name)
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));
        symbols.push(build_document_symbol(
            target.name,
            Some("target".to_string()),
            SymbolKind::CLASS,
            None,
            range,
            range,
        ));
    }

    for dependency in manifest.dependencies {
        let range = block_header_range(text, "dependency", &dependency.name)
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));
        symbols.push(build_document_symbol(
            dependency.name,
            Some("dependency".to_string()),
            SymbolKind::NAMESPACE,
            None,
            range,
            range,
        ));
    }

    symbols
}

fn workspace_document_symbols(text: &str) -> Vec<DocumentSymbol> {
    let Ok(manifest) = parse_workspace_manifest(text) else {
        return Vec::new();
    };

    let mut symbols = Vec::new();
    if let Some(range) = first_match_range(text, "workspace") {
        symbols.push(build_document_symbol(
            manifest.workspace.name.clone(),
            Some("workspace".to_string()),
            SymbolKind::MODULE,
            None,
            range,
            range,
        ));
    }

    for member in manifest.members {
        let range = block_header_range(text, "member", &member.name)
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));
        symbols.push(build_document_symbol(
            member.name,
            Some("member".to_string()),
            SymbolKind::MODULE,
            None,
            range,
            range,
        ));
    }

    for dependency_override in manifest.overrides {
        let range = block_header_range(text, "override", &dependency_override.dependency)
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));
        symbols.push(build_document_symbol(
            dependency_override.dependency,
            Some("override".to_string()),
            SymbolKind::CONSTANT,
            None,
            range,
            range,
        ));
    }

    for registry in manifest.registries {
        let range = block_header_range(text, "registry", &registry.name)
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));
        symbols.push(build_document_symbol(
            registry.name,
            Some("registry".to_string()),
            SymbolKind::INTERFACE,
            None,
            range,
            range,
        ));
    }

    symbols
}

type CompletionTriple = (&'static str, CompletionItemKind, &'static str);

const PROJECT_MANIFEST_KEYWORDS: &[CompletionTriple] = &[
    (
        "project",
        CompletionItemKind::MODULE,
        "Top-level project block",
    ),
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
        "name",
        CompletionItemKind::FIELD,
        "Project or dependency name",
    ),
    ("version", CompletionItemKind::FIELD, "Version string"),
    ("root", CompletionItemKind::FIELD, "Source root folder"),
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
        "project" => Some("`project { ... }` defines project metadata."),
        "target" => Some("`target \"Name\" { ... }` defines a build target."),
        "dependency" => Some("`dependency \"Alias\" { ... }` defines a dependency."),
        "name" => Some("`name` is required in the `project` block."),
        "version" => Some("`version` is required in the `project` block."),
        "root" => Some("`root` is optional and defaults to `Src`."),
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
    let mut consumed = 0usize;
    for line in text.lines() {
        let line_start = consumed;
        let line_end = consumed + line.len();
        consumed = line_end.saturating_add(1);

        let trimmed = line.trim();
        if !trimmed.starts_with("path") || !trimmed.contains('=') {
            continue;
        }

        let quote_start = line.find('"')?;
        let quote_end = line[quote_start + 1..].find('"')? + quote_start + 1;
        let value_start = line_start + quote_start + 1;
        let value_end = line_start + quote_end;
        if !(value_start <= offset && offset <= value_end) {
            continue;
        }

        let dep_rel = &line[quote_start + 1..quote_end];
        let current = file_path_from_uri(uri)?;
        let parent = current.parent()?;
        let target = parent.join(dep_rel).join("Project.proj");
        let dep_uri = file_uri_from_path(&target)?;
        return Some(Location {
            uri: dep_uri,
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
        });
    }
    None
}
