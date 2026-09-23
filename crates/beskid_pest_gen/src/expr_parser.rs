use crate::ast::{Expr, RepeatKind};

pub(crate) fn parse_expr(input: &str) -> Result<Expr, String> {
    parse_choice(input)
}

fn parse_choice(input: &str) -> Result<Expr, String> {
    let items = split_many(input, '|')?;
    let mut parsed = Vec::new();
    for item in items {
        parsed.push(parse_seq(item)?);
    }
    if parsed.len() == 1 { Ok(parsed.into_iter().next().unwrap()) } else { Ok(Expr::Choice(parsed)) }
}

fn parse_seq(input: &str) -> Result<Expr, String> {
    let items = split_many(input, '~')?;
    let mut parsed = Vec::new();
    for item in items {
        parsed.push(parse_repeat(item)?);
    }
    if parsed.len() == 1 { Ok(parsed.into_iter().next().unwrap()) } else { Ok(Expr::Seq(parsed)) }
}

fn parse_repeat(input: &str) -> Result<Expr, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty repeat operand".to_string());
    }
    if let Some(inner) = trimmed.strip_suffix('*') {
        return Ok(Expr::Repeat(Box::new(parse_primary(inner.trim())?), RepeatKind::ZeroOrMore));
    }
    if let Some(inner) = trimmed.strip_suffix('+') {
        return Ok(Expr::Repeat(Box::new(parse_primary(inner.trim())?), RepeatKind::OneOrMore));
    }
    if let Some(inner) = trimmed.strip_suffix('?') {
        return Ok(Expr::Opt(Box::new(parse_primary(inner.trim())?)));
    }
    parse_primary(trimmed)
}

fn parse_primary(input: &str) -> Result<Expr, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty primary".to_string());
    }
    if trimmed == "ANY" {
        return Ok(Expr::Any);
    }
    if let Some(inner) = trimmed.strip_prefix('!') {
        return Ok(Expr::Not(Box::new(parse_primary(inner.trim())?)));
    }
    if trimmed.starts_with('"') {
        return parse_string_literal(trimmed);
    }
    if trimmed.starts_with('(') {
        let inner = strip_parens(trimmed)?;
        return Ok(Expr::Group(Box::new(parse_expr(inner)?)));
    }
    if is_ident(trimmed) {
        return Ok(Expr::RuleRef(trimmed.to_string()));
    }
    Err(format!("unsupported primary `{trimmed}`"))
}

fn parse_string_literal(input: &str) -> Result<Expr, String> {
    let mut out = String::new();
    let mut chars = input.chars();
    if chars.next() != Some('"') {
        return Err("expected opening quote".to_string());
    }
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let esc = chars.next().ok_or_else(|| "unfinished escape".to_string())?;
            match esc {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                other => {
                    out.push('\\');
                    out.push(other);
                }
            }
            continue;
        }
        if ch == '"' {
            return Ok(Expr::Literal(out));
        }
        out.push(ch);
    }
    Err("unterminated string literal".to_string())
}

fn strip_parens(input: &str) -> Result<&str, String> {
    let trimmed = input.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return Err("expected parenthesized expression".to_string());
    }
    let mut depth = 0i32;
    let bytes = trimmed.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    if index + 1 != bytes.len() {
                        return Err("trailing tokens after group".to_string());
                    }
                    return Ok(trimmed[1..index].trim());
                }
            }
            _ => {}
        }
    }
    Err("unbalanced parentheses".to_string())
}

fn split_many(input: &str, sep: char) -> Result<Vec<&str>, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty expression".to_string());
    }
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut in_string = false;
    let chars: Vec<char> = trimmed.chars().collect();
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        if in_string {
            if ch == '\\' {
                index += 2;
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c == sep && depth == 0 => {
                parts.push(trimmed[start..byte_index(trimmed, index)].trim());
                start = byte_index(trimmed, index + 1);
            }
            _ => {}
        }
        index += 1;
    }
    if in_string {
        return Err("unterminated string literal".to_string());
    }
    if depth != 0 {
        return Err("unbalanced parentheses".to_string());
    }
    parts.push(trimmed[start..].trim());
    parts.retain(|part| !part.is_empty());
    if parts.is_empty() {
        return Err("empty expression".to_string());
    }
    Ok(parts)
}

fn byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices().nth(char_index).map(|(index, _)| index).unwrap_or(text.len())
}

fn is_ident(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
