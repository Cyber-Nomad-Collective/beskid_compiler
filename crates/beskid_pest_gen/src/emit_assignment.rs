use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::ast::{Expr, GrammarRule, RepeatKind};
use crate::emit::indent_block;
use crate::rule_name::rule_name_to_callable;
use crate::strings::is_beskid_string_representable;

pub(crate) fn emit_step_on_cursor(
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
                crate::strings::escape_beskid_string(value)
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
            format!("{target} = Parser.Literal({cursor}, \"{}\", \"{rule}\");", crate::strings::escape_beskid_string(value))
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
