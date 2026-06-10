//! Emit Beskid combinator parsers from a constrained Pest grammar surface.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// A parsed grammar rule (minimal Pest subset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarRule {
    pub name: String,
    pub expression: String,
}

/// Parse simple `name = { expr }` rules from `.pest` source (one rule per line block).
pub fn parse_grammar_rules(source: &str) -> Result<Vec<GrammarRule>, String> {
    let mut rules = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_expr = String::new();

    for line in source.lines() {
        let trimmed = line.split("//").next().unwrap_or("").trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((name, expr)) = trimmed.split_once('=') {
            if let Some(prev) = current_name.take() {
                rules.push(GrammarRule {
                    name: prev,
                    expression: current_expr.trim().to_string(),
                });
                current_expr.clear();
            }
            current_name = Some(name.trim().to_string());
            let expr_part = expr.trim().trim_start_matches('{').trim();
            if expr_part.ends_with('}') {
                rules.push(GrammarRule {
                    name: name.trim().to_string(),
                    expression: expr_part.trim_end_matches('}').trim().to_string(),
                });
                current_name = None;
                current_expr.clear();
            } else {
                current_expr.push_str(expr_part);
            }
        } else if current_name.is_some() {
            let part = trimmed.trim_end_matches('}').trim();
            if !current_expr.is_empty() {
                current_expr.push(' ');
            }
            current_expr.push_str(part);
            if trimmed.ends_with('}')
                && let Some(prev) = current_name.take()
            {
                rules.push(GrammarRule {
                    name: prev,
                    expression: current_expr.trim().to_string(),
                });
                current_expr.clear();
            }
        }
    }
    if let Some(prev) = current_name {
        rules.push(GrammarRule {
            name: prev,
            expression: current_expr.trim().to_string(),
        });
    }
    if rules.is_empty() {
        return Err("no grammar rules found".to_string());
    }
    Ok(rules)
}

/// Map a Pest snake_case rule name to a PascalCase callable (`lower_run` → `ParseLowerRun`).
pub fn rule_name_to_callable(rule_name: &str) -> String {
    let mut parts = rule_name.split('_').filter(|part| !part.is_empty());
    let Some(first) = parts.next() else {
        return "Parse".to_string();
    };
    let mut pascal = String::new();
    pascal.push_str(&capitalize_ascii(first));
    for part in parts {
        pascal.push_str(&capitalize_ascii(part));
    }
    format!("Parse{pascal}")
}

fn capitalize_ascii(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::new();
    out.extend(first.to_uppercase());
    out.extend(chars);
    out
}

/// Emit a Beskid module that wraps combinator calls for each rule.
pub fn emit_combinator_module(module_name: &str, rules: &[GrammarRule]) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "// CHECKED IN: materialized by corelib_pest_gen (PascalCase emit per PARSER-005)"
    )
    .unwrap();
    writeln!(
        out,
        "/// Generated combinator parser module for `{module_name}`."
    )
    .unwrap();
    writeln!(out, "use Core.Text.Cursor;").unwrap();
    writeln!(out, "use Core.Text.Parser;").unwrap();
    writeln!(out).unwrap();

    let mut by_name = BTreeMap::new();
    for rule in rules {
        by_name.insert(rule.name.as_str(), rule);
    }

    for rule in rules {
        let callable = rule_name_to_callable(&rule.name);
        let body = emit_rule_body(&rule.expression, &rule.name, &by_name);
        writeln!(
            out,
            "pub Parser.TextParseResult<string> {callable}(Cursor.TextCursor c) {{\n{body}\n}}\n",
            body = indent_block(&body, 4)
        )
        .unwrap();
    }
    out
}

fn indent_block(body: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    body.lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{pad}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn emit_rule_body(expr: &str, rule: &str, rules: &BTreeMap<&str, &GrammarRule>) -> String {
    match parse_expr(expr) {
        Ok(node) => emit_node(&node, rule, rules),
        Err(message) => format!(
            "return Parser.Fail(c, Parser.ParseErrorKind::ExpectedRule, \"{rule}: {message}\");"
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expr {
    Literal(String),
    RuleRef(String),
    Any,
    Seq(Vec<Expr>),
    Choice(Vec<Expr>),
    Repeat(Box<Expr>, RepeatKind),
    Opt(Box<Expr>),
    Group(Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepeatKind {
    ZeroOrMore,
    OneOrMore,
}

fn parse_expr(input: &str) -> Result<Expr, String> {
    parse_choice(input)
}

fn parse_choice(input: &str) -> Result<Expr, String> {
    let items = split_many(input, '|')?;
    let mut parsed = Vec::new();
    for item in items {
        parsed.push(parse_seq(item)?);
    }
    if parsed.len() == 1 {
        Ok(parsed.into_iter().next().unwrap())
    } else {
        Ok(Expr::Choice(parsed))
    }
}

fn parse_seq(input: &str) -> Result<Expr, String> {
    let items = split_many(input, '~')?;
    let mut parsed = Vec::new();
    for item in items {
        parsed.push(parse_repeat(item)?);
    }
    if parsed.len() == 1 {
        Ok(parsed.into_iter().next().unwrap())
    } else {
        Ok(Expr::Seq(parsed))
    }
}

fn parse_repeat(input: &str) -> Result<Expr, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty repeat operand".to_string());
    }
    if let Some(inner) = trimmed.strip_suffix('*') {
        return Ok(Expr::Repeat(
            Box::new(parse_primary(inner.trim())?),
            RepeatKind::ZeroOrMore,
        ));
    }
    if let Some(inner) = trimmed.strip_suffix('+') {
        return Ok(Expr::Repeat(
            Box::new(parse_primary(inner.trim())?),
            RepeatKind::OneOrMore,
        ));
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
            let esc = chars
                .next()
                .ok_or_else(|| "unfinished escape".to_string())?;
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
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
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

fn emit_node(node: &Expr, rule: &str, rules: &BTreeMap<&str, &GrammarRule>) -> String {
    match node {
        Expr::Literal(value) => emit_literal_expr(value, "c", rule, "return"),
        Expr::RuleRef(name) => {
            if rules.contains_key(name.as_str()) {
                format!("return {}(c);", rule_name_to_callable(name))
            } else {
                format!(
                    "return Parser.Fail(c, Parser.ParseErrorKind::ExpectedRule, \"{rule}: unknown rule {name}\");"
                )
            }
        }
        Expr::Any => format!("return Parser.Satisfy(c, \"{rule}\");"),
        Expr::Group(inner) => emit_node(inner, rule, rules),
        Expr::Opt(inner) => {
            let inner_code = emit_node_on_cursor("c", inner, rule, rules);
            format!(
                "{inner_code}\nif Parser.IsOk(opt_inner) {{\n    return opt_inner;\n}}\nreturn Parser.Pure(\"\", c);"
            )
        }
        Expr::Repeat(inner, RepeatKind::ZeroOrMore) => emit_many(inner, rule, rules, false),
        Expr::Repeat(inner, RepeatKind::OneOrMore) => emit_many(inner, rule, rules, true),
        Expr::Seq(parts) => emit_seq(parts, rule, rules),
        Expr::Choice(parts) => emit_choice(parts, rule, rules),
    }
}

fn emit_node_on_cursor(
    cursor: &str,
    node: &Expr,
    rule: &str,
    rules: &BTreeMap<&str, &GrammarRule>,
) -> String {
    match node {
        Expr::Literal(value) => emit_literal_expr(value, cursor, rule, "opt_inner"),
        Expr::RuleRef(name) => {
            format!(
                "Parser.TextParseResult<string> opt_inner = {}({cursor});",
                rule_name_to_callable(name)
            )
        }
        Expr::Any => format!(
            "Parser.TextParseResult<string> opt_inner = Parser.Satisfy({cursor}, \"{rule}\");"
        ),
        Expr::Group(inner) => emit_node_on_cursor(cursor, inner, rule, rules),
        _ => {
            let body = emit_node(node, rule, rules);
            format!(
                "Parser.TextParseResult<string> opt_inner = {{\n    {body}\n}};"
            )
        }
    }
}

fn emit_seq(parts: &[Expr], rule: &str, rules: &BTreeMap<&str, &GrammarRule>) -> String {
    if parts.is_empty() {
        return "return Parser.Pure(\"\", c);".to_string();
    }
    if parts.len() == 1 {
        return emit_node(&parts[0], rule, rules);
    }
    let mut out = String::new();
    writeln!(out, "Cursor.TextCursor seqCur = c;").unwrap();
    for (index, part) in parts.iter().enumerate() {
        let var = format!("seq{index}");
        let step = emit_step_on_cursor("seqCur", part, rule, rules, &var);
        writeln!(out, "{step}").unwrap();
        writeln!(out, "if !Parser.IsOk({var}) {{").unwrap();
        writeln!(out, "    return {var};").unwrap();
        writeln!(out, "}}").unwrap();
        writeln!(out, "    seqCur = Parser.RestOnOk({var}, seqCur);").unwrap();
    }
    writeln!(out, "return Parser.Pure(\"\", seqCur);").unwrap();
    out.trim_end().to_string()
}

fn emit_choice(parts: &[Expr], rule: &str, rules: &BTreeMap<&str, &GrammarRule>) -> String {
    if parts.is_empty() {
        return format!("return Parser.Fail(c, Parser.ParseErrorKind::ChoiceFailed, \"{rule}\");");
    }
    if parts.len() == 1 {
        return emit_node(&parts[0], rule, rules);
    }
    let mut out = String::new();
    for (index, part) in parts.iter().enumerate() {
        let var = format!("choice{index}");
        let step = emit_step_on_cursor("c", part, rule, rules, &var);
        writeln!(out, "{step}").unwrap();
        writeln!(out, "if Parser.IsOk({var}) {{").unwrap();
        writeln!(out, "    return {var};").unwrap();
        writeln!(out, "}}").unwrap();
    }
    writeln!(
        out,
        "return Parser.Fail(c, Parser.ParseErrorKind::ChoiceFailed, \"{rule}\");"
    )
    .unwrap();
    out.trim_end().to_string()
}

fn emit_many(
    inner: &Expr,
    rule: &str,
    rules: &BTreeMap<&str, &GrammarRule>,
    one_or_more: bool,
) -> String {
    let step = emit_step_on_cursor("manyCur", inner, rule, rules, "manyStep");
    let mut out = String::new();
    writeln!(out, "Cursor.TextCursor manyCur = c;").unwrap();
    writeln!(out, "i64 manyCount = 0;").unwrap();
    writeln!(out, "while true {{").unwrap();
    writeln!(out, "    i64 manyPos = Cursor.Position(manyCur);").unwrap();
    writeln!(out, "    {step}").unwrap();
    writeln!(out, "    if !Parser.IsOk(manyStep) {{").unwrap();
    writeln!(out, "        break;").unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(
        out,
        "    Cursor.TextCursor manyRest = Parser.RestOnOk(manyStep, manyCur);"
    )
    .unwrap();
    writeln!(out, "    if Cursor.Position(manyRest) == manyPos {{").unwrap();
    writeln!(
        out,
        "        return Parser.Fail(manyCur, Parser.ParseErrorKind::ZeroWidthRepeat, \"{rule}\");"
    )
    .unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out, "    manyCur = manyRest;").unwrap();
    writeln!(out, "    manyCount = manyCount + 1;").unwrap();
    writeln!(out, "}}").unwrap();
    if one_or_more {
        writeln!(out, "if manyCount < 1 {{").unwrap();
        writeln!(
            out,
            "    return Parser.Fail(c, Parser.ParseErrorKind::ExpectedRule, \"{rule}\");"
        )
        .unwrap();
        writeln!(out, "}}").unwrap();
    }
    writeln!(out, "return Parser.Pure(\"\", manyCur);").unwrap();
    out.trim_end().to_string()
}

fn emit_step_on_cursor(
    cursor: &str,
    node: &Expr,
    rule: &str,
    rules: &BTreeMap<&str, &GrammarRule>,
    var: &str,
) -> String {
    match node {
        Expr::Literal(value) => emit_literal_expr(value, cursor, rule, var),
        Expr::RuleRef(name) => {
            format!(
                "Parser.TextParseResult<string> {var} = {}({cursor});",
                rule_name_to_callable(name)
            )
        }
        Expr::Any => format!(
            "Parser.TextParseResult<string> {var} = Parser.Satisfy({cursor}, \"{rule}\");"
        ),
        Expr::Group(inner) => {
            let body = emit_node(inner, rule, rules).replace("return ", "");
            format!("Parser.TextParseResult<string> {var} = {{ {body} }};")
        }
        _ => {
            let body = emit_node(node, rule, rules).replace("return ", "");
            format!("Parser.TextParseResult<string> {var} = {{ {body} }};")
        }
    }
}

fn emit_literal_expr(value: &str, cursor: &str, rule: &str, target: &str) -> String {
    if !is_beskid_string_representable(value) {
        return format!(
            "Parser.TextParseResult<string> {target} = Parser.Fail({cursor}, Parser.ParseErrorKind::ExpectedLiteral, \"{rule}: unrepresentable literal\");"
        );
    }
    let escaped = escape_beskid_string(value);
    if target == "return" {
        format!("return Parser.Literal({cursor}, \"{escaped}\", \"{rule}\");")
    } else {
        format!(
            "Parser.TextParseResult<string> {target} = Parser.Literal({cursor}, \"{escaped}\", \"{rule}\");"
        )
    }
}

fn is_beskid_string_representable(value: &str) -> bool {
    value.chars().all(|ch| {
        ch == ' ' || (ch.is_ascii_graphic() && ch != '"' && ch != '\\' && ch != '$')
    })
}

fn escape_beskid_string(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_rule() {
        let src = r#"digit = { "0" | "1" }"#;
        let rules = parse_grammar_rules(src).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "digit");
    }

    #[test]
    fn emits_literal_parser() {
        let rules = vec![GrammarRule {
            name: "hi".to_string(),
            expression: "\"hello\"".to_string(),
        }];
        let out = emit_combinator_module("test", &rules);
        assert!(out.contains("ParseHi"));
        assert!(out.contains("Parser.Literal"));
        assert!(out.contains("Parser.TextParseResult"));
    }

    #[test]
    fn parses_optional_and_group() {
        let expr = parse_expr(r#"( "a" | "b" )? "#).unwrap();
        assert!(matches!(expr, Expr::Opt(_)));
    }

    #[test]
    fn parses_regex_grammar_rules() {
        let src = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corelib/packages/foundation/grammars/regex.pest"
        ));
        let rules = parse_grammar_rules(src).expect("regex.pest should parse");
        assert!(rules.iter().any(|rule| rule.name == "pat"));
        assert!(rules.iter().any(|rule| rule.name == "lower_run"));
    }

    #[test]
    fn rule_name_to_callable_maps_snake_case() {
        assert_eq!(rule_name_to_callable("lower_run"), "ParseLowerRun");
        assert_eq!(rule_name_to_callable("digit"), "ParseDigit");
        assert_eq!(rule_name_to_callable("pat_branch"), "ParsePatBranch");
    }

    #[test]
    fn emits_choice_backtracking() {
        let rules = vec![GrammarRule {
            name: "pick".to_string(),
            expression: "\"a\" | \"b\"".to_string(),
        }];
        let out = emit_combinator_module("test", &rules);
        assert!(out.contains("ChoiceFailed"));
        assert!(out.contains("if Parser.IsOk(choice0)"));
    }
}
