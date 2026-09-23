use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::ast::{Expr, GrammarRule, RepeatKind};
use crate::emit_assignment::emit_step_on_cursor;
use crate::rule_name::rule_name_to_callable;
use crate::strings::is_beskid_string_representable;

pub(crate) fn emit_node(node: &Expr, rule: &str, rules: &BTreeMap<&str, &GrammarRule>) -> String {
    emit_node_from_cursor("c", node, rule, rules)
}

pub(crate) fn emit_node_from_cursor(cursor: &str, node: &Expr, rule: &str, rules: &BTreeMap<&str, &GrammarRule>) -> String {
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

pub(crate) fn emit_literal_expr(value: &str, cursor: &str, rule: &str, target: &str) -> String {
    if !is_beskid_string_representable(value) {
        return format!(
            "Parser.Result.TextParseResult<string> {target} = Parser.Fail({cursor}, Parser.Result.ParseErrorKind::ExpectedLiteral(\"{rule}: unrepresentable literal\"), \"{rule}: unrepresentable literal\");"
        );
    }
    let escaped = crate::strings::escape_beskid_string(value);
    if target == "return" {
        format!("return Parser.Literal({cursor}, \"{escaped}\", \"{rule}\");")
    } else {
        format!("Parser.Result.TextParseResult<string> {target} = Parser.Literal({cursor}, \"{escaped}\", \"{rule}\");")
    }
}
