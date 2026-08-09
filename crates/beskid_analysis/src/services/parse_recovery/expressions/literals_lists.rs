use super::super::scan::{skip_ws, unbalanced_delimiters};
use super::super::{candidate::RepairCandidate, lists, scan};
use super::match_lambda::current_match_arm_start;
use super::priorities::{
    PRI_ANGLE_LIST_TRAILING_COMMA_DELETE, PRI_ANGLE_LIST_TRAILING_COMMA_FIX, PRI_ANGLE_LIST_TRAILING_COMMA_REPLACE,
    PRI_ARRAY_CLOSE, PRI_ARRAY_TRAILING_COMMA_DELETE, PRI_ARRAY_TRAILING_COMMA_FIX, PRI_CALL_CLOSE,
    PRI_ENUM_CTOR_CLOSE, PRI_GROUPED_CLOSE, PRI_PATTERN_CLOSE, PRI_STRUCT_CLOSE, PRI_STRUCT_COMMA,
    PRI_STRUCT_FIELD_COLON, PRI_STRUCT_TRAILING_COMMA_DELETE, PRI_STRUCT_TRAILING_COMMA_FIX, PRI_STRUCT_VALUE_STUB,
};
use super::scanner_context::{find_match_block_brace, find_unclosed_paren_before, prefix_before};

pub(super) fn struct_literal_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    candidates: &mut Vec<RepairCandidate>,
) {
    let Some(struct_brace) = find_struct_literal_brace(source, error_pos) else {
        return;
    };

    let (_, _, brace, _) = unbalanced_delimiters(source, error_pos);
    if brace > 0 {
        candidates.push(RepairCandidate::insert(insert_at, "}", "closed incomplete struct literal", PRI_STRUCT_CLOSE));
    }

    if struct_field_needs_comma(source, struct_brace, error_pos) {
        candidates.push(RepairCandidate::insert(
            insert_at,
            ",",
            "inserted comma between struct literal fields",
            PRI_STRUCT_COMMA,
        ));
    }

    if let Some(colon_pos) = struct_field_needs_colon(source, struct_brace, error_pos) {
        candidates.push(RepairCandidate::insert(
            colon_pos,
            ": _",
            "inserted placeholder field value after struct field name",
            PRI_STRUCT_FIELD_COLON,
        ));
    }

    if struct_field_missing_value_after_colon(source, struct_brace, error_pos) {
        candidates.push(RepairCandidate::insert(
            insert_at,
            "0",
            "inserted numeric stub for missing struct field value (last resort)",
            PRI_STRUCT_VALUE_STUB,
        ));
    }
}

pub(super) fn array_literal_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    candidates: &mut Vec<RepairCandidate>,
) {
    if !inside_unclosed_array_literal(source, error_pos) {
        return;
    }

    let (paren, bracket, _, _) = unbalanced_delimiters(source, error_pos);
    if bracket > 0 && paren >= 0 {
        candidates.push(RepairCandidate::insert(insert_at, "]", "closed incomplete array literal", PRI_ARRAY_CLOSE));
    }
}

pub(super) fn paren_expression_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    candidates: &mut Vec<RepairCandidate>,
) {
    let (paren, _, _, _) = unbalanced_delimiters(source, error_pos);
    if paren <= 0 {
        return;
    }

    if inside_enum_constructor_call(source, error_pos) {
        candidates.push(RepairCandidate::insert(
            insert_at,
            ")",
            "closed incomplete enum constructor argument list",
            PRI_ENUM_CTOR_CLOSE,
        ));
        return;
    }

    if inside_pattern_list(source, error_pos) {
        candidates.push(RepairCandidate::insert(
            insert_at,
            ")",
            "closed incomplete pattern parenthesis list",
            PRI_PATTERN_CLOSE,
        ));
        return;
    }

    if inside_grouped_expression(source, error_pos) {
        candidates.push(RepairCandidate::insert(
            insert_at,
            ")",
            "closed incomplete grouped expression",
            PRI_GROUPED_CLOSE,
        ));
        return;
    }

    if inside_call_argument_list(source, error_pos) {
        candidates.push(RepairCandidate::insert(
            insert_at,
            ")",
            "closed incomplete call argument list",
            PRI_CALL_CLOSE,
        ));
    }
}

pub(super) fn struct_field_separator_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    candidates: &mut Vec<RepairCandidate>,
) {
    lists::trailing_separator_before_close_delimiter(
        source,
        error_pos,
        insert_at,
        candidates,
        b'{',
        b'}',
        struct_brace_opens_literal_at,
        "field: 0",
        PRI_STRUCT_TRAILING_COMMA_DELETE,
        PRI_STRUCT_TRAILING_COMMA_FIX,
        "removed trailing comma in struct literal field list",
        "inserted placeholder struct field after trailing comma",
    );
}

pub(super) fn bracket_argument_separator_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    candidates: &mut Vec<RepairCandidate>,
) {
    lists::trailing_separator_before_close_delimiter(
        source,
        error_pos,
        insert_at,
        candidates,
        b'[',
        b']',
        inside_expression_array_list,
        "0",
        PRI_ARRAY_TRAILING_COMMA_DELETE,
        PRI_ARRAY_TRAILING_COMMA_FIX,
        "removed trailing comma in expression array list",
        "inserted placeholder array item after trailing comma",
    );
}

pub(super) fn angle_list_separator_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    candidates: &mut Vec<RepairCandidate>,
) {
    lists::replace_trailing_separator_with_close_before_delimiter(
        source,
        error_pos,
        insert_at,
        candidates,
        b'<',
        b'>',
        inside_generic_or_type_angle_list,
        "T",
        PRI_ANGLE_LIST_TRAILING_COMMA_DELETE,
        PRI_ANGLE_LIST_TRAILING_COMMA_REPLACE,
        PRI_ANGLE_LIST_TRAILING_COMMA_FIX,
        "removed trailing comma in generic or type angle list",
        "closed generic/type angle list trailing comma",
        "inserted placeholder generic entry after trailing comma",
    );
}

fn find_struct_literal_brace(source: &str, through: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = through.min(source.len());
    while i > 0 {
        i -= 1;
        if bytes[i] != b'{' {
            continue;
        }
        if struct_brace_opens_literal(source, i) {
            return Some(i);
        }
    }
    None
}

fn struct_brace_opens_literal(source: &str, brace: usize) -> bool {
    let mut pos = brace;
    while pos > 0 {
        pos -= 1;
        let b = source.as_bytes()[pos];
        if b.is_ascii_whitespace() {
            continue;
        }
        if b == b')' || b == b']' || b == b'}' || b == b'=' || b == b'>' || b == b':' {
            return false;
        }
        return scan::is_ident_continue(b);
    }
    false
}

fn struct_field_needs_comma(source: &str, struct_brace: usize, error_pos: usize) -> bool {
    let field_start = current_struct_field_start(source, struct_brace, error_pos);
    let segment = source[field_start..error_pos].trim();
    if segment.is_empty() || !segment.contains(':') {
        return false;
    }
    let tail = skip_ws(source, error_pos);
    tail >= source.len() || source.as_bytes()[tail] != b'}'
}

fn struct_field_needs_colon(source: &str, struct_brace: usize, error_pos: usize) -> Option<usize> {
    let field_start = current_struct_field_start(source, struct_brace, error_pos);
    let segment = source[field_start..error_pos].trim();
    if segment.is_empty() || segment.contains(':') {
        return None;
    }
    if !segment.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(error_pos.min(source.len()))
}

fn struct_field_missing_value_after_colon(source: &str, struct_brace: usize, error_pos: usize) -> bool {
    let field_start = current_struct_field_start(source, struct_brace, error_pos);
    let segment = source[field_start..error_pos].trim();
    if !segment.ends_with(':') {
        return false;
    }
    skip_ws(source, error_pos) >= source.trim_end().len()
}

fn current_struct_field_start(source: &str, struct_brace: usize, error_pos: usize) -> usize {
    let slice = &source[struct_brace + 1..error_pos.min(source.len())];
    if let Some(comma) = slice.rfind(',') { struct_brace + 1 + comma + 1 } else { struct_brace + 1 }
}

fn inside_unclosed_array_literal(source: &str, error_pos: usize) -> bool {
    let bytes = source.as_bytes();
    let mut i = error_pos.min(source.len());
    while i > 0 {
        i -= 1;
        if bytes[i] != b'[' {
            continue;
        }
        if array_bracket_opens_literal(source, i) {
            let (_, bracket, _, _) = unbalanced_delimiters(source, error_pos);
            return bracket > 0;
        }
    }
    false
}

fn array_bracket_opens_literal(source: &str, bracket: usize) -> bool {
    let mut pos = bracket;
    while pos > 0 {
        pos -= 1;
        let b = source.as_bytes()[pos];
        if b.is_ascii_whitespace() {
            continue;
        }
        return b != b':' && b != b'[';
    }
    true
}

fn inside_enum_constructor_call(source: &str, error_pos: usize) -> bool {
    let Some(open) = find_unclosed_paren_before(source, error_pos) else {
        return false;
    };
    if !paren_prefix_has_enum_path(source, open) {
        return false;
    }
    !enum_paren_is_pattern(source, error_pos, open)
}

pub(super) fn inside_pattern_list(source: &str, error_pos: usize) -> bool {
    let Some(open) = find_unclosed_paren_before(source, error_pos) else {
        return false;
    };
    if !paren_prefix_has_enum_path(source, open) {
        return false;
    }
    enum_paren_is_pattern(source, error_pos, open)
}

fn enum_paren_is_pattern(source: &str, error_pos: usize, open_paren: usize) -> bool {
    let Some(match_brace) = find_match_block_brace(source, error_pos) else {
        return false;
    };
    let arm_start = current_match_arm_start(source, match_brace, error_pos);
    if open_paren <= arm_start {
        return false;
    }
    let arm_segment = &source[arm_start..error_pos];
    !arm_segment.contains("=>")
}

pub(super) fn inside_grouped_expression(source: &str, error_pos: usize) -> bool {
    let Some(open) = find_unclosed_paren_before(source, error_pos) else {
        return false;
    };
    let prefix = prefix_before(source, open);
    prefix.ends_with('=') || prefix.ends_with("return") || prefix.ends_with('(')
}

fn inside_call_argument_list(source: &str, error_pos: usize) -> bool {
    let Some(open) = find_unclosed_paren_before(source, error_pos) else {
        return false;
    };
    let prefix = prefix_before(source, open);
    (open > 0 && scan::is_ident_continue(source.as_bytes()[open - 1]))
        || prefix.ends_with('!')
        || prefix.ends_with("spawn")
}

pub(super) fn inside_expression_argument_list(source: &str, open: usize, error_pos: usize) -> bool {
    if !inside_call_argument_list(source, error_pos) {
        return false;
    }

    let before_open = source[..open].trim_end();
    if before_open.ends_with('!') {
        return true;
    }

    true
}

fn inside_expression_array_list(source: &str, open: usize, error_pos: usize) -> bool {
    if !inside_unclosed_array_literal(source, error_pos) {
        return false;
    }

    array_bracket_opens_literal(source, open)
}

fn inside_generic_or_type_angle_list(source: &str, open: usize, _scan_pos: usize) -> bool {
    let prefix = prefix_before(source, open);
    if prefix.is_empty() {
        return false;
    }

    let trimmed = prefix.trim_end();
    if trimmed.is_empty() {
        return false;
    }
    let prev = trimmed.as_bytes()[trimmed.len() - 1];
    prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'.' || prev == b':' || prev == b'>' || prev == b')'
}

fn struct_brace_opens_literal_at(source: &str, open: usize, scan_pos: usize) -> bool {
    find_struct_literal_brace(source, scan_pos).is_some_and(|struct_open| struct_open == open)
}

fn paren_prefix_has_enum_path(source: &str, open_paren: usize) -> bool {
    let prefix = prefix_before(source, open_paren);
    prefix.contains("::")
}
