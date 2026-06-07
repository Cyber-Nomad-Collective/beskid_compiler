//! Build a [`BsolDocument`] from pest pairs and top-level block scanning.

use pest::Parser;
use pest::error::InputLocation;
use pest::iterators::Pair;

use crate::ast::{
    BsolAssignment, BsolBlock, BsolBracketList, BsolDocument, BsolItem, BsolListItem,
    BsolQuotedString, BsolSpan, BsolValue,
};
use crate::error::BsolError;
use crate::parser::{BsolParser, Rule};

/// Parse Bsol source into the generic document AST.
pub fn parse_bsol_document(source: &str) -> Result<BsolDocument, BsolError> {
    let mut blocks = Vec::new();
    let mut cursor = 0usize;
    while cursor < source.len() {
        skip_ws_and_comments(source, &mut cursor);
        if cursor >= source.len() {
            break;
        }
        let (block, next) = parse_block_at(source, cursor)?;
        blocks.push(block);
        cursor = next;
    }
    Ok(BsolDocument { blocks })
}

fn parse_block_at(source: &str, start: usize) -> Result<(BsolBlock, usize), BsolError> {
    let mut cursor = start;
    skip_ws_and_comments(source, &mut cursor);
    let kind_start = cursor;
    let kind = read_ident(source, &mut cursor).ok_or_else(|| {
        BsolError::parse_at(
            span_at(source, start, cursor.max(start + 1)),
            "expected block kind identifier",
        )
    })?;
    skip_ws_and_comments(source, &mut cursor);

    let label = if source.as_bytes().get(cursor) == Some(&b'"') {
        let label_start = cursor;
        let value = read_quoted_string(source, &mut cursor).ok_or_else(|| {
            BsolError::parse_at(
                span_at(source, label_start, cursor.max(label_start + 1)),
                "expected quoted block label",
            )
        })?;
        skip_ws_and_comments(source, &mut cursor);
        Some(BsolQuotedString {
            span: span_at(source, label_start, cursor),
            value,
        })
    } else {
        None
    };

    let schemaless = read_schemaless_marker(source, &mut cursor);
    skip_ws_and_comments(source, &mut cursor);
    if source.as_bytes().get(cursor) != Some(&b'{') {
        return Err(BsolError::parse_at(
            span_at(source, kind_start, cursor.max(kind_start + 1)),
            "expected `{` to open block body",
        ));
    }
    let body_open = cursor;
    let body_close = find_matching_close_brace(source, body_open).ok_or_else(|| {
        BsolError::parse_at(
            span_at(source, body_open, source.len()),
            "unclosed block body",
        )
    })?;
    let body_end = body_close - 1;
    let block_end = body_close;
    let span = span_at(source, kind_start, block_end);

    if schemaless {
        let raw = source.get(body_open + 1..body_end).unwrap_or("").to_string();
        return Ok((
            BsolBlock {
                span,
                kind,
                label,
                schemaless_body: Some(raw),
                items: Vec::new(),
            },
            block_end,
        ));
    }

    let items = parse_block_items_in_range(source, body_open + 1, body_end)?;
    Ok((
        BsolBlock {
            span,
            kind,
            label,
            schemaless_body: None,
            items,
        },
        block_end,
    ))
}

fn parse_block_items_in_range(
    source: &str,
    start: usize,
    end: usize,
) -> Result<Vec<BsolItem>, BsolError> {
    let mut items = Vec::new();
    let mut cursor = start;
    while cursor < end {
        skip_ws_and_comments(source, &mut cursor);
        if cursor >= end {
            break;
        }
        let item_start = cursor;
        let Some(_kind) = read_ident(source, &mut cursor) else {
            break;
        };
        skip_ws_and_comments(source, &mut cursor);
        if source.as_bytes().get(cursor) == Some(&b'=') {
            let assign_end = find_assignment_end(source, item_start, end)?;
            items.push(parse_assignment_slice(source, item_start, assign_end)?);
            cursor = assign_end;
            continue;
        }
        let (block, next) = parse_block_at(source, item_start)?;
        if next > end {
            return Err(BsolError::parse_at(
                span_at(source, item_start, end),
                "nested block extends past enclosing body",
            ));
        }
        items.push(BsolItem::Block(block));
        cursor = next;
    }
    Ok(items)
}

fn skip_horizontal_ws(source: &str, cursor: &mut usize) {
    while *cursor < source.len() {
        match source.as_bytes().get(*cursor) {
            Some(b' ' | b'\t') => *cursor += 1,
            _ => break,
        }
    }
}

fn find_assignment_end(source: &str, start: usize, end: usize) -> Result<usize, BsolError> {
    let mut cursor = start;
    if read_ident(source, &mut cursor).is_none() {
        return Err(BsolError::parse_at(
            span_at(source, start, end.max(start + 1)),
            "expected assignment key",
        ));
    }
    skip_horizontal_ws(source, &mut cursor);
    if source.as_bytes().get(cursor) != Some(&b'=') {
        return Err(BsolError::parse_at(
            span_at(source, start, end.max(start + 1)),
            "expected `=` in assignment",
        ));
    }
    cursor += 1;
    skip_horizontal_ws(source, &mut cursor);
    if cursor >= end {
        return Err(BsolError::parse_at(
            span_at(source, start, end),
            "missing assignment value",
        ));
    }
    if source.as_bytes().get(cursor) == Some(&b'"') {
        if read_quoted_string(source, &mut cursor).is_none() {
            return Err(BsolError::parse_at(
                span_at(source, start, end),
                "unclosed string in assignment",
            ));
        }
    } else if source.as_bytes().get(cursor) == Some(&b'[') {
        let bracket_end = find_matching_close_bracket(source, cursor).ok_or_else(|| {
            BsolError::parse_at(span_at(source, start, end), "unclosed bracket list")
        })?;
        cursor = bracket_end;
    } else if read_bare_token(source, &mut cursor).is_none() {
        return Err(BsolError::parse_at(
            span_at(source, start, end),
            "missing assignment value",
        ));
    }
    Ok(cursor.min(end))
}

fn read_bare_token(source: &str, cursor: &mut usize) -> Option<()> {
    let start = *cursor;
    let first = source[*cursor..].chars().next()?;
    if first.is_ascii_digit() {
        while let Some(ch) = source[*cursor..].chars().next() {
            if !ch.is_ascii_digit() {
                break;
            }
            *cursor += ch.len_utf8();
        }
    } else if first.is_ascii_alphabetic() || first == '_' {
        read_ident(source, cursor)?;
    } else {
        return None;
    }
    (*cursor > start).then_some(())
}

fn find_matching_close_bracket(source: &str, open: usize) -> Option<usize> {
    debug_assert_eq!(source.as_bytes().get(open), Some(&b'['));
    let mut depth = 0i32;
    let mut i = open;
    let mut in_string = false;
    while i < source.len() {
        let b = source.as_bytes()[i];
        match b {
            b'"' => in_string = !in_string,
            b'[' if !in_string => depth += 1,
            b']' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn parse_assignment_slice(source: &str, item_start: usize, item_end: usize) -> Result<BsolItem, BsolError> {
    let raw = source.get(item_start..item_end).unwrap_or("");
    let trim_offset = raw.len().saturating_sub(raw.trim_start().len());
    let base = item_start + trim_offset;
    let trimmed = raw.trim();
    let mut pairs = BsolParser::parse(Rule::assignment, trimmed)
        .map_err(|err| pest_error_with_offset(source, base, err))?;
    let pair = pairs
        .next()
        .ok_or_else(|| BsolError::Parse("Bsol parse produced no assignment pair".to_string()))?;
    Ok(BsolItem::Assignment(build_assignment(pair, source, base)?))
}

fn build_assignment(
    pair: Pair<Rule>,
    source: &str,
    base: usize,
) -> Result<BsolAssignment, BsolError> {
    let span = span_at(source, base + pair.as_span().start(), base + pair.as_span().end());
    let mut inner = pair.into_inner();
    let key_pair = inner
        .next()
        .ok_or_else(|| BsolError::parse_at(span, "missing assignment key"))?;
    let key = key_pair.as_str().to_string();
    let value_pair = inner
        .next()
        .ok_or_else(|| BsolError::parse_at(span, "missing assignment value"))?;
    let value = build_value(value_pair, source, base)?;
    Ok(BsolAssignment { span, key, value })
}

fn build_value(
    pair: Pair<Rule>,
    source: &str,
    source_offset: usize,
) -> Result<BsolValue, BsolError> {
    let inner = pair
        .into_inner()
        .next()
        .ok_or_else(|| BsolError::Parse("empty Bsol value".to_string()))?;
    match inner.as_rule() {
        Rule::quoted_string => Ok(BsolValue::QuotedString(build_quoted_string(
            inner, source, source_offset,
        ))),
        Rule::bare_token => Ok(BsolValue::Ident(inner.as_str().to_string())),
        Rule::bracket_list => Ok(BsolValue::BracketList(build_bracket_list(
            inner, source, source_offset,
        )?)),
        other => Err(BsolError::parse_at(
            offset_span(source, source_offset, inner.as_span()),
            format!("unexpected value rule `{other:?}`"),
        )),
    }
}

fn build_quoted_string(
    pair: Pair<Rule>,
    source: &str,
    source_offset: usize,
) -> BsolQuotedString {
    let span = offset_span(source, source_offset, pair.as_span());
    BsolQuotedString::new(span, pair.as_str())
}

fn build_bracket_list(
    pair: Pair<Rule>,
    source: &str,
    source_offset: usize,
) -> Result<BsolBracketList, BsolError> {
    let span = offset_span(source, source_offset, pair.as_span());
    let mut items = Vec::new();
    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::list_content => {
                for item in child.into_inner() {
                    items.push(build_list_item(item, source, source_offset)?);
                }
            }
            Rule::list_item => items.push(build_list_item(child, source, source_offset)?),
            _ => {}
        }
    }
    Ok(BsolBracketList { span, items })
}

fn build_list_item(
    pair: Pair<Rule>,
    source: &str,
    source_offset: usize,
) -> Result<BsolListItem, BsolError> {
    let span = offset_span(source, source_offset, pair.as_span());
    let text = pair.as_str().trim();
    if text == "default" {
        return Ok(BsolListItem::Default);
    }
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::quoted_string => {
                return Ok(BsolListItem::QuotedString(build_quoted_string(
                    inner, source, source_offset,
                )));
            }
            Rule::ident => return Ok(BsolListItem::Ident(inner.as_str().to_string())),
            _ => {}
        }
    }
    Err(BsolError::parse_at(
        span,
        format!("unexpected list item `{text}`"),
    ))
}

fn read_ident(source: &str, cursor: &mut usize) -> Option<String> {
    let start = *cursor;
    let mut chars = source[*cursor..].chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic() && first != '_' {
        return None;
    }
    *cursor += first.len_utf8();
    while let Some(ch) = source[*cursor..].chars().next() {
        if !ch.is_ascii_alphanumeric() && ch != '_' {
            break;
        }
        *cursor += ch.len_utf8();
    }
    Some(source[start..*cursor].to_string())
}

fn read_quoted_string(source: &str, cursor: &mut usize) -> Option<String> {
    if source.as_bytes().get(*cursor) != Some(&b'"') {
        return None;
    }
    let start = *cursor;
    *cursor += 1;
    while *cursor < source.len() {
        let ch = source[*cursor..].chars().next()?;
        if ch == '"' {
            let value = source.get(start + 1..*cursor)?.to_string();
            *cursor += 1;
            return Some(value);
        }
        *cursor += ch.len_utf8();
    }
    None
}

fn read_schemaless_marker(source: &str, cursor: &mut usize) -> bool {
    let marker = "@schemaless";
    if source[*cursor..].starts_with(marker) {
        let next = cursor.saturating_add(marker.len());
        let continues_ident = source[next..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        if !continues_ident {
            *cursor = next;
            return true;
        }
    }
    false
}

fn skip_ws_and_comments(source: &str, cursor: &mut usize) {
    while *cursor < source.len() {
        let remaining = &source[*cursor..];
        if remaining.starts_with("//") {
            if let Some(end) = remaining.find('\n') {
                *cursor += end + 1;
                continue;
            }
            *cursor = source.len();
            break;
        }
        if remaining.starts_with('#') {
            if let Some(end) = remaining.find('\n') {
                *cursor += end + 1;
                continue;
            }
            *cursor = source.len();
            break;
        }
        let Some(ch) = remaining.chars().next() else {
            break;
        };
        if ch.is_whitespace() {
            *cursor += ch.len_utf8();
            continue;
        }
        break;
    }
}

fn find_matching_close_brace(source: &str, open_brace: usize) -> Option<usize> {
    debug_assert_eq!(source.as_bytes().get(open_brace), Some(&b'{'));
    let mut depth = 0i32;
    let mut i = open_brace;
    let mut in_string = false;
    while i < source.len() {
        let b = source.as_bytes()[i];
        match b {
            b'"' => in_string = !in_string,
            b'{' if !in_string => depth += 1,
            b'}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn span_at(source: &str, start: usize, end: usize) -> BsolSpan {
    let start = start.min(source.len());
    let end = end.max(start.saturating_add(1)).min(source.len());
    let pest_span = pest::Span::new(source, start, end)
        .or_else(|| pest::Span::new(source, start, start.saturating_add(1).min(source.len())))
        .expect("span bounds");
    BsolSpan::from_pest(pest_span, source)
}

fn offset_span(source: &str, offset: usize, span: pest::Span<'_>) -> BsolSpan {
    span_at(source, offset + span.start(), offset + span.end())
}

fn pest_error_with_offset(
    source: &str,
    offset: usize,
    err: pest::error::Error<Rule>,
) -> BsolError {
    let start = match err.location {
        InputLocation::Pos(pos) => offset + pos,
        InputLocation::Span((start, _)) => offset + start,
    };
    let line = source[..start.min(source.len())].lines().count().max(1);
    BsolError::ParseAt {
        line,
        message: err.to_string(),
        start: Some(start),
        end: source.get(start..).map(|tail| start + tail.len().min(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_nested_blocks() {
        let src = r#"p {
  name = "p"
  mod {
    maxGeneratorRounds = 4
  }
}
target "t" {
  kind = Lib
}
"#;
        let doc = parse_bsol_document(src).expect("parse");
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(doc.blocks[0].kind, "p");
        assert!(matches!(doc.blocks[0].items[1], BsolItem::Block(_)));
    }

    #[test]
    fn parse_schemaless_block_captures_raw_body() {
        let src = r#"raw @schemaless {
  this is not = valid bsol { but it stays }
  nested { braces } ok
}
"#;
        let doc = parse_bsol_document(src).expect("parse");
        assert_eq!(doc.blocks.len(), 1);
        let block = &doc.blocks[0];
        assert_eq!(block.kind, "raw");
        let body = block.schemaless_body.as_ref().expect("schemaless body");
        assert!(body.contains("this is not = valid bsol"));
        assert!(body.contains("nested { braces }"));
        assert!(block.items.is_empty());
    }

    #[test]
    fn validate_corelib_tests_manifest() {
        let src = include_str!("../../../corelib/beskid_corelib/tests/corelib_tests/corelib_tests.bproj");
        let doc = parse_bsol_document(src).expect("parse corelib_tests");
        let profile = crate::load_profile("project.v1").expect("profile");
        let result = crate::validate::validate(&doc, &profile);
        if let Err(e) = &result {
            eprintln!("validate error: {e}");
            for b in &doc.blocks {
                if b.kind == "target" {
                    for item in &b.items {
                        if let BsolItem::Assignment(a) = item {
                            eprintln!(
                                "  target {:?} {} = {:?}",
                                b.label.as_ref().map(|q| &q.value),
                                a.key,
                                a.value
                            );
                        }
                    }
                }
            }
        }
        result.expect("validate corelib_tests");
    }

    #[test]
    fn validate_corelib_workspace_manifest() {
        let src = include_str!("../../../corelib/CoreLib.bws");
        let doc = parse_bsol_document(src).expect("parse CoreLib.bws");
        let profile = crate::load_profile("workspace.v1").expect("profile");
        crate::validate::validate(&doc, &profile).expect("validate CoreLib.bws");
    }
}
