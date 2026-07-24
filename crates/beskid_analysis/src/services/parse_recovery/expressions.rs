//! Expression / pattern recovery candidates (match, lambda, literals, calls).

use crate::parser::Rule;

use super::{RepairCandidate, next_token_start, skip_ws, unbalanced_delimiters};

const PRI_MATCH_CLOSE: u8 = 60;
const PRI_MATCH_ARROW: u8 = 61;
const PRI_MATCH_ARM_COMMA: u8 = 62;
const PRI_LAMBDA_BODY: u8 = 63;
const PRI_STRUCT_CLOSE: u8 = 64;
const PRI_STRUCT_COMMA: u8 = 65;
const PRI_STRUCT_FIELD_COLON: u8 = 66;
const PRI_STRUCT_VALUE_STUB: u8 = 67;
const PRI_ARRAY_CLOSE: u8 = 68;
const PRI_CALL_CLOSE: u8 = 69;
const PRI_GROUPED_CLOSE: u8 = 70;
const PRI_ENUM_CTOR_CLOSE: u8 = 71;
const PRI_PATTERN_CLOSE: u8 = 72;

/// Generate expression- and pattern-oriented repairs near the Pest error locus.
pub fn repairs(
    source: &str,
    error_pos: usize,
    _parse_error: &pest::error::Error<Rule>,
) -> Vec<RepairCandidate> {
    let mut candidates = Vec::new();
    let error_pos = error_pos.min(source.len());
    let insert_at = recovery_insert_pos(source, error_pos);

    match_repairs(source, error_pos, insert_at, &mut candidates);
    lambda_repairs(source, error_pos, insert_at, &mut candidates);
    struct_literal_repairs(source, error_pos, insert_at, &mut candidates);
    array_literal_repairs(source, error_pos, insert_at, &mut candidates);
    paren_expression_repairs(source, error_pos, insert_at, &mut candidates);

    candidates
}

fn recovery_insert_pos(source: &str, error_pos: usize) -> usize {
    next_token_start(source, error_pos).unwrap_or_else(|| source.trim_end().len())
}

fn match_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    candidates: &mut Vec<RepairCandidate>,
) {
    let Some(match_brace) = find_match_block_brace(source, error_pos) else {
        return;
    };

    let (_, _, brace, _) = unbalanced_delimiters(source, error_pos);
    if brace > 0 {
        candidates.push(RepairCandidate::insert(
            insert_at,
            "}",
            "closed incomplete match expression block",
            PRI_MATCH_CLOSE,
        ));
    }

    if missing_match_arm_arrow(source, match_brace, error_pos) {
        let arrow_pos = match_arm_arrow_pos(source, match_brace, error_pos);
        candidates.push(RepairCandidate::insert(
            arrow_pos,
            "=>",
            "inserted missing match arm fat arrow",
            PRI_MATCH_ARROW,
        ));
    }

    if trailing_incomplete_match_arm(source, match_brace, error_pos) {
        candidates.push(RepairCandidate::insert(
            insert_at,
            ",",
            "inserted comma after incomplete match arm",
            PRI_MATCH_ARM_COMMA,
        ));
    }
}

fn lambda_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    candidates: &mut Vec<RepairCandidate>,
) {
    if !lambda_missing_body(source, error_pos) {
        return;
    }

    candidates.push(RepairCandidate::insert(
        insert_at,
        "{}",
        "inserted empty block as placeholder lambda body",
        PRI_LAMBDA_BODY,
    ));
}

fn struct_literal_repairs(
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
        candidates.push(RepairCandidate::insert(
            insert_at,
            "}",
            "closed incomplete struct literal",
            PRI_STRUCT_CLOSE,
        ));
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

fn array_literal_repairs(
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
        candidates.push(RepairCandidate::insert(
            insert_at,
            "]",
            "closed incomplete array literal",
            PRI_ARRAY_CLOSE,
        ));
    }
}

fn paren_expression_repairs(
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

fn find_match_block_brace(source: &str, through: usize) -> Option<usize> {
    let match_kw = find_keyword_backward(source, through, "match")?;
    let after_kw = skip_ws(source, match_kw + "match".len());
    let brace = find_next_brace_after_expression(source, after_kw, through)?;
    if through > brace { Some(brace) } else { None }
}

fn find_next_brace_after_expression(source: &str, from: usize, limit: usize) -> Option<usize> {
    let limit = limit.min(source.len());
    let mut pos = from;
    while pos < limit {
        pos = skip_ws(source, pos);
        if pos >= limit {
            break;
        }
        match source.as_bytes()[pos] {
            b'{' => return Some(pos),
            b'(' | b'[' | b'"' | b'\'' => {
                pos = skip_balanced_token(source, pos, limit)?;
            }
            _ => {
                if is_identifier_start(source.as_bytes(), pos) {
                    pos = skip_identifier(source, pos);
                } else {
                    pos += 1;
                }
            }
        }
    }
    None
}

fn skip_balanced_token(source: &str, pos: usize, limit: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if pos >= limit {
        return None;
    }
    match bytes[pos] {
        b'(' => skip_balanced_delim(source, pos, limit, b'(', b')'),
        b'[' => skip_balanced_delim(source, pos, limit, b'[', b']'),
        b'"' => skip_string(source, pos, limit),
        b'\'' => skip_char(source, pos, limit),
        _ => Some(pos + 1),
    }
}

fn skip_balanced_delim(
    source: &str,
    open_pos: usize,
    limit: usize,
    open: u8,
    close: u8,
) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut i = open_pos;
    while i < limit {
        match bytes[i] {
            b'"' => i = skip_string(source, i, limit)?,
            b'\'' => i = skip_char(source, i, limit)?,
            b'/' if i + 1 < limit && bytes[i + 1] == b'/' => {
                i += 2;
                while i < limit && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < limit && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < limit && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(limit);
            }
            c if c == open => {
                depth += 1;
                i += 1;
            }
            c if c == close => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => i += 1,
        }
    }
    None
}

fn skip_string(source: &str, pos: usize, limit: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = pos + 1;
    while i < limit {
        if bytes[i] == b'\\' {
            i = (i + 2).min(limit);
            continue;
        }
        if bytes[i] == b'"' {
            return Some(i + 1);
        }
        i += 1;
    }
    Some(limit)
}

fn skip_char(source: &str, pos: usize, limit: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = pos + 1;
    while i < limit {
        if bytes[i] == b'\\' {
            i = (i + 2).min(limit);
            continue;
        }
        if bytes[i] == b'\'' {
            return Some(i + 1);
        }
        i += 1;
    }
    Some(limit)
}

fn missing_match_arm_arrow(source: &str, match_brace: usize, error_pos: usize) -> bool {
    let arm_start = current_match_arm_start(source, match_brace, error_pos);
    let segment = &source[arm_start..error_pos];
    if segment.contains("=>") {
        return false;
    }
    arm_segment_looks_like_pattern(segment)
}

fn match_arm_arrow_pos(source: &str, match_brace: usize, error_pos: usize) -> usize {
    let arm_start = current_match_arm_start(source, match_brace, error_pos);
    skip_ws(source, error_pos).max(arm_start)
}

fn current_match_arm_start(source: &str, match_brace: usize, error_pos: usize) -> usize {
    let slice = &source[match_brace + 1..error_pos.min(source.len())];
    if let Some(comma) = slice.rfind(',') {
        match_brace + 1 + comma + 1
    } else {
        match_brace + 1
    }
}

fn arm_segment_looks_like_pattern(segment: &str) -> bool {
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('_') {
        return true;
    }
    trimmed
        .chars()
        .any(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '.' || c == '(')
}

fn trailing_incomplete_match_arm(source: &str, match_brace: usize, error_pos: usize) -> bool {
    let tail_start = skip_ws(source, error_pos);
    if tail_start < source.len() {
        return false;
    }
    let (_, _, brace, _) = unbalanced_delimiters(source, error_pos);
    if brace <= 0 {
        return false;
    }
    let arm_start = current_match_arm_start(source, match_brace, error_pos);
    let segment = source[arm_start..error_pos].trim();
    if segment.is_empty() {
        return false;
    }
    segment.contains("=>") && !segment.ends_with(',')
}

fn lambda_missing_body(source: &str, error_pos: usize) -> bool {
    let Some(arrow) = find_lambda_arrow(source, error_pos) else {
        return false;
    };
    if find_match_block_brace(source, arrow).is_some() {
        return false;
    }
    let after_arrow = skip_ws(source, arrow + 2);
    let tail = skip_ws(source, error_pos);
    tail >= source.trim_end().len() && after_arrow >= tail
}

fn find_lambda_arrow(source: &str, through: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = through.min(source.len());
    while i > 1 {
        i -= 1;
        if bytes[i] == b'>' && i > 0 && bytes[i - 1] == b'=' {
            let arrow = i - 1;
            if lambda_arrow_is_parameter_tail(source, arrow) {
                return Some(arrow);
            }
        }
    }
    None
}

fn lambda_arrow_is_parameter_tail(source: &str, arrow: usize) -> bool {
    let mut pos = arrow;
    while pos > 0 {
        pos -= 1;
        let b = source.as_bytes()[pos];
        if b.is_ascii_whitespace() {
            continue;
        }
        return b == b')' || is_identifier_part(source.as_bytes(), pos);
    }
    false
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
        return is_identifier_part(source.as_bytes(), pos);
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
    if !segment
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    Some(error_pos.min(source.len()))
}

fn struct_field_missing_value_after_colon(
    source: &str,
    struct_brace: usize,
    error_pos: usize,
) -> bool {
    let field_start = current_struct_field_start(source, struct_brace, error_pos);
    let segment = source[field_start..error_pos].trim();
    if !segment.ends_with(':') {
        return false;
    }
    skip_ws(source, error_pos) >= source.trim_end().len()
}

fn current_struct_field_start(source: &str, struct_brace: usize, error_pos: usize) -> usize {
    let slice = &source[struct_brace + 1..error_pos.min(source.len())];
    if let Some(comma) = slice.rfind(',') {
        struct_brace + 1 + comma + 1
    } else {
        struct_brace + 1
    }
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

fn inside_pattern_list(source: &str, error_pos: usize) -> bool {
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

fn inside_grouped_expression(source: &str, error_pos: usize) -> bool {
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
    is_identifier_part(source.as_bytes(), open.saturating_sub(1))
        || prefix.ends_with('!')
        || prefix.ends_with("spawn")
}

fn find_unclosed_paren_before(source: &str, error_pos: usize) -> Option<usize> {
    let (paren, _, _, _) = unbalanced_delimiters(source, error_pos);
    if paren <= 0 {
        return None;
    }
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut i = error_pos.min(source.len());
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

fn paren_prefix_has_enum_path(source: &str, open_paren: usize) -> bool {
    let prefix = prefix_before(source, open_paren);
    prefix.contains("::")
}

fn prefix_before(source: &str, pos: usize) -> &str {
    let end = skip_ws(source, pos);
    let mut start = end;
    while start > 0 {
        start -= 1;
        let b = source.as_bytes()[start];
        if b.is_ascii_whitespace() {
            break;
        }
        if !(b.is_ascii_alphanumeric() || b == b'_' || b == b':' || b == b'.' || b == b'!') {
            break;
        }
    }
    source[start..end].trim()
}

fn find_keyword_backward(source: &str, through: usize, keyword: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let kw = keyword.as_bytes();
    if through < kw.len() {
        return None;
    }
    let max_start = through.min(source.len()) - kw.len();
    let mut start = max_start;
    loop {
        if &bytes[start..start + kw.len()] == kw {
            let before_ok = start == 0 || !is_identifier_part(bytes, start - 1);
            let end = start + kw.len();
            let after_ok = end >= source.len() || !is_identifier_part(bytes, end);
            if before_ok && after_ok {
                return Some(start);
            }
        }
        if start == 0 {
            break;
        }
        start -= 1;
    }
    None
}

fn is_identifier_start(bytes: &[u8], pos: usize) -> bool {
    bytes
        .get(pos)
        .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
}

fn is_identifier_part(bytes: &[u8], pos: usize) -> bool {
    bytes
        .get(pos)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

fn skip_identifier(source: &str, pos: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = pos;
    if i < source.len() && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') {
        i += 1;
        while i < source.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
    }
    i
}
