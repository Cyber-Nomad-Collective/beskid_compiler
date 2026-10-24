//! Optional arguments: a call may omit trailing optional parameters (see
//! [`crate::parameter_defaults`]); this pass fills them in before the assembly is indexed.
//!
//! Every call that resolves to a top-level function and passes at least its required arguments
//! but not all of them gets the missing trailing arguments appended: the default value of an
//! ordinary optional parameter, or for a caller-information parameter the file of the unit that
//! holds the call (the unit's requested path; renderers map a materialized copy to the authored
//! file) and the 1-based line and column where the callee expression starts. An explicit argument
//! is never replaced. Every appended node carries the span of the callee expression. The model is
//! C# optional parameters and `CallerFilePath`, `CallerLineNumber` and `CallerColumnNumber`.
//!
//! The callee is found from the call path, the unit's `use` declarations and the module graph, so
//! a same-named function elsewhere is never rewritten. Defaults are allowed only on top-level
//! functions (E1254 elsewhere), which are exactly the callees this pass resolves.

use std::path::{Path, PathBuf};

use super::SourceUnit;
use crate::macros::map_statement;
use crate::parameter_defaults::{
    CallerKind, DefaultConstant, caller_kind, converted_to, default_constant, top_level_constant,
};
use crate::syntax::{Expression, HostBodyItem, Literal, LiteralExpression, MethodDefinition, Node, Spanned, Statement};

/// What the compiler passes for one omitted optional parameter.
enum Slot {
    Caller(CallerKind),
    Default(DefaultConstant),
}

struct Provider {
    unit_path: PathBuf,
    logical_name: String,
    name: String,
    /// Number of parameters without a default value (they come first).
    required: usize,
    /// The optional parameters, in order.
    optional: Vec<Slot>,
}

fn collect_providers(units: &[SourceUnit]) -> Vec<Provider> {
    let mut providers = Vec::new();
    for unit in units {
        for item in &unit.program.node.items {
            let Node::Function(function) = &item.node else { continue };
            let parameters = &function.node.parameters;
            let Some(required) = parameters.iter().position(|parameter| parameter.node.default.is_some()) else {
                continue;
            };
            // An invalid declaration (a required parameter after an optional one, a default that is
            // not a constant, a `bulk` default) is reported by the declaration rules; its calls stay
            // as written and get an arity error instead of a guessed value.
            let optional = parameters[required..]
                .iter()
                .map(|parameter| {
                    let default = parameter.node.default.as_ref()?;
                    if parameter.node.bulk {
                        return None;
                    }
                    let value = default_constant(default, &|name| top_level_constant(&unit.program.node, name))?;
                    let value = converted_to(value, &parameter.node.ty.node);
                    Some(caller_kind(&parameter.node).map_or(Slot::Default(value), Slot::Caller))
                })
                .collect::<Option<Vec<_>>>();
            let Some(optional) = optional else { continue };
            providers.push(Provider {
                unit_path: unit.path.clone(),
                logical_name: unit.logical_name.clone(),
                name: function.node.name.node.name.clone(),
                required,
                optional,
            });
        }
    }
    providers
}

/// The units with omitted optional arguments filled in, or `None` when no call changes.
pub(super) fn fill_optional_arguments(
    units: &[SourceUnit],
    module_source: &dyn Fn(&[String]) -> Option<PathBuf>,
) -> Option<Vec<SourceUnit>> {
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
        let passed = call.node.args.len();
        if passed < provider.required || passed >= provider.required + provider.optional.len() {
            return expression;
        }
        let start = call.node.callee.span.line_col_start;
        let span = call.node.callee.span;
        let mut call = call.clone();
        for slot in &provider.optional[passed - provider.required..] {
            let argument = match slot {
                Slot::Default(value) => value.to_expression(span),
                Slot::Caller(kind) => {
                    let literal = match kind {
                        CallerKind::File => Literal::String(format!("\"{}\"", escape(&file))),
                        CallerKind::Line => Literal::Integer(format!("{}_i64", start.0)),
                        CallerKind::Column => Literal::Integer(format!("{}_i64", start.1)),
                    };
                    let literal = Spanned::new(literal, span);
                    Spanned::new(Expression::Literal(Spanned::new(LiteralExpression { literal }, span)), span)
                }
            };
            call.node.args.push(argument);
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

type Map<'a> = dyn FnMut(Spanned<Expression>) -> Spanned<Expression> + 'a;

fn rewrite_block(statements: &mut Vec<Spanned<Statement>>, map: &mut Map<'_>) {
    *statements = std::mem::take(statements).into_iter().map(|statement| map_statement(statement, &mut |e| map(e))).collect();
}

fn rewrite_methods(methods: &mut [Spanned<MethodDefinition>], map: &mut Map<'_>) {
    for method in methods {
        rewrite_block(&mut method.node.body.node.statements, map);
    }
}

fn rewrite_host_body(body: &mut [Spanned<HostBodyItem>], map: &mut Map<'_>) {
    for item in body {
        match &mut item.node {
            HostBodyItem::Hook(hook) => rewrite_block(&mut hook.node.body.node.statements, map),
            HostBodyItem::Scope(scope) => rewrite_host_body(&mut scope.node.body, map),
            HostBodyItem::Registry(_) | HostBodyItem::Registration(_) => {}
        }
    }
}

fn rewrite_item(item: &mut Spanned<Node>, map: &mut Map<'_>) {
    match &mut item.node {
        Node::Function(function) => rewrite_block(&mut function.node.body.node.statements, map),
        Node::Method(method) => rewrite_block(&mut method.node.body.node.statements, map),
        Node::ImplBlock(implementation) => rewrite_methods(&mut implementation.node.methods, map),
        Node::ExtendTypeDefinition(extension) => rewrite_methods(&mut extension.node.methods, map),
        Node::TypeDefinition(definition) => rewrite_methods(&mut definition.node.methods, map),
        Node::HostDefinition(host) => rewrite_host_body(&mut host.node.body, map),
        Node::TestDefinition(test) => rewrite_block(&mut test.node.statements, map),
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
pub unit Check(bool ok, string because, [CallerFilePath] string file = "", [CallerLineNumber] i64 line = 0, [CallerColumnNumber] i64 column = 0) { return; }
pub unit Say(string text, string because = "", i64 times = -2, [CallerLineNumber] i64 line = 0) { return; }
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
        let out = fill_optional_arguments(&units, &resolve).expect("rewritten");
        let text = serde_json::to_string(&out[1].program).unwrap();
        assert!(text.contains("\\\"/app/Main.bd\\\""), "{text}");
        assert!(text.contains("4_i64") && text.contains("5_i64"), "line 4, column 5: {text}");
    }

    #[test]
    fn an_explicit_argument_wins() {
        let main = "use Testing.Assert;\ntest A {\n    Assert.Check(true, \"m\", \"x.bd\", 9_i64, 9_i64);\n}\n";
        let units = vec![unit("/lib/Assert.bd", ASSERT), unit("/app/Main.bd", main)];
        assert!(fill_optional_arguments(&units, &resolve).is_none());
    }

    #[test]
    fn a_function_of_the_same_name_elsewhere_is_not_rewritten() {
        let main = "use Other.Mod;\nunit Check(bool a, string b) { return; }\ntest A {\n    Check(true, \"m\");\n    Mod.Check(true, \"m\");\n}\n";
        let units = vec![unit("/lib/Assert.bd", ASSERT), unit("/app/Main.bd", main)];
        assert!(fill_optional_arguments(&units, &resolve).is_none());
    }

    #[test]
    fn a_function_without_caller_parameters_is_untouched() {
        let main = "use Testing.Assert;\ntest A {\n    Assert.Plain(true);\n}\n";
        let units = vec![unit("/lib/Assert.bd", ASSERT), unit("/app/Main.bd", main)];
        assert!(fill_optional_arguments(&units, &resolve).is_none());
    }

    #[test]
    fn a_fully_qualified_call_and_a_generic_call_are_filled() {
        let main = "test A {\n    Testing.Assert.Check(true, \"m\");\n}\n";
        let units = vec![unit("/lib/Assert.bd", ASSERT), unit("/app/Main.bd", main)];
        assert!(fill_optional_arguments(&units, &resolve).is_some());
    }

    fn call_arguments(out: &[SourceUnit], unit: usize) -> Vec<Vec<String>> {
        let mut calls = Vec::new();
        let mut collect = |expression: Spanned<Expression>| {
            if let Expression::Call(call) = &expression.node {
                calls.push(
                    call.node
                        .args
                        .iter()
                        .map(|argument| crate::format::Emitter::new().write(&argument.node).expect("emit"))
                        .collect::<Vec<_>>(),
                );
            }
            expression
        };
        let mut items = out[unit].program.node.items.clone();
        for item in &mut items {
            rewrite_item(item, &mut collect);
        }
        calls
    }

    #[test]
    fn omitted_defaults_are_passed_and_explicit_ones_are_kept() {
        let main = "use Testing.Assert;\ntest A {\n    Assert.Say(\"a\");\n    Assert.Say(\"b\", \"why\");\n    Assert.Say(\"c\", \"why\", 3, 9_i64);\n}\n";
        let units = vec![unit("/lib/Assert.bd", ASSERT), unit("/app/Main.bd", main)];
        let out = fill_optional_arguments(&units, &resolve).expect("rewritten");
        let calls = call_arguments(&out, 1);
        assert_eq!(calls[0], ["\"a\"", "\"\"", "-2", "3_i64"], "{calls:?}");
        assert_eq!(calls[1], ["\"b\"", "\"why\"", "-2", "4_i64"], "{calls:?}");
        assert_eq!(calls[2], ["\"c\"", "\"why\"", "3", "9_i64"], "{calls:?}");
    }

    #[test]
    fn a_call_without_its_required_arguments_is_left_for_the_arity_error() {
        let main = "use Testing.Assert;\ntest A {\n    Assert.Say();\n}\n";
        let units = vec![unit("/lib/Assert.bd", ASSERT), unit("/app/Main.bd", main)];
        assert!(fill_optional_arguments(&units, &resolve).is_none());
    }

    #[test]
    fn a_default_names_a_constant_of_its_own_file_and_calls_in_methods_and_hooks_are_filled() {
        let main = "const Limit = 7;\nunit Take(i64 count = Limit) { return; }\ntype Box { unit Run() { Take(); } }\nhost App() { startup() { Take(); } }\n";
        let units = vec![unit("/app/Main.bd", main)];
        let out = fill_optional_arguments(&units, &resolve).expect("rewritten");
        let calls = call_arguments(&out, 0);
        assert_eq!(calls, [vec!["7".to_string()], vec!["7".to_string()]], "{calls:?}");
    }

    #[test]
    fn an_invalid_default_fills_nothing() {
        let main = "unit Take(i64 count = Other()) { return; }\nunit Other() { return; }\nunit Run() { Take(); }\n";
        let units = vec![unit("/app/Main.bd", main)];
        assert!(fill_optional_arguments(&units, &resolve).is_none());
    }

    #[test]
    fn the_file_path_is_escaped() {
        assert_eq!(escape("C:\\a\\\"b"), "C:\\\\a\\\\\\\"b");
    }
}
