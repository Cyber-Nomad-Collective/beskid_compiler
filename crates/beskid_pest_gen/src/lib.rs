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
                rules.push(GrammarRule { name: prev, expression: current_expr.trim().to_string() });
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
                rules.push(GrammarRule { name: prev, expression: current_expr.trim().to_string() });
                current_expr.clear();
            }
        }
    }
    if let Some(prev) = current_name {
        rules.push(GrammarRule { name: prev, expression: current_expr.trim().to_string() });
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
    writeln!(out, "// CHECKED IN: materialized by corelib_pest_gen (PascalCase emit per PARSER-005)").unwrap();
    writeln!(out, "/// Generated combinator parser module for `{module_name}`.").unwrap();
    writeln!(out, "use Core.Text.Cursor;").unwrap();
    writeln!(out, "use Core.Text.Parser;").unwrap();
    writeln!(out, "use Core.Text.Parser.Terms;").unwrap();
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
            "pub Parser.Result.TextParseResult<string> {callable}(Cursor.TextCursor c) {{\n{body}\n}}\n",
            body = indent_block(&body, 4)
        )
        .unwrap();
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

fn indent_block(body: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    body.lines()
        .map(|line| if line.is_empty() { String::new() } else { format!("{pad}{line}") })
        .collect::<Vec<_>>()
        .join("\n")
}

fn emit_rule_body(expr: &str, rule: &str, rules: &BTreeMap<&str, &GrammarRule>) -> String {
    if let Some(term) = builtin_term(expr, rule) {
        return format!("return Core.Text.Parser.Terms.{term}(c, \"{rule}\");");
    }
    match parse_expr(expr) {
        Ok(node) => emit_node(&node, rule, rules),
        Err(message) => format!(
            "return Parser.Fail(c, Parser.Result.ParseErrorKind::ExpectedRule(\"{rule}: {message}\"), \"{rule}: {message}\");"
        ),
    }
}

fn builtin_term(expr: &str, rule: &str) -> Option<&'static str> {
    let (alphabet, term) = match rule {
        "digit" => ("0123456789", "Digit"),
        "lower" => ("abcdefghijklmnopqrstuvwxyz", "Lower"),
        "upper" => ("ABCDEFGHIJKLMNOPQRSTUVWXYZ", "Upper"),
        _ => return None,
    };
    let Expr::Choice(parts) = parse_expr(expr).ok()? else {
        return None;
    };
    let literals = parts
        .iter()
        .map(|part| match part {
            Expr::Literal(value) => Some(value.as_str()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    (literals.len() == alphabet.len()
        && literals.iter().zip(alphabet.chars()).all(|(value, ch)| *value == ch.to_string()))
    .then_some(term)
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
    Not(Box<Expr>),
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

fn emit_node(node: &Expr, rule: &str, rules: &BTreeMap<&str, &GrammarRule>) -> String {
    emit_node_from_cursor("c", node, rule, rules)
}

fn emit_node_from_cursor(cursor: &str, node: &Expr, rule: &str, rules: &BTreeMap<&str, &GrammarRule>) -> String {
    match node {
        Expr::Literal(value) => emit_literal_expr(value, cursor, rule, "return"),
        Expr::RuleRef(name) => {
            if rules.contains_key(name.as_str()) {
                format!("return {}({cursor});", rule_name_to_callable(name))
            } else {
                format!(
                    "return Parser.Fail({cursor}, Parser.Result.ParseErrorKind::ExpectedRule(\"{rule}: unknown rule {name}\"), \"{rule}: unknown rule {name}\");"
                )
            }
        }
        Expr::Any => format!("return Parser.Satisfy({cursor}, \"{rule}\");"),
        Expr::Group(inner) => emit_node_from_cursor(cursor, inner, rule, rules),
        Expr::Opt(inner) => {
            let inner_code = emit_node_on_cursor(cursor, inner, rule, rules);
            format!(
                "{inner_code}\nif Parser.IsOk(optInner) {{\n    return optInner;\n}}\nreturn Parser.Pure(\"\", {cursor});"
            )
        }
        Expr::Not(inner) => {
            let inner_code = emit_step_on_cursor(cursor, inner, rule, rules, "notInner");
            format!(
                "{inner_code}\nif Parser.IsOk(notInner) {{\n    return Parser.Fail({cursor}, Parser.Result.ParseErrorKind::ExpectedRule(\"{rule}\"), \"{rule}\");\n}}\nreturn Parser.Pure(\"\", {cursor});"
            )
        }
        Expr::Repeat(inner, RepeatKind::ZeroOrMore) => emit_many(cursor, inner, rule, rules, false),
        Expr::Repeat(inner, RepeatKind::OneOrMore) => emit_many(cursor, inner, rule, rules, true),
        Expr::Seq(parts) => emit_seq(cursor, parts, rule, rules),
        Expr::Choice(parts) => emit_choice(cursor, parts, rule, rules),
    }
}

fn emit_node_on_cursor(cursor: &str, node: &Expr, rule: &str, rules: &BTreeMap<&str, &GrammarRule>) -> String {
    match node {
        Expr::Literal(value) => emit_literal_expr(value, cursor, rule, "optInner"),
        Expr::RuleRef(name) => {
            format!("Parser.Result.TextParseResult<string> optInner = {}({cursor});", rule_name_to_callable(name))
        }
        Expr::Any => format!("Parser.Result.TextParseResult<string> optInner = Parser.Satisfy({cursor}, \"{rule}\");"),
        Expr::Group(inner) => emit_node_on_cursor(cursor, inner, rule, rules),
        _ => {
            let body = emit_node_from_cursor(cursor, node, rule, rules);
            format!("Parser.Result.TextParseResult<string> optInner = {{\n    {body}\n}};")
        }
    }
}

fn emit_seq(cursor: &str, parts: &[Expr], rule: &str, rules: &BTreeMap<&str, &GrammarRule>) -> String {
    if parts.is_empty() {
        return format!("return Parser.Pure(\"\", {cursor});");
    }
    if parts.len() == 1 {
        return emit_node_from_cursor(cursor, &parts[0], rule, rules);
    }
    let mut out = String::new();
    writeln!(out, "mut Cursor.TextCursor seqCur = {cursor};").unwrap();
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

fn emit_choice(cursor: &str, parts: &[Expr], rule: &str, rules: &BTreeMap<&str, &GrammarRule>) -> String {
    if parts.is_empty() {
        return format!(
            "return Parser.Fail({cursor}, Parser.Result.ParseErrorKind::ChoiceFailed(\"{rule}\"), \"{rule}\");"
        );
    }
    if parts.len() == 1 {
        return emit_node_from_cursor(cursor, &parts[0], rule, rules);
    }
    let mut out = String::new();
    for (index, part) in parts.iter().enumerate() {
        let var = format!("choice{index}");
        let step = emit_step_on_cursor(cursor, part, rule, rules, &var);
        writeln!(out, "{step}").unwrap();
        writeln!(out, "if Parser.IsOk({var}) {{").unwrap();
        writeln!(out, "    return {var};").unwrap();
        writeln!(out, "}}").unwrap();
    }
    writeln!(out, "return Parser.Fail({cursor}, Parser.Result.ParseErrorKind::ChoiceFailed(\"{rule}\"), \"{rule}\");")
        .unwrap();
    out.trim_end().to_string()
}

fn emit_many(
    cursor: &str,
    inner: &Expr,
    rule: &str,
    rules: &BTreeMap<&str, &GrammarRule>,
    one_or_more: bool,
) -> String {
    let step = emit_step_on_cursor("manyCur", inner, rule, rules, "manyStep");
    let mut out = String::new();
    writeln!(out, "mut Cursor.TextCursor manyCur = {cursor};").unwrap();
    writeln!(out, "mut i64 manyCount = 0;").unwrap();
    writeln!(out, "while true {{").unwrap();
    writeln!(out, "    i64 manyPos = Cursor.Position(manyCur);").unwrap();
    writeln!(out, "    {step}").unwrap();
    writeln!(out, "    if !Parser.IsOk(manyStep) {{").unwrap();
    writeln!(out, "        break;").unwrap();
    writeln!(out, "    }}").unwrap();
    writeln!(out, "    Cursor.TextCursor manyRest = Parser.RestOnOk(manyStep, manyCur);").unwrap();
    writeln!(out, "    if Cursor.Position(manyRest) == manyPos {{").unwrap();
    writeln!(
        out,
        "        return Parser.Fail(manyCur, Parser.Result.ParseErrorKind::ZeroWidthRepeat(\"{rule}\"), \"{rule}\");"
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
            "    return Parser.Fail({cursor}, Parser.Result.ParseErrorKind::ExpectedRule(\"{rule}\"), \"{rule}\");"
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
        Expr::Literal(value) if is_beskid_string_representable(value) => {
            return format!(
                "Parser.Result.TextParseResult<string> {var} = Parser.Literal({cursor}, \"{}\", \"{rule}\");",
                escape_beskid_string(value)
            );
        }
        Expr::RuleRef(name) if rules.contains_key(name.as_str()) => {
            return format!("Parser.Result.TextParseResult<string> {var} = {}({cursor});", rule_name_to_callable(name));
        }
        Expr::Any => {
            return format!("Parser.Result.TextParseResult<string> {var} = Parser.Satisfy({cursor}, \"{rule}\");");
        }
        Expr::Group(inner) => return emit_step_on_cursor(cursor, inner, rule, rules, var),
        _ => {}
    }
    let mut out = format!("mut Parser.Result.TextParseResult<string> {var} = Parser.Pure(\"\", {cursor});\n");
    out.push_str(&emit_assignment_on_cursor(cursor, node, rule, rules, var));
    out
}

fn emit_assignment_on_cursor(
    cursor: &str,
    node: &Expr,
    rule: &str,
    rules: &BTreeMap<&str, &GrammarRule>,
    target: &str,
) -> String {
    match node {
        Expr::Literal(value) => {
            if !is_beskid_string_representable(value) {
                return format!(
                    "{target} = Parser.Fail({cursor}, Parser.Result.ParseErrorKind::ExpectedLiteral(\"{rule}: unrepresentable literal\"), \"{rule}: unrepresentable literal\");"
                );
            }
            format!("{target} = Parser.Literal({cursor}, \"{}\", \"{rule}\");", escape_beskid_string(value))
        }
        Expr::RuleRef(name) if rules.contains_key(name.as_str()) => {
            format!("{target} = {}({cursor});", rule_name_to_callable(name))
        }
        Expr::RuleRef(name) => format!(
            "{target} = Parser.Fail({cursor}, Parser.Result.ParseErrorKind::ExpectedRule(\"{rule}: unknown rule {name}\"), \"{rule}: unknown rule {name}\");"
        ),
        Expr::Any => format!("{target} = Parser.Satisfy({cursor}, \"{rule}\");"),
        Expr::Group(inner) => emit_assignment_on_cursor(cursor, inner, rule, rules, target),
        Expr::Opt(inner) => {
            let inner = emit_assignment_on_cursor(cursor, inner, rule, rules, target);
            format!("{inner}\nif !Parser.IsOk({target}) {{\n    {target} = Parser.Pure(\"\", {cursor});\n}}")
        }
        Expr::Not(inner) => {
            let inner = emit_assignment_on_cursor(cursor, inner, rule, rules, target);
            format!(
                "{inner}\nif Parser.IsOk({target}) {{\n    {target} = Parser.Fail({cursor}, Parser.Result.ParseErrorKind::ExpectedRule(\"{rule}\"), \"{rule}\");\n}} else {{\n    {target} = Parser.Pure(\"\", {cursor});\n}}"
            )
        }
        Expr::Seq(parts) => emit_seq_assignment(cursor, parts, rule, rules, target),
        Expr::Choice(parts) => emit_choice_assignment(cursor, parts, rule, rules, target),
        Expr::Repeat(inner, kind) => {
            emit_repeat_assignment(cursor, inner, rule, rules, target, *kind == RepeatKind::OneOrMore)
        }
    }
}

fn emit_seq_assignment(
    cursor: &str,
    parts: &[Expr],
    rule: &str,
    rules: &BTreeMap<&str, &GrammarRule>,
    target: &str,
) -> String {
    let seq_cursor = format!("{target}SeqCur");
    let mut out = format!("mut Cursor.TextCursor {seq_cursor} = {cursor};");
    for part in parts {
        let assignment = indent_block(&emit_assignment_on_cursor(&seq_cursor, part, rule, rules, target), 4);
        writeln!(out).unwrap();
        writeln!(out, "if Parser.IsOk({target}) {{").unwrap();
        writeln!(out, "{assignment}").unwrap();
        writeln!(out, "    if Parser.IsOk({target}) {{").unwrap();
        writeln!(out, "        {seq_cursor} = Parser.RestOnOk({target}, {seq_cursor});").unwrap();
        write!(out, "    }}\n}}").unwrap();
    }
    out
}

fn emit_choice_assignment(
    cursor: &str,
    parts: &[Expr],
    rule: &str,
    rules: &BTreeMap<&str, &GrammarRule>,
    target: &str,
) -> String {
    let mut out = format!(
        "{target} = Parser.Fail({cursor}, Parser.Result.ParseErrorKind::ChoiceFailed(\"{rule}\"), \"{rule}\");"
    );
    for part in parts {
        let assignment = indent_block(&emit_assignment_on_cursor(cursor, part, rule, rules, target), 4);
        writeln!(out).unwrap();
        writeln!(out, "if !Parser.IsOk({target}) {{").unwrap();
        write!(out, "{assignment}\n}}").unwrap();
    }
    out
}

fn emit_repeat_assignment(
    cursor: &str,
    inner: &Expr,
    rule: &str,
    rules: &BTreeMap<&str, &GrammarRule>,
    target: &str,
    one_or_more: bool,
) -> String {
    let repeat_cursor = format!("{target}ManyCur");
    let repeat_count = format!("{target}ManyCount");
    let repeat_pos = format!("{target}ManyPos");
    let repeat_rest = format!("{target}ManyRest");
    let assignment = indent_block(&emit_assignment_on_cursor(&repeat_cursor, inner, rule, rules, target), 4);
    let mut out = format!(
        "mut Cursor.TextCursor {repeat_cursor} = {cursor};\nmut i64 {repeat_count} = 0;\nwhile Parser.IsOk({target}) {{\n    i64 {repeat_pos} = Cursor.Position({repeat_cursor});\n{assignment}\n    if !Parser.IsOk({target}) {{\n        {target} = Parser.Pure(\"\", {repeat_cursor});\n        break;\n    }}\n    Cursor.TextCursor {repeat_rest} = Parser.RestOnOk({target}, {repeat_cursor});\n    if Cursor.Position({repeat_rest}) == {repeat_pos} {{\n        {target} = Parser.Fail({repeat_cursor}, Parser.Result.ParseErrorKind::ZeroWidthRepeat(\"{rule}\"), \"{rule}\");\n        break;\n    }}\n    {repeat_cursor} = {repeat_rest};\n    {repeat_count} = {repeat_count} + 1;\n}}"
    );
    if one_or_more {
        write!(out, "\nif Parser.IsOk({target}) && {repeat_count} < 1 {{\n    {target} = Parser.Fail({cursor}, Parser.Result.ParseErrorKind::ExpectedRule(\"{rule}\"), \"{rule}\");\n}}").unwrap();
    }
    out
}

fn emit_literal_expr(value: &str, cursor: &str, rule: &str, target: &str) -> String {
    if !is_beskid_string_representable(value) {
        return format!(
            "Parser.Result.TextParseResult<string> {target} = Parser.Fail({cursor}, Parser.Result.ParseErrorKind::ExpectedLiteral(\"{rule}: unrepresentable literal\"), \"{rule}: unrepresentable literal\");"
        );
    }
    let escaped = escape_beskid_string(value);
    if target == "return" {
        format!("return Parser.Literal({cursor}, \"{escaped}\", \"{rule}\");")
    } else {
        format!("Parser.Result.TextParseResult<string> {target} = Parser.Literal({cursor}, \"{escaped}\", \"{rule}\");")
    }
}

fn is_beskid_string_representable(value: &str) -> bool {
    value.chars().all(|ch| ch == ' ' || ch.is_ascii_graphic())
}

fn escape_beskid_string(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '$' if chars.peek() == Some(&'{') => out.push_str("\\$"),
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
        let rules = vec![GrammarRule { name: "hi".to_string(), expression: "\"hello\"".to_string() }];
        let out = emit_combinator_module("test", &rules);
        assert!(out.contains("ParseHi"));
        assert!(out.contains("Parser.Literal"));
        assert!(out.contains("Parser.Result.TextParseResult"));
        assert!(out.contains("use Core.Text.Parser.Terms;"));
    }

    #[test]
    fn parses_optional_and_group() {
        let expr = parse_expr(r#"( "a" | "b" )? "#).unwrap();
        assert!(matches!(expr, Expr::Opt(_)));
    }

    #[test]
    fn parses_regex_grammar_rules() {
        let src =
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/grammars/regex.pest"));
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
        let rules = vec![GrammarRule { name: "pick".to_string(), expression: "\"a\" | \"b\"".to_string() }];
        let out = emit_combinator_module("test", &rules);
        assert!(out.contains("ChoiceFailed"));
        assert!(out.contains("if Parser.IsOk(choice0)"));
    }

    #[test]
    fn nested_repeat_threads_the_enclosing_sequence_cursor() {
        let rules = vec![GrammarRule { name: "pat".to_string(), expression: r#""a" ~ ("|" ~ "b")*"#.to_string() }];
        let out = emit_combinator_module("test", &rules);

        assert!(out.contains("mut Cursor.TextCursor seqCur = c;"));
        assert!(out.contains("mut Cursor.TextCursor seq1ManyCur = seqCur;"));
        assert!(out.contains("mut Cursor.TextCursor seq1SeqCur = seq1ManyCur;"));
        assert!(!out.contains("Parser.Literal(c, \"|\", \"pat\")"), "nested repeat restarted at c:\n{out}");
    }

    #[test]
    fn nested_optional_preserves_a_successful_cursor_advance() {
        let rules = vec![GrammarRule { name: "maybe".to_string(), expression: r#""a" ~ "b"?"#.to_string() }];
        let out = emit_combinator_module("test", &rules);

        assert!(out.contains("mut Parser.Result.TextParseResult<string> seq1 = Parser.Pure(\"\", seqCur);"));
        assert!(out.contains("seq1 = Parser.Literal(seqCur, \"b\""));
        assert!(out.contains("if !Parser.IsOk(seq1)"));
        assert!(out.contains("seq1 = Parser.Pure(\"\", seqCur);"));
        assert!(!out.contains("seq1 = {"), "optional cursor result was discarded:\n{out}");
    }

    #[test]
    fn negative_lookahead_is_zero_width_and_composable() {
        let rules = vec![GrammarRule { name: "class_char".to_string(), expression: r#"!("]") ~ ANY"#.to_string() }];
        let out = emit_combinator_module("test", &rules);

        assert!(out.contains("seq0 = Parser.Literal(seqCur, \"]\""));
        assert!(out.contains("seq0 = Parser.Pure(\"\", seqCur);"));
        assert!(out.contains("seqCur = Parser.RestOnOk(seq0, seqCur);"));
    }

    #[test]
    fn generated_literals_escape_only_beskid_string_syntax() {
        let rules = vec![
            GrammarRule { name: "anchor".to_string(), expression: r#""$""#.to_string() },
            GrammarRule { name: "interpolation_text".to_string(), expression: r#""${""#.to_string() },
        ];
        let out = emit_combinator_module("test", &rules);

        assert!(out.contains(r#"Parser.Literal(c, "$", "anchor")"#));
        assert!(out.contains(r#"Parser.Literal(c, "\${", "interpolation_text")"#));
        assert!(!out.contains("unrepresentable literal"));
    }
}
