//! Expression / pattern recovery candidates (match, lambda, literals, calls).

use crate::parser::Rule;

use super::{candidate::RepairCandidate, expected_tokens, lists, scan, syntax_primitives};
use super::scan::{next_token_start, skip_ws, unbalanced_delimiters};

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
const PRI_CALL_TRAILING_COMMA_DELETE: u8 = 70;
const PRI_CALL_TRAILING_COMMA_FIX: u8 = 71;
const PRI_ARRAY_TRAILING_COMMA_DELETE: u8 = 72;
const PRI_ARRAY_TRAILING_COMMA_FIX: u8 = 73;
const PRI_GROUPED_CLOSE: u8 = 74;
const PRI_ENUM_CTOR_CLOSE: u8 = 75;
const PRI_PATTERN_CLOSE: u8 = 76;
const PRI_ANGLE_LIST_TRAILING_COMMA_DELETE: u8 = 77;
const PRI_ANGLE_LIST_TRAILING_COMMA_FIX: u8 = 78;
const PRI_ANGLE_LIST_TRAILING_COMMA_REPLACE: u8 = 79;
const PRI_STRUCT_TRAILING_COMMA_DELETE: u8 = 80;
const PRI_STRUCT_TRAILING_COMMA_FIX: u8 = 81;
const PRI_EXPR_EXPECTED_OPERATOR: u8 = 82;
const PRI_MEMBER_ACCESS_STUB: u8 = 83;
const PRI_INDEX_CLOSE: u8 = 84;
const PRI_INDEX_PLACEHOLDER: u8 = 85;
const PRI_EXPR_CONTROL_BODY: u8 = 86;

/// Generate expression- and pattern-oriented repairs near the Pest error locus.
pub fn repairs(source: &str, error_pos: usize, parse_error: &pest::error::Error<Rule>) -> Vec<RepairCandidate> {
    let mut candidates = Vec::new();
    let error_pos = syntax_primitives::recovery_scan_pos(source, error_pos);
    let tail_pos = source.trim_end().len();
    let insert_at = if error_pos >= tail_pos && tail_pos > 0 && source[..tail_pos].ends_with('.') {
        tail_pos
    } else if error_pos >= tail_pos && source[..tail_pos].ends_with('[') {
        tail_pos
    } else {
        recovery_insert_pos(source, error_pos)
    };

    match_repairs(source, error_pos, insert_at, &mut candidates);
    lambda_repairs(source, error_pos, insert_at, &mut candidates);
    struct_literal_repairs(source, error_pos, insert_at, &mut candidates);
    struct_field_separator_repairs(source, error_pos, insert_at, &mut candidates);
    array_literal_repairs(source, error_pos, insert_at, &mut candidates);
    paren_expression_repairs(source, error_pos, insert_at, &mut candidates);
    paren_argument_separator_repairs(source, error_pos, insert_at, &mut candidates);
    bracket_argument_separator_repairs(source, error_pos, insert_at, &mut candidates);
    expression_operator_repairs(source, error_pos, insert_at, parse_error, &mut candidates);
    member_access_repairs(source, error_pos, insert_at, parse_error, &mut candidates);
    index_expression_repairs(source, error_pos, insert_at, &mut candidates);
    control_expression_body_repairs(source, error_pos, insert_at, parse_error, &mut candidates);
    angle_list_separator_repairs(source, error_pos, insert_at, &mut candidates);

    candidates
}

fn recovery_insert_pos(source: &str, error_pos: usize) -> usize {
    next_token_start(source, error_pos).unwrap_or_else(|| source.trim_end().len())
}

fn match_repairs(source: &str, error_pos: usize, insert_at: usize, candidates: &mut Vec<RepairCandidate>) {
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

fn lambda_repairs(source: &str, error_pos: usize, insert_at: usize, candidates: &mut Vec<RepairCandidate>) {
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

fn struct_literal_repairs(source: &str, error_pos: usize, insert_at: usize, candidates: &mut Vec<RepairCandidate>) {
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

fn array_literal_repairs(source: &str, error_pos: usize, insert_at: usize, candidates: &mut Vec<RepairCandidate>) {
    if !inside_unclosed_array_literal(source, error_pos) {
        return;
    }

    let (paren, bracket, _, _) = unbalanced_delimiters(source, error_pos);
    if bracket > 0 && paren >= 0 {
        candidates.push(RepairCandidate::insert(insert_at, "]", "closed incomplete array literal", PRI_ARRAY_CLOSE));
    }
}

fn paren_expression_repairs(source: &str, error_pos: usize, insert_at: usize, candidates: &mut Vec<RepairCandidate>) {
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

fn paren_argument_separator_repairs(
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
        b'(',
        b')',
        |source, open, scan_pos| {
            inside_expression_argument_list(source, open, scan_pos)
                || inside_grouped_expression(source, scan_pos)
                || inside_pattern_list(source, scan_pos)
        },
        "0",
        PRI_CALL_TRAILING_COMMA_DELETE,
        PRI_CALL_TRAILING_COMMA_FIX,
        "removed trailing comma in expression argument list",
        "inserted placeholder argument after trailing comma",
    );
}

fn expression_operator_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    parse_error: &pest::error::Error<Rule>,
    candidates: &mut Vec<RepairCandidate>,
) {
    if !expression_expected_after_operator(parse_error) {
        return;
    }

    let Some(prev) = scan::prev_non_ws_byte(source, error_pos) else {
        return;
    };
    if !scan::is_operator_byte(prev) {
        return;
    }

    candidates.push(RepairCandidate::insert(
        insert_at,
        "0",
        "inserted placeholder expression after trailing operator",
        PRI_EXPR_EXPECTED_OPERATOR,
    ));
}

fn member_access_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    _parse_error: &pest::error::Error<Rule>,
    candidates: &mut Vec<RepairCandidate>,
) {
    let tail_pos = source.trim_end().len();
    let dot_pos = if error_pos > 0 && error_pos <= source.len() && source.as_bytes()[error_pos - 1] == b'.' {
        error_pos - 1
    } else if error_pos >= tail_pos && tail_pos > 0 && source.as_bytes()[tail_pos.saturating_sub(1)] == b'.' {
        tail_pos.saturating_sub(1)
    } else {
        return;
    };

    let Some(prev) = scan::prev_non_ws_byte(source, dot_pos.saturating_add(1)) else {
        return;
    };
    if prev != b'.' {
        return;
    }

    if !dot_access_prefix_looks_expression(source, dot_pos) {
        return;
    }

    candidates.push(RepairCandidate::insert(
        insert_at,
        "field",
        "inserted placeholder member access field",
        PRI_MEMBER_ACCESS_STUB,
    ));
}

fn index_expression_repairs(source: &str, error_pos: usize, insert_at: usize, candidates: &mut Vec<RepairCandidate>) {
    let tail_pos = source.trim_end().len();
    let seek_pos = if error_pos >= tail_pos && source[..tail_pos].ends_with('[') {
        tail_pos
    } else {
        error_pos
    };

    let Some(bracket_open) = find_unclosed_bracket_before(source, seek_pos.saturating_add(1)) else {
        return;
    };

    if !index_open_is_expression_context(source, bracket_open) {
        return;
    }

    let scan_pos = skip_ws(source, bracket_open + 1);
    if scan_pos > source.len() {
        return;
    }
    let scan_end = seek_pos.min(source.len());
    let inside = if scan_end <= scan_pos {
        ""
    } else {
        source[scan_pos..scan_end].trim()
    };
    if inside.is_empty() {
        candidates.push(RepairCandidate::insert(
            insert_at,
            "0]",
            "inserted index placeholder and closed bracket",
            PRI_INDEX_PLACEHOLDER,
        ));
    } else {
        candidates.push(RepairCandidate::insert(
            insert_at,
            "]",
            "closed incomplete index expression",
            PRI_INDEX_CLOSE,
        ));
    }
}

fn control_expression_body_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    parse_error: &pest::error::Error<Rule>,
    candidates: &mut Vec<RepairCandidate>,
) {
    if !syntax_primitives::recovery_expected_or_follow_token_has_body_hint(parse_error)
        && !syntax_primitives::recovery_source_has_fallback_control_flow_hint(
            source,
            error_pos,
            syntax_primitives::CONTROL_EXPRESSION_KEYWORDS,
        )
    {
        return;
    }

    let mut latest = None::<(usize, &str)>;

    for &keyword in syntax_primitives::CONTROL_EXPRESSION_KEYWORDS {
        if let Some(pos) = scan::find_keyword_backward(source, error_pos, keyword) {
            latest = match latest {
                Some((existing_pos, existing_keyword)) if existing_pos > pos => {
                    Some((existing_pos, existing_keyword))
                }
                _ => Some((pos, keyword)),
            };
        }
    }

    let Some((kw_pos, keyword)) = latest else {
        return;
    };

    let prefix = prefix_before(source, kw_pos);
    if prefix.ends_with('.') {
        return;
    }

    if kw_pos + keyword.len() >= source.len() || kw_pos + keyword.len() >= error_pos {
        return;
    }

    let after_keyword = skip_ws(source, kw_pos + keyword.len());
    if source[after_keyword..error_pos].contains('{') {
        return;
    }

    let near_tail = source[after_keyword..error_pos].trim_end();
    if near_tail.is_empty() {
        return;
    }

    candidates.push(RepairCandidate::insert(
        insert_at,
        " { }",
        "inserted placeholder control-expression body",
        PRI_EXPR_CONTROL_BODY,
    ));
}

fn expression_expected_after_operator(parse_error: &pest::error::Error<Rule>) -> bool {
    syntax_primitives::recovery_expected_token_has_any_class(
        parse_error,
        &[
            expected_tokens::ReplacementTokenClass::Identifier,
            expected_tokens::ReplacementTokenClass::Number,
            expected_tokens::ReplacementTokenClass::StringLike,
            expected_tokens::ReplacementTokenClass::Keyword,
            expected_tokens::ReplacementTokenClass::Delimiter,
        ],
    )
}

fn dot_access_prefix_looks_expression(source: &str, dot_pos: usize) -> bool {
    if dot_pos == 0 {
        return false;
    }

    let bytes = source.as_bytes();
    let mut pos = dot_pos;
    pos = pos.saturating_sub(1);
    while pos > 0 && bytes[pos].is_ascii_whitespace() {
        pos -= 1;
    }

    let Some(prev) = bytes.get(pos).copied() else {
        return false;
    };
    if prev == b'.' || prev == b':' || prev == b'(' || prev == b',' {
        return false;
    }

    scan::is_ident_start(prev)
        || prev == b')'
        || prev == b']'
        || prev == b'}'
        || prev == b'"'
        || prev == b'\''
}

fn find_unclosed_bracket_before(source: &str, error_pos: usize) -> Option<usize> {
    syntax_primitives::find_unclosed_delimiter_before(source, error_pos, b'[', b']')
}

fn index_open_is_expression_context(source: &str, open: usize) -> bool {
    let Some(mut pos) = open.checked_sub(1) else {
        return false;
    };

    while pos > 0 {
        let b = source.as_bytes()[pos];
        if b.is_ascii_whitespace() {
            pos -= 1;
            continue;
        }
        return scan::is_ident_continue(b)
            || matches!(b, b')' | b']' | b'}')
            || b.is_ascii_digit();
    }

    false
}

fn struct_field_separator_repairs(
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
        |source, open, scan_pos| struct_brace_opens_literal_at(source, open, scan_pos),
        "field: 0",
        PRI_STRUCT_TRAILING_COMMA_DELETE,
        PRI_STRUCT_TRAILING_COMMA_FIX,
        "removed trailing comma in struct literal field list",
        "inserted placeholder struct field after trailing comma",
    );
}

fn bracket_argument_separator_repairs(
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

fn angle_list_separator_repairs(
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

fn find_match_block_brace(source: &str, through: usize) -> Option<usize> {
    let match_kw = scan::find_keyword_backward(source, through, "match")?;
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
                if scan::is_ident_start(source.as_bytes()[pos]) {
                    pos = scan::skip_identifier(source, pos);
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

fn skip_balanced_delim(source: &str, open_pos: usize, limit: usize, open: u8, close: u8) -> Option<usize> {
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
    if let Some(comma) = slice.rfind(',') { match_brace + 1 + comma + 1 } else { match_brace + 1 }
}

fn arm_segment_looks_like_pattern(segment: &str) -> bool {
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('_') {
        return true;
    }
    trimmed.chars().any(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '.' || c == '(')
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
        return b == b')' || scan::is_ident_continue(b);
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
    (open > 0 && scan::is_ident_continue(source.as_bytes()[open - 1])) || prefix.ends_with('!') || prefix.ends_with("spawn")
}

fn inside_expression_argument_list(source: &str, open: usize, error_pos: usize) -> bool {
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

fn find_unclosed_paren_before(source: &str, error_pos: usize) -> Option<usize> {
    syntax_primitives::find_unclosed_delimiter_before(source, error_pos, b'(', b')')
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
