//! Generic function value legality fact (E1203, E1204, E1229, E1610).
//!
//! A generic function item read as a value (`Apply(5, Identity)`, `Identity<i64>`) is
//! instantiated where it is read (`function_item_value`): its explicit type arguments, or the
//! expected function type of its position, bind every generic parameter. This fact never
//! re-derives the binding; it asks the same `function_item_value` authority lowering asks and
//! reports why it has no instance:
//!
//! - an explicit type-argument list whose count differs from the declaration's generics: E1204;
//! - one generic parameter bound to two different types by the expected function type: E1229;
//! - a binding that does not satisfy a `where` bound of the declaration: E1610;
//! - any other undetermined type argument (no expected function type, a generic parameter the
//!   expected type does not reach, or a type argument that names a generic parameter of the
//!   enclosing item, which 0.6.0 does not instantiate): E1203.
//!
//! Every function, method and test body is judged, generic ones included, so a generic body that
//! reads a generic function value it cannot instantiate is rejected before specialization.

use super::calls::is_spawn_target;
use super::members::is_call_callee;
use super::*;
use beskid_analysis::syntax::PathExpression;

/// Why one generic function value has no instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum GenericFunctionValueKind {
    /// E1203: the type arguments cannot be determined at the read.
    MissingTypeArguments,
    /// E1204: the explicit type-argument count differs from the declaration's generics.
    GenericArgumentMismatch { expected: usize, actual: usize },
    /// E1229: the expected function type binds one generic parameter to two different types.
    Conflict(GenericBindingConflict),
    /// E1610: the bound type does not satisfy a `where` bound of the declaration.
    BoundNotSatisfied { type_name: Arc<str>, contract_name: Arc<str>, reason: Option<Arc<str>> },
}

/// One generic function value without an instance, at its path expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericFunctionValueFinding {
    pub site: AstNodeKey,
    pub kind: GenericFunctionValueKind,
}

/// Every generic function value in the item `key` that has no instance, in source order.
pub(super) fn generic_function_value_findings(db: &dyn Db, key: AstNodeKey) -> Vec<GenericFunctionValueFinding> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return Vec::new();
    };
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    if !matches!(
        index.kind(key.node),
        Some(NodeKind::FunctionDefinition | NodeKind::MethodDefinition | NodeKind::TestDefinition)
    ) {
        return Vec::new();
    }
    let mut paths = Vec::new();
    collect_nodes_of_kind(index, key.node, NodeKind::PathExpression, &mut paths);
    let mut findings = Vec::new();
    for node in paths {
        let Some(path) = index.node_at(program, node).and_then(|node| node.of::<PathExpression>()) else { continue };
        if is_call_callee(program, index, node) || is_spawn_target(index, node) {
            continue;
        }
        let site = AstNodeKey { node, ..key };
        if !names_generic_function_item(db, program, index, site, &path.path.node) {
            continue;
        }
        let error = match function_item_value(db, site) {
            Ok(_) => continue,
            Err(error) => error,
        };
        let explicit = path.path.node.segments.last().map_or(0, |segment| segment.node.type_args.len());
        let declared = resolve_item_declaration_candidate(db, program, index, site, &path.path.node)
            .and_then(|declaration| generic_callable_parameters(db, declaration))
            .map_or(0, |(names, _)| names.len());
        let kind = if explicit != 0 && explicit != declared {
            GenericFunctionValueKind::GenericArgumentMismatch { expected: declared, actual: explicit }
        } else if let Some(conflict) = error.binding_conflict() {
            GenericFunctionValueKind::Conflict(conflict.clone())
        } else if let Some(violation) = error.bound_violation() {
            GenericFunctionValueKind::BoundNotSatisfied {
                type_name: Arc::clone(&violation.type_name),
                contract_name: Arc::clone(&violation.contract_name),
                reason: violation.reason.clone(),
            }
        } else {
            GenericFunctionValueKind::MissingTypeArguments
        };
        findings.push(GenericFunctionValueFinding { site, kind });
    }
    findings
}

/// The legality-gate findings of [`generic_function_value_findings`].
pub(super) fn generic_function_value_semantic_findings(db: &dyn Db, key: AstNodeKey) -> Vec<SemanticFinding> {
    generic_function_value_findings(db, key)
        .into_iter()
        .map(|finding| {
            let kind = match finding.kind {
                GenericFunctionValueKind::MissingTypeArguments => {
                    SemanticIssueKind::TypeMissingTypeArguments { parameter: None }
                }
                GenericFunctionValueKind::GenericArgumentMismatch { expected, actual } => {
                    SemanticIssueKind::TypeGenericArgumentMismatch { expected, actual }
                }
                GenericFunctionValueKind::Conflict(conflict) => SemanticIssueKind::TypeGenericParameterConflict {
                    parameter: conflict.parameter.to_string(),
                    first_name: conflict.first.to_string(),
                    second_name: conflict.second.to_string(),
                },
                GenericFunctionValueKind::BoundNotSatisfied { type_name, contract_name, reason } => {
                    SemanticIssueKind::GenericBoundNotSatisfied {
                        type_name: type_name.to_string(),
                        contract_name: contract_name.to_string(),
                        reason: reason.as_deref().map(str::to_owned),
                    }
                }
            };
            SemanticFinding { kind, site: finding.site, related: Vec::new() }
        })
        .collect()
}
