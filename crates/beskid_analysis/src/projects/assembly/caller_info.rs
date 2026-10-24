//! Caller information: `[CallerFilePath]`, `[CallerLineNumber]` and `[CallerColumnNumber]`.
//!
//! A function whose last parameters carry these attributes may be called without them. Before the
//! assembly is indexed, every call that omits exactly those arguments gets them appended as
//! literals: the file of the unit that holds the call (the unit's requested path; renderers map a
//! materialized copy to the authored file), and the 1-based line and column where the callee
//! expression starts. An explicit argument is never replaced, because only a call with exactly the
//! non-caller arguments is rewritten. The model is C# `CallerFilePath`, `CallerLineNumber` and
//! `CallerColumnNumber`.
//!
//! The callee is found from the call path, the unit's `use` declarations and the module graph, so
//! a same-named function elsewhere is never rewritten.

use std::path::{Path, PathBuf};

use super::SourceUnit;
use crate::macros::map_statement;
use crate::syntax::{Expression, Literal, LiteralExpression, Node, Parameter, Spanned, Statement};

const MARKERS: [&str; 3] = ["CallerFilePath", "CallerLineNumber", "CallerColumnNumber"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallerKind {
    File,
    Line,
    Column,
}

struct Provider {
    unit_path: PathBuf,
    logical_name: String,
    name: String,
    /// Number of parameters before the first caller-information parameter.
    explicit_arity: usize,
    kinds: Vec<CallerKind>,
}

fn caller_kind(parameter: &Parameter) -> Option<CallerKind> {
    parameter.attributes.iter().find_map(|attribute| match attribute.node.name.node.name.as_str() {
        "CallerFilePath" => Some(CallerKind::File),
        "CallerLineNumber" => Some(CallerKind::Line),
        "CallerColumnNumber" => Some(CallerKind::Column),
        _ => None,
    })
}

fn collect_providers(units: &[SourceUnit]) -> Vec<Provider> {
    let mut providers = Vec::new();
    for unit in units {
        if !MARKERS.iter().any(|marker| unit.source.contains(marker)) {
            continue;
        }
        for item in &unit.program.node.items {
            let Node::Function(function) = &item.node else { continue };
            let kinds = function.node.parameters.iter().map(|parameter| caller_kind(&parameter.node)).collect::<Vec<_>>();
            let Some(first) = kinds.iter().position(Option::is_some) else { continue };
            if kinds[first..].iter().any(Option::is_none) {
                continue;
            }
            providers.push(Provider {
                unit_path: unit.path.clone(),
                logical_name: unit.logical_name.clone(),
                name: function.node.name.node.name.clone(),
                explicit_arity: first,
                kinds: kinds[first..].iter().flatten().copied().collect(),
            });
        }
    }
    providers
}

/// The units with caller information filled in, or `None` when no call changes.
pub(super) fn fill_caller_information(
    units: &[SourceUnit],
    module_source: &dyn Fn(&[String]) -> Option<PathBuf>,
) -> Option<Vec<SourceUnit>> {
    if !units.iter().any(|unit| MARKERS.iter().any(|marker| unit.source.contains(marker))) {
        return None;
    }
    let providers = collect_providers(units);
    if providers.is_empty() {
        return None;
    }
    let mut changed = false;
    let rewritten = units
        .iter()
        .map(|unit| {
            let mentions = providers
                .iter()
                .any(|provider| unit.source.contains(&format!("{}(", provider.name)) || unit.source.contains(&format!("{}<", provider.name)));
            if !mentions {
                return unit.clone();
            }
            let mut unit = unit.clone();
            changed |= rewrite_unit(&mut unit, &providers, module_source);
            unit
        })
        .collect();
    changed.then_some(rewritten)
}

fn rewrite_unit(
    unit: &mut SourceUnit,
    providers: &[Provider],
    module_source: &dyn Fn(&[String]) -> Option<PathBuf>,
) -> bool {
    let imports = unit
        .program
        .node
        .items
        .iter()
        .filter_map(|item| {
            let Node::UseDeclaration(declaration) = &item.node else { return None };
            let path = declaration.node.path.node.segments.iter().map(|s| s.node.name.node.name.clone()).collect::<Vec<_>>();
            let binding = declaration
                .node
                .alias
                .as_ref()
                .map(|alias| alias.node.name.clone())
                .or_else(|| path.last().cloned())?;
            Some((binding, path))
        })
        .collect::<Vec<_>>();
    let file = unit.origin_path.display().to_string();
    let unit_path = unit.path.clone();
    let mut changed = false;
    let mut map = |expression: Spanned<Expression>| -> Spanned<Expression> {
        let Expression::Call(call) = &expression.node else { return expression };
        let Expression::Path(callee) = &call.node.callee.node else { return expression };
        let names = callee.node.path.node.segments.iter().map(|s| s.node.name.node.name.clone()).collect::<Vec<_>>();
        let Some(provider) = find_provider(providers, &unit_path, &imports, &names, module_source) else {
            return expression;
        };
        if provider.explicit_arity != call.node.args.len() {
            return expression;
        }
        let start = call.node.callee.span.line_col_start;
        let span = call.node.callee.span;
        let mut call = call.clone();
        for kind in &provider.kinds {
            let literal = match kind {
                CallerKind::File => Literal::String(format!("\"{}\"", escape(&file))),
                CallerKind::Line => Literal::Integer(format!("{}_i64", start.0)),
                CallerKind::Column => Literal::Integer(format!("{}_i64", start.1)),
            };
            let literal = Spanned::new(literal, span);
            call.node.args.push(Spanned::new(Expression::Literal(Spanned::new(LiteralExpression { literal }, span)), span));
        }
        changed = true;
        Spanned::new(Expression::Call(call), expression.span)
    };
    let mut items = std::mem::take(&mut unit.program.node.items);
    for item in &mut items {
        rewrite_item(item, &mut map);
    }
    unit.program.node.items = items;
    changed
}

fn rewrite_item(item: &mut Spanned<Node>, map: &mut impl FnMut(Spanned<Expression>) -> Spanned<Expression>) {
    let block = |statements: &mut Vec<Spanned<Statement>>, map: &mut dyn FnMut(Spanned<Expression>) -> Spanned<Expression>| {
        *statements = std::mem::take(statements).into_iter().map(|statement| map_statement(statement, &mut |e| map(e))).collect();
    };
    match &mut item.node {
        Node::Function(function) => block(&mut function.node.body.node.statements, map),
        Node::Method(method) => block(&mut method.node.body.node.statements, map),
        Node::ImplBlock(implementation) => {
            for method in &mut implementation.node.methods {
                block(&mut method.node.body.node.statements, map);
            }
        }
        Node::TestDefinition(test) => block(&mut test.node.statements, map),
        Node::InlineModule(module) => {
            for inner in &mut module.node.items {
                rewrite_item(inner, map);
            }
        }
        _ => {}
    }
}

fn find_provider<'a>(
    providers: &'a [Provider],
    unit_path: &Path,
    imports: &[(String, Vec<String>)],
    names: &[String],
    module_source: &dyn Fn(&[String]) -> Option<PathBuf>,
) -> Option<&'a Provider> {
    let (function, qualifier) = names.split_last()?;
    if qualifier.is_empty() {
        return providers.iter().find(|provider| provider.unit_path == unit_path && provider.name == *function);
    }
    let mut modules = vec![qualifier.to_vec()];
    for (binding, path) in imports {
        if *binding == qualifier[0] {
            modules.push(path.iter().chain(&qualifier[1..]).cloned().collect());
        }
    }
    modules.iter().find_map(|module| {
        // A module index names the owning file; a synthetic assembly without one names the unit
        // by its logical path (`Testing/Assert.bd`).
        let source = module_source(module);
        let logical = format!("{}.bd", module.join("/"));
        providers.iter().find(|provider| {
            provider.name == *function
                && (source.as_deref() == Some(provider.unit_path.as_path()) || provider.logical_name == logical)
        })
    })
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::parse_program;

    const ASSERT: &str = r#"
pub unit Check(bool ok, string because, [CallerFilePath] string file, [CallerLineNumber] i64 line, [CallerColumnNumber] i64 column) { return; }
pub unit Plain(bool ok) { return; }
"#;

    fn unit(path: &str, source: &str) -> SourceUnit {
        let program = parse_program(source).expect("parse");
        SourceUnit {
            logical_name: path.to_string(),
            origin_path: PathBuf::from(path),
            path: PathBuf::from(path),
            source: source.to_string(),
            program,
        }
    }

    fn resolve(path: &[String]) -> Option<PathBuf> {
        (path == ["Testing".to_string(), "Assert".to_string()]).then(|| PathBuf::from("/lib/Assert.bd"))
    }

    #[test]
    fn omitted_caller_arguments_are_filled_from_the_call_site() {
        let main = "use Testing.Assert;\ntest A {\n    let x = 1;\n    Assert.Check(true, \"m\");\n}\n";
        let units = vec![unit("/lib/Assert.bd", ASSERT), unit("/app/Main.bd", main)];
        let out = fill_caller_information(&units, &resolve).expect("rewritten");
        let text = serde_json::to_string(&out[1].program).unwrap();
        assert!(text.contains("\\\"/app/Main.bd\\\""), "{text}");
        assert!(text.contains("4_i64") && text.contains("5_i64"), "line 4, column 5: {text}");
    }

    #[test]
    fn an_explicit_argument_wins() {
        let main = "use Testing.Assert;\ntest A {\n    Assert.Check(true, \"m\", \"x.bd\", 9_i64, 9_i64);\n}\n";
        let units = vec![unit("/lib/Assert.bd", ASSERT), unit("/app/Main.bd", main)];
        assert!(fill_caller_information(&units, &resolve).is_none());
    }

    #[test]
    fn a_function_of_the_same_name_elsewhere_is_not_rewritten() {
        let main = "use Other.Mod;\nunit Check(bool a, string b) { return; }\ntest A {\n    Check(true, \"m\");\n    Mod.Check(true, \"m\");\n}\n";
        let units = vec![unit("/lib/Assert.bd", ASSERT), unit("/app/Main.bd", main)];
        assert!(fill_caller_information(&units, &resolve).is_none());
    }

    #[test]
    fn a_function_without_caller_parameters_is_untouched() {
        let main = "use Testing.Assert;\ntest A {\n    Assert.Plain(true);\n}\n";
        let units = vec![unit("/lib/Assert.bd", ASSERT), unit("/app/Main.bd", main)];
        assert!(fill_caller_information(&units, &resolve).is_none());
    }

    #[test]
    fn a_fully_qualified_call_and_a_generic_call_are_filled() {
        let main = "test A {\n    Testing.Assert.Check(true, \"m\");\n}\n";
        let units = vec![unit("/lib/Assert.bd", ASSERT), unit("/app/Main.bd", main)];
        assert!(fill_caller_information(&units, &resolve).is_some());
    }

    #[test]
    fn the_file_path_is_escaped() {
        assert_eq!(escape("C:\\a\\\"b"), "C:\\\\a\\\\\\\"b");
    }
}
