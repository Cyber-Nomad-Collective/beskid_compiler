use std::path::Path;

use crate::syntax::Node;

pub(crate) fn import_paths_from_source_full(source: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        if let Some(import_path) = parse_use_import_path(trimmed) {
            paths.push(import_path);
        }
    }
    paths
}

/// Out-of-line module dependencies declared by parsed syntax (`pub mod A.B;`).
///
/// Import closure intentionally treats an unparseable source as contributing no extra module
/// declarations: the regular unit build remains the authority for reporting that parse error,
/// while discovery does not guess at a declaration from stale or malformed text.
pub(crate) fn module_declaration_paths_from_source(path: &Path, source: &str) -> Vec<String> {
    let logical_name = path.display().to_string();
    let Ok(program) = crate::services::parse_program_with_source_name(&logical_name, source) else {
        return Vec::new();
    };

    program
        .node
        .items
        .iter()
        .filter_map(|item| match &item.node {
            Node::ModuleDeclaration(declaration) => Some(
                declaration
                    .node
                    .path
                    .node
                    .segments
                    .iter()
                    .map(|segment| segment.node.name.node.name.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
            ),
            _ => None,
        })
        .filter(|module_path| !module_path.is_empty())
        .collect()
}

/// Module path prefixes from qualified references (`Core.Results.Result`, `Core.Syscall.WriteWith`).
pub(crate) fn module_paths_from_qualified_references(source: &str) -> Vec<String> {
    use std::collections::HashSet;
    let mut paths = HashSet::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("use ") {
            continue;
        }
        for dotted in find_dotted_module_references(trimmed) {
            let segments: Vec<&str> = dotted.split('.').filter(|segment| !segment.is_empty()).collect();
            if segments.len() < 2 {
                continue;
            }
            for len in 1..segments.len() {
                paths.insert(segments[..len].join("."));
            }
        }
    }
    paths.into_iter().collect()
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_part(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn find_dotted_module_references(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if !is_ident_start(bytes[index]) || !bytes[index].is_ascii_uppercase() {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && is_ident_part(bytes[index]) {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'.' {
            continue;
        }
        let mut end = index;
        loop {
            if end >= bytes.len() || bytes[end] != b'.' {
                break;
            }
            end += 1;
            if end >= bytes.len() || !is_ident_start(bytes[end]) {
                break;
            }
            while end < bytes.len() && is_ident_part(bytes[end]) {
                end += 1;
            }
        }
        if end > start + 1 {
            out.push(line[start..end].to_string());
        }
    }
    out
}

fn parse_use_import_path(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix("use ")?;
    let without_comment = rest.split("//").next()?.trim_end_matches(';').trim();
    let import_path = without_comment.split_once(" as ").map(|(path, _)| path.trim()).unwrap_or(without_comment);
    (!import_path.is_empty()).then(|| import_path.to_string())
}

/// When a unit imports nested symbols (`Core.Syscall.ReadRequest`), also pull in the
/// parent module facade (`Core/Syscall/Syscall.bd`) that hosts sibling functions referenced via
/// qualified paths (`Core.Syscall.ReadWith`) without an explicit `use`.
pub(crate) fn parent_module_import_path(import_path: &str) -> Option<String> {
    let segments: Vec<&str> = import_path.split('.').filter(|segment| !segment.is_empty()).collect();
    if segments.len() <= 2 {
        return None;
    }
    Some(segments[..segments.len() - 1].join("."))
}
