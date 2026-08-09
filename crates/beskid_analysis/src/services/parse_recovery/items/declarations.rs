use super::super::{
    candidate::RepairCandidate,
    scan::{self, next_token_start, skip_ws},
    syntax_primitives,
};
use super::priorities::{PRIORITY_USE_MOD_SEMI_BEFORE_NEXT, PRIORITY_USE_MOD_SEMI_EOF};

pub(super) fn use_mod_semicolon_repairs(source: &str, error_pos: usize) -> Vec<RepairCandidate> {
    let Some((kind, decl_start)) = find_use_or_mod_declaration(source, error_pos) else {
        return Vec::new();
    };
    if kind == ModDeclKind::Inline {
        return Vec::new();
    }

    let decl_end = declaration_end_without_semicolon(source, decl_start, error_pos);
    if has_semicolon_in_range(source, decl_start, decl_end) {
        return Vec::new();
    }

    let mut out = Vec::new();
    let trimmed_end = source[..decl_end].trim_end().len();
    if trimmed_end > decl_start {
        out.push(RepairCandidate::insert(
            trimmed_end,
            ";",
            "inserted semicolon to complete use or mod declaration",
            PRIORITY_USE_MOD_SEMI_EOF,
        ));
    }

    let after_decl = skip_ws(source, decl_start + 3);
    if let Some(next_item) = next_item_keyword_start(source, after_decl)
        && next_item > decl_start
        && next_item <= source.len()
    {
        let insert_at = syntax_primitives::recovery_insert_position(source, next_item);
        out.push(RepairCandidate::insert(
            insert_at,
            ";",
            "inserted semicolon before next top-level item keyword",
            PRIORITY_USE_MOD_SEMI_BEFORE_NEXT,
        ));
    }
    out
}

enum ModDeclKind {
    Path,
    Inline,
}

fn find_use_or_mod_declaration(source: &str, error_pos: usize) -> Option<(ModDeclKind, usize)> {
    let mut last: Option<(ModDeclKind, usize)> = None;
    let mut pos = 0usize;
    while pos < error_pos {
        if scan::keyword_at(source, pos, "use") {
            if !has_semicolon_in_range(source, pos, error_pos) {
                last = Some((ModDeclKind::Path, pos));
            }
        } else if scan::keyword_at(source, pos, "mod") {
            let kind = mod_declaration_kind(source, pos);
            if kind == ModDeclKind::Path && !has_semicolon_in_range(source, pos, error_pos) {
                last = Some((kind, pos));
            }
        }
        pos += 1;
    }
    last
}

fn mod_declaration_kind(source: &str, mod_pos: usize) -> ModDeclKind {
    let mut pos = skip_ws(source, mod_pos + 3);
    let Some(name_start) = next_token_start(source, pos) else {
        return ModDeclKind::Path;
    };
    pos = name_start;
    while pos < source.len() {
        let b = source.as_bytes()[pos];
        if b.is_ascii_alphanumeric() || b == b'_' {
            pos += 1;
            continue;
        }
        if b == b'.' {
            return ModDeclKind::Path;
        }
        break;
    }
    let after_name = skip_ws(source, pos);
    if after_name < source.len() && source.as_bytes()[after_name] == b'{' {
        ModDeclKind::Inline
    } else {
        ModDeclKind::Path
    }
}

fn declaration_end_without_semicolon(source: &str, decl_start: usize, error_pos: usize) -> usize {
    if let Some(next_item) = next_item_keyword_start(source, decl_start + 1)
        && next_item > decl_start
        && next_item <= error_pos.max(decl_start)
    {
        return next_item;
    }
    error_pos.max(decl_start).min(source.len())
}

fn has_semicolon_in_range(source: &str, start: usize, end: usize) -> bool {
    source.as_bytes()[start..end.min(source.len())].contains(&b';')
}

fn next_item_keyword_start(source: &str, from: usize) -> Option<usize> {
    let mut pos = from;
    while pos < source.len() {
        let token = next_token_start(source, pos)?;
        if scan::keyword_at(source, token, "pub") {
            pos = token + 3;
            continue;
        }
        for kw in syntax_primitives::ITEM_START_KEYWORDS {
            if scan::keyword_at(source, token, kw) {
                return Some(token);
            }
        }
        pos = token + 1;
    }
    None
}
