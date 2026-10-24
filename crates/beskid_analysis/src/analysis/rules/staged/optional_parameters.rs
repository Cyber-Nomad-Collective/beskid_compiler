//! Declaration rules for optional parameters (see [`crate::parameter_defaults`]):
//! E1251 a required parameter after an optional one, E1252 a default that is not a compile-time
//! constant, E1253 a default that does not fit the parameter type, E1254 an optional parameter
//! where no call can pass it (hook parameters, `bulk` parameters), E1255 a caller-information parameter without a default, E1256 a
//! caller-information parameter of the wrong type.

use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::rules::RuleContext;
use crate::format::Emitter;
use crate::parameter_defaults::{caller_kind, default_constant, default_fits, top_level_constant};
use crate::syntax::{ContractNode, HostBodyItem, MethodDefinition, Node, Parameter, Program, Spanned, Type};

const HOOK: &str = "a parameter of an `init`, `dispose` or `startup` hook";
const BULK: &str = "a `bulk` parameter";

impl SemanticPipelineRule {
    pub(super) fn stage_optional_parameters(&self, ctx: &mut RuleContext, program: &Program) {
        check_items(ctx, program, &program.items);
    }
}

fn check_items(ctx: &mut RuleContext, program: &Program, items: &[Spanned<Node>]) {
    let methods = |ctx: &mut RuleContext, methods: &[Spanned<MethodDefinition>]| {
        for method in methods {
            check_parameters(ctx, program, &method.node.parameters);
        }
    };
    for item in items {
        match &item.node {
            Node::Function(function) => check_parameters(ctx, program, &function.node.parameters),
            Node::Method(method) => check_parameters(ctx, program, &method.node.parameters),
            Node::ImplBlock(implementation) => methods(ctx, &implementation.node.methods),
            Node::ExtendTypeDefinition(extension) => methods(ctx, &extension.node.methods),
            Node::TypeDefinition(definition) => methods(ctx, &definition.node.methods),
            Node::ContractDefinition(contract) => {
                for item in &contract.node.items {
                    if let ContractNode::MethodSignature(signature) = &item.node {
                        check_parameters(ctx, program, &signature.node.parameters);
                    }
                }
            }
            Node::HostDefinition(host) => {
                check_parameters(ctx, program, &host.node.parameters);
                check_host_body(ctx, program, &host.node.body);
            }
            Node::InlineModule(module) => check_items(ctx, program, &module.node.items),
            _ => {}
        }
    }
}

fn check_host_body(ctx: &mut RuleContext, program: &Program, body: &[Spanned<HostBodyItem>]) {
    for item in body {
        match &item.node {
            HostBodyItem::Scope(scope) => {
                check_parameters(ctx, program, &scope.node.parameters);
                check_host_body(ctx, program, &scope.node.body);
            }
            // The composition runtime starts hooks; no call site passes their arguments, so a
            // default could never apply.
            HostBodyItem::Hook(hook) => reject_optional(ctx, &hook.node.parameters, HOOK),
            HostBodyItem::Registry(_) | HostBodyItem::Registration(_) => {}
        }
    }
}

/// E1254 for every default value or caller-information attribute in `parameters`.
fn reject_optional(ctx: &mut RuleContext, parameters: &[Spanned<Parameter>], place: &str) {
    for parameter in parameters {
        let site = match (&parameter.node.default, caller_kind(&parameter.node)) {
            (Some(default), _) => default.span,
            (None, Some(_)) => parameter.node.name.span,
            (None, None) => continue,
        };
        ctx.emit_issue(site, SemanticIssueKind::OptionalParameterNotAllowed { place: place.to_string() });
    }
}

fn check_parameters(ctx: &mut RuleContext, program: &Program, parameters: &[Spanned<Parameter>]) {
    let mut optional_seen = false;
    for parameter in parameters {
        let parameter = &parameter.node;
        let name = parameter.name.node.name.clone();
        let caller = caller_kind(parameter);
        if let Some(kind) = caller {
            let expected = kind.required_type();
            if !matches!(&parameter.ty.node, Type::Primitive(primitive) if primitive.node == expected) {
                ctx.emit_issue(
                    parameter.ty.span,
                    SemanticIssueKind::CallerInformationParameterType {
                        name: name.clone(),
                        attribute: kind.attribute().to_string(),
                        expected: expected_text(expected),
                    },
                );
            }
        }
        let Some(default) = &parameter.default else {
            if let Some(kind) = caller {
                ctx.emit_issue(
                    parameter.name.span,
                    SemanticIssueKind::CallerInformationWithoutDefault {
                        name: name.clone(),
                        attribute: kind.attribute().to_string(),
                        expected: expected_text(kind.required_type()),
                    },
                );
            } else if optional_seen {
                ctx.emit_issue(parameter.name.span, SemanticIssueKind::RequiredParameterAfterOptional { name });
            }
            continue;
        };
        optional_seen = true;
        if parameter.bulk {
            ctx.emit_issue(default.span, SemanticIssueKind::OptionalParameterNotAllowed { place: BULK.to_string() });
            continue;
        }
        let Some(value) = default_constant(default, &|constant| top_level_constant(program, constant)) else {
            ctx.emit_issue(default.span, SemanticIssueKind::DefaultValueNotConstant { name });
            continue;
        };
        // A caller-information parameter of the wrong type is reported once, above.
        if caller.is_none() && !default_fits(&value, &parameter.ty.node) {
            ctx.emit_issue(
                default.span,
                SemanticIssueKind::DefaultValueTypeMismatch { name, value: value.text(), expected: type_text(&parameter.ty) },
            );
        }
    }
}

fn type_text(ty: &Spanned<Type>) -> String {
    Emitter::new().write(&ty.node).unwrap_or_else(|_| "<type>".to_string())
}

fn expected_text(primitive: crate::syntax::PrimitiveType) -> String {
    match primitive {
        crate::syntax::PrimitiveType::String => "string".to_string(),
        _ => "i64".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use crate::analysis::{AnalysisOptions, builtin_rules, run_rules};
    use crate::services::parse_program;

    /// `(code, message)` of every E125x finding.
    fn findings(source: &str) -> Vec<(String, String)> {
        let program = parse_program(source).expect("parse");
        let result = run_rules(&program.node, "test.bd", source, &builtin_rules(), AnalysisOptions::default());
        result
            .diagnostics
            .iter()
            .filter_map(|diagnostic| {
                let code = diagnostic.code.as_deref()?;
                code.starts_with("E125").then(|| (code.to_string(), diagnostic.message.clone()))
            })
            .collect()
    }

    #[test]
    fn valid_optional_parameters_are_silent() {
        let source = "const Limit = 3;\n\
                      pub unit Check(bool ok, string because = \"\", i64 times = Limit, f64 scale = 1, f32 small = -0.5, \
                      u8 byte = 255, char mark = 'x', bool strict = false, \
                      [CallerFilePath] string file = \"\", [CallerLineNumber] i64 line = 0, [CallerColumnNumber] i64 column = 0) { return; }";
        assert!(findings(source).is_empty(), "{:?}", findings(source));
    }

    #[test]
    fn a_required_parameter_after_an_optional_one_is_e1251() {
        let found = findings("pub unit F(string because = \"\", bool ok) { return; }");
        assert_eq!(found, [("E1251".to_string(), "required parameter `ok` follows an optional parameter".to_string())]);
    }

    #[test]
    fn a_default_that_is_not_a_constant_is_e1252() {
        let found = findings("pub i64 G() { return 1; }\npub unit F(i64 count = G()) { return; }");
        assert_eq!(found, [("E1252".to_string(), "default value of `count` is not a compile-time constant".to_string())]);
    }

    #[test]
    fn a_default_of_the_wrong_type_is_e1253() {
        let found = findings("pub unit F(i64 count = \"x\", u8 small = 300, i64 exact = 1_i32, string text = 0) { return; }");
        let codes = found.iter().map(|(code, _)| code.as_str()).collect::<Vec<_>>();
        assert_eq!(codes, ["E1253", "E1253", "E1253", "E1253"], "{found:?}");
        assert_eq!(found[1].1, "default value `300` does not fit parameter `small` of type `u8`");
    }

    #[test]
    fn methods_contract_methods_inline_modules_hosts_and_scopes_take_optional_parameters() {
        let source = "pub type Box { pub unit Run(i64 x = 1) { return; } }\n\
                      pub contract Runner { unit Go(i64 x = 1, string because = \"\"); }\n\
                      mod Inner { pub unit Go(i64 x = 1) { return; } }\n\
                      host App(i64 port = 80) { scope Request(string id = \"\") { } }";
        assert!(findings(source).is_empty(), "{:?}", findings(source));
    }

    #[test]
    fn the_declaration_rules_apply_to_methods_too() {
        let found = findings("pub type Box { pub unit Run(i64 x = \"no\", bool y) { return; } }");
        let codes = found.iter().map(|(code, _)| code.as_str()).collect::<Vec<_>>();
        assert_eq!(codes, ["E1253", "E1251"], "{found:?}");
    }

    #[test]
    fn hook_and_bulk_parameters_cannot_be_optional() {
        let source = "host App() { startup(i64 x = 1) { } }\npub unit Many(bulk i64[] values = 1) { return; }";
        let messages = findings(source).into_iter().map(|(_, message)| message).collect::<Vec<_>>();
        assert_eq!(
            messages,
            [
                "a parameter of an `init`, `dispose` or `startup` hook cannot be optional",
                "a `bulk` parameter cannot be optional",
            ]
        );
    }

    #[test]
    fn caller_information_needs_a_default_and_its_type() {
        let found = findings("pub unit F([CallerLineNumber] i64 line, [CallerFilePath] i64 file = 0) { return; }");
        assert_eq!(
            found,
            [
                ("E1255".to_string(), "`[CallerLineNumber]` parameter `line` needs a default value".to_string()),
                ("E1256".to_string(), "`[CallerFilePath]` parameter `file` must have type `string`".to_string()),
            ]
        );
    }
}
