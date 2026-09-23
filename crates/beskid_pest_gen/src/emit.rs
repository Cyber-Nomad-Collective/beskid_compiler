use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::ast::{Expr, GrammarRule};
use crate::emit_node::emit_node;
use crate::expr_parser::parse_expr;

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
        let callable = crate::rule_name::rule_name_to_callable(&rule.name);
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

pub(crate) fn indent_block(body: &str, spaces: usize) -> String {
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
