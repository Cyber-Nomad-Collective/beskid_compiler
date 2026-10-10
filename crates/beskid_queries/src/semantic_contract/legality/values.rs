//! E1206 value obligations of one item that `typing_obligations` does not judge: positions whose
//! destination or value is a nominal, array, contract, or function type, and the value positions
//! the proven-primitive fact has no destination for.
//!
//! `typing_obligations` judges proven primitives only, because `SemanticTypeId` represents every
//! nominal value as `pointer`. This fact judges the same legacy `require_same_type` sites on the
//! source type identity instead (`generic_source_type_identity` for a declared destination,
//! `generic_source_expression_identity` for a value), the identity authority generic
//! specialization already uses:
//!
//! * a typed `let` initializer (site: the declared name), a `return` value against the declared
//!   result (site: the return statement; a `test` body returns `unit`, as the legacy
//!   `TestDefinition` item typing sets `current_return_type`), and a local assignment (site: the
//!   assignment), when one side is not a proven primitive;
//! * every value arm of a `match` that is the direct initializer of a typed `let` or the value of
//!   a `return`, against that contextual destination (site: the arm value), as the legacy
//!   `type_match_expression_with_expected` does;
//! * direct call arguments against the declared parameters of a function, a method, or a
//!   contract method signature reached through a contract-typed or bounded receiver (site: the
//!   argument; legacy `type_call_expression`, every `require_same_type(arg.span, ..)` site). A
//!   parameter of a generic callee that mentions none of the callee's generic parameters is
//!   judged from the declaration alone; one that does is judged only when explicit type
//!   arguments on the callee path or a proven specialization fix the substitution;
//! * arguments of a call through a local or parameter of function type, against the declared
//!   function type's parameters (legacy `CallableValueCall`), and such a call's declared result
//!   where it flows into a judged destination;
//! * arguments of an event raise against the event field's declared parameters (legacy
//!   `EventInvoke` path of `type_call_expression`);
//! * the body of a lambda written into a position of function type (a typed `let` or a call
//!   parameter) against that type's result (site: the lambda body; legacy
//!   `type_lambda_expression_with_expected`), and the signature of an event subscription handler
//!   against the event field's type (site: the assignment; legacy `require_same_type(assign.span,
//!   target, value)`);
//! * struct literal field values against the declared field types, and enum constructor
//!   arguments against the variant's declared field types (site: the value), with the generic
//!   parameters of a generic `type` or `enum` substituted from the explicit type arguments of the
//!   literal path or from the contextual destination;
//! * literal and enum patterns against the scrutinee, recursively through enum pattern payloads
//!   against the variant's declared field types (site: the pattern; legacy `type_pattern` and
//!   `type_pattern_with_expected`).
//!
//! Compatibility follows the legacy `require_same_type`: two nominal values are compatible when
//! they name the same declaration (generic arguments are not compared, as `named_item_id` does
//! not compare them); arrays when their proven primitive elements are equal; `u8[]` and `i64`
//! share the byte-array representation; two function types when their parameters and results
//! are pairwise compatible; `never` and `pointer` are never judged; a function value against a
//! non-function destination is not judged; two primitives follow `typing::compatible`. A
//! contract-typed destination (legacy `is_contract_compatible`) accepts a nominal value whose
//! `type` declaration conforms to the contract through its own conformance list or an `impl`
//! block, and rejects a proven primitive, an array, or a nominal `type` that does not conform.
//!
//! The fact is fail-closed: a generic parameter, or anything `generic_source_type_identity`
//! cannot prove, is never judged, and neither is a value whose identity is unavailable, a
//! nominal value into a contract destination whose concrete `type` declaration is not unique,
//! a lambda whose body is a block, a generic-mentioning parameter or field without a proven
//! substitution, or an explicitly typed lambda parameter against the expected parameter type
//! (the legacy checker adopts the explicit type without comparing it).

use super::members::enum_definition;
use super::typing::{compatible, enclosing_callable, expression_nodes, nodes_of_kind, proven_type};
use super::*;
use crate::semantic_contract::contracts::{concrete_declaration, type_declaration_conforms_to_contract};
use beskid_analysis::syntax::{
    AssignExpression, AssignOp, AstNodeId, CallExpression, ContractMethodSignature, EnumConstructorExpression,
    Expression, FieldKind, FunctionDefinition, LambdaExpression, LetStatement, MatchExpression,
    MethodDefinition, Pattern, ReturnStatement, Spanned, StructLiteralExpression, TestDefinition, Type,
    TypeDefinition,
};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

type Identity = GenericSourceTypeIdentity;
type Program = Spanned<beskid_analysis::syntax::Program>;

/// One value whose source type identity is incompatible with its destination (E1206).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ValueObligation {
    pub expected: Arc<str>,
    pub actual: Arc<str>,
    /// The exact node that carries the legacy diagnostic.
    pub site: AstNodeKey,
}

/// Report every value obligation `key` (a function, method, or test item) fails, in source order.
/// Non-item nodes contain no fact.
pub fn value_obligations(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[ValueObligation]>> {
    with_registered_syntax(db, key, value_obligations_tracked)
}

#[salsa::tracked(persist)]
fn value_obligations_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[ValueObligation]>> {
    with_node(db, syntax, key, |program, index, node| {
        if !matches!(
            node.node_kind(),
            NodeKind::FunctionDefinition | NodeKind::MethodDefinition | NodeKind::TestDefinition
        ) {
            return None;
        }
        let mut findings = Vec::new();
        collect_let_values(db, program, index, key, &mut findings);
        collect_return_values(db, program, index, key, node, &mut findings);
        collect_assignment_values(db, program, index, key, &mut findings);
        collect_call_argument_values(db, program, index, key, &mut findings);
        collect_callable_value_call_values(db, program, index, key, &mut findings);
        collect_event_raise_values(db, program, index, key, &mut findings);
        collect_struct_literal_values(db, program, index, key, &mut findings);
        collect_enum_constructor_values(db, program, index, key, &mut findings);
        collect_pattern_values(db, program, index, key, &mut findings);
        findings.sort_by_key(|finding| finding.site.node.0);
        findings.dedup();
        Some(Arc::from(findings))
    })
}

/// A judged destination: a source type identity, or a contract the value must conform to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Destination {
    Identity(Identity),
    /// A contract-typed destination (legacy `is_contract_compatible`), with its source spelling.
    Contract { contract: AstNodeKey, name: Arc<str> },
}

/// Whether `ty` is a primitive other than `pointer`: the destinations `typing_obligations` judges.
fn primitive_destination(ty: &Type) -> bool {
    semantic_type_from_syntax(ty).is_ok_and(|ty| ty != SemanticTypeId::POINTER)
}

/// Whether `node_type` proves `value` a primitive other than `pointer` (the ABI of nominal values).
fn proven_primitive(db: &dyn Db, value: AstNodeKey) -> bool {
    proven_type(db, value).is_some_and(|ty| ty != SemanticTypeId::POINTER)
}

/// The destination a declared type denotes, resolved in the declaring unit at `scope`: a
/// contract (judged by conformance) or a source type identity. Generic parameters of the
/// enclosing callable are not judged.
fn declared_destination(db: &dyn Db, scope: AstNodeKey, ty: &Type) -> Option<Destination> {
    if type_syntax_is_enclosing_generic_parameter_reference(db, scope, ty) {
        return None;
    }
    if let Type::Complex(path) = ty
        && let Some(contract) = resolve_contract(db, scope, &path.node)
    {
        return Some(Destination::Contract { contract, name: Arc::from(render_type_syntax(ty).as_str()) });
    }
    generic_source_type_identity(db, scope, ty).ok().map(Destination::Identity)
}

/// The destination a declared type of a generic declaration (a generic `type`, `enum`, or
/// callable whose generic parameter names are `generics`) denotes. A type that mentions a
/// generic parameter is judged only with the proven `substitutions`; otherwise it is the plain
/// declared destination.
fn generic_declared_destination(
    db: &dyn Db,
    scope: AstNodeKey,
    generics: &[&str],
    ty: &Type,
    substitutions: Option<&HashMap<&str, &Identity>>,
) -> Option<Destination> {
    if generics.iter().any(|generic| type_syntax_mentions_generic_parameter(ty, generic)) {
        let substitutions = substitutions?;
        return generic_source_type_identity_with_substitutions(db, scope, ty, substitutions)
            .ok()
            .map(Destination::Identity);
    }
    declared_destination(db, scope, ty)
}

/// The identity of a value: its proven primitive, its source expression identity, or the
/// declared result of a call through a local of function type.
fn value_identity(db: &dyn Db, value: AstNodeKey) -> Option<Identity> {
    match proven_type(db, value) {
        Some(ty) if ty != SemanticTypeId::POINTER => Some(Identity::Abi(ty)),
        _ => generic_source_expression_identity(db, value).ok().or_else(|| callable_value_call_result(db, value)),
    }
}

/// The declared result of `value` when it is a call through a local or parameter of function
/// type (legacy `CallableValueCall`, typed as the function type's return).
fn callable_value_call_result(db: &dyn Db, value: AstNodeKey) -> Option<Identity> {
    let syntax = db.syntax_unit(value.unit).filter(|syntax| syntax.accepts_key(db, value))?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let call = index.node_at(program, value.node)?.of::<CallExpression>()?;
    let (_, result) = callable_value_signature(db, program, index, value, call)?;
    Some(*result)
}

/// The declared function type of a call's callee when the callee is a single-segment path naming
/// a local or parameter whose declared type is a function type. Calls to items, methods, and
/// lambda-initialized locals without an annotation are not such calls.
fn callable_value_signature(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    call: AstNodeKey,
    expression: &CallExpression,
) -> Option<(Arc<[Identity]>, Box<Identity>)> {
    let Expression::Path(path) = &expression.callee.node else { return None };
    let [segment] = path.node.path.node.segments.as_slice() else { return None };
    if !segment.node.type_args.is_empty() {
        return None;
    }
    let declaration = resolve_lexical_declaration(program, index, call.node, segment.node.name.node.name.as_str())?;
    match generic_source_local_identity(db, program, index, call, declaration).ok()? {
        Identity::Function { parameters, result } => Some((parameters, result)),
        _ => None,
    }
}

fn is_byte_array(identity: &Identity) -> bool {
    matches!(identity, Identity::Array(element) if matches!(element.as_ref(), Identity::Abi(SemanticTypeId::U8)))
}

fn identities_compatible(expected: &Identity, actual: &Identity) -> bool {
    match (expected, actual) {
        (Identity::Abi(expected), Identity::Abi(actual)) => compatible(*expected, *actual),
        (Identity::Abi(ty), _) | (_, Identity::Abi(ty))
            if matches!(*ty, SemanticTypeId::POINTER | SemanticTypeId::NEVER) =>
        {
            true
        }
        (
            Identity::Function { parameters: expected_parameters, result: expected_result },
            Identity::Function { parameters: actual_parameters, result: actual_result },
        ) => {
            expected_parameters.len() == actual_parameters.len()
                && expected_parameters
                    .iter()
                    .zip(actual_parameters.iter())
                    .all(|(expected, actual)| identities_compatible(expected, actual))
                && identities_compatible(expected_result, actual_result)
        }
        (Identity::Function { .. }, _) | (_, Identity::Function { .. }) => true,
        (Identity::Nominal { qualified_name: expected, .. }, Identity::Nominal { qualified_name: actual, .. }) => {
            expected == actual
        }
        (Identity::Array(expected), Identity::Array(actual)) => match (expected.as_ref(), actual.as_ref()) {
            (Identity::Abi(expected), Identity::Abi(actual)) => {
                expected == actual || *expected == SemanticTypeId::POINTER || *actual == SemanticTypeId::POINTER
            }
            _ => true,
        },
        (Identity::Abi(SemanticTypeId::I64), array) | (array, Identity::Abi(SemanticTypeId::I64)) => {
            is_byte_array(array)
        }
        _ => false,
    }
}

/// Whether a contract-typed destination accepts `actual` (legacy `is_contract_compatible` after
/// the `named_item_id` equality test): `None` when conformance cannot be proven either way.
fn contract_accepts(db: &dyn Db, scope: AstNodeKey, contract: AstNodeKey, actual: &Identity) -> Option<bool> {
    match actual {
        Identity::Abi(SemanticTypeId::POINTER | SemanticTypeId::NEVER) | Identity::Function { .. } => None,
        Identity::Abi(_) | Identity::Array(_) => Some(false),
        Identity::Nominal { .. } => {
            let concrete = concrete_declaration(db, scope, actual)?;
            let definition = type_definition(db, concrete)?;
            Some(type_declaration_conforms_to_contract(db, concrete, &definition, contract))
        }
    }
}

fn type_definition(db: &dyn Db, declaration: AstNodeKey) -> Option<TypeDefinition> {
    let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
    syntax.syntax_index(db).node_at(syntax.expanded_program(db), declaration.node)?.of::<TypeDefinition>().cloned()
}

fn identity_name(identity: &Identity) -> String {
    match identity {
        Identity::Abi(ty) => ty.display_name(),
        Identity::Nominal { qualified_name, arguments } if arguments.is_empty() => qualified_name.to_string(),
        Identity::Nominal { qualified_name, arguments } => {
            format!("{qualified_name}<{}>", arguments.iter().map(identity_name).collect::<Vec<_>>().join(", "))
        }
        Identity::Array(element) => format!("{}[]", identity_name(element)),
        Identity::Function { parameters, result } => format!(
            "{}({})",
            identity_name(result),
            parameters.iter().map(identity_name).collect::<Vec<_>>().join(", ")
        ),
    }
}

fn judge(
    db: &dyn Db,
    findings: &mut Vec<ValueObligation>,
    expected: &Destination,
    actual: &Identity,
    site: AstNodeKey,
) {
    let (accepted, expected_name) = match expected {
        Destination::Identity(expected) => (identities_compatible(expected, actual), identity_name(expected)),
        Destination::Contract { contract, name } => {
            let Some(accepted) = contract_accepts(db, site, *contract, actual) else { return };
            (accepted, name.to_string())
        }
    };
    if !accepted {
        findings.push(ValueObligation {
            expected: Arc::from(expected_name.as_str()),
            actual: Arc::from(identity_name(actual).as_str()),
            site,
        });
    }
}

/// Judge the value at `subject` (its normalized node; `site` is the diagnostic node) against
/// `expected`. A lambda written into a function-typed destination is judged by its body against
/// the destination's result (legacy `type_lambda_expression_with_expected`); any other value by
/// its identity. A proven primitive against a primitive destination is left to
/// `typing_obligations` when `typing_judged`.
#[expect(clippy::too_many_arguments, reason = "one value judgement site with its proven expectation")]
fn judge_value(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    expected: &Destination,
    site: AstNodeId,
    subject: AstNodeId,
    typing_judged: bool,
    findings: &mut Vec<ValueObligation>,
) {
    let subject_key = AstNodeKey { node: subject, ..key };
    if let Some(lambda) = index.node_at(program, subject).and_then(|node| node.of::<LambdaExpression>()) {
        if let Destination::Identity(Identity::Function { parameters, result }) = expected {
            judge_lambda_body(db, program, index, key, subject, lambda, parameters, result, findings);
        }
        return;
    }
    if typing_judged && matches!(expected, Destination::Identity(Identity::Abi(_))) && proven_primitive(db, subject_key) {
        return;
    }
    let Some(actual) = value_identity(db, subject_key) else { return };
    judge(db, findings, expected, &actual, AstNodeKey { node: site, ..key });
}

/// The body of a lambda against the result of the function type it is written into (legacy
/// `require_same_type(lambda.node.body.span, expected_return, return_type)` in
/// `type_lambda_expression_with_expected`). The legacy checker reports an arity difference as
/// E1204 and types nothing else, so only a lambda of matching arity is judged; a block body is
/// not judged.
#[expect(clippy::too_many_arguments, reason = "one lambda judgement site with its proven expectation")]
fn judge_lambda_body(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    lambda_node: AstNodeId,
    lambda: &LambdaExpression,
    expected_parameters: &[Identity],
    expected_result: &Identity,
    findings: &mut Vec<ValueObligation>,
) {
    if lambda.parameters.len() != expected_parameters.len() {
        return;
    }
    let Some((site, body)) = expression_nodes(program, index, lambda_node, lambda.body.as_ref()) else { return };
    if index.kind(body) == Some(NodeKind::BlockExpression) {
        return;
    }
    let Some(actual) = value_identity(db, AstNodeKey { node: body, ..key }) else { return };
    judge(db, findings, &Destination::Identity(expected_result.clone()), &actual, AstNodeKey { node: site, ..key });
}

/// The function type a lambda denotes on its own (legacy `type_lambda_expression_with_expected`
/// without an expected signature): every parameter explicitly typed, and a proven non-block body.
fn lambda_own_signature(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    lambda_node: AstNodeId,
    lambda: &LambdaExpression,
) -> Option<Identity> {
    let lambda_key = AstNodeKey { node: lambda_node, ..key };
    let parameters = lambda
        .parameters
        .iter()
        .map(|parameter| {
            let ty = parameter.node.ty.as_ref()?;
            if type_syntax_is_enclosing_generic_parameter_reference(db, lambda_key, &ty.node) {
                return None;
            }
            generic_source_type_identity(db, lambda_key, &ty.node).ok()
        })
        .collect::<Option<Vec<_>>>()?;
    let (_, body) = expression_nodes(program, index, lambda_node, lambda.body.as_ref())?;
    if index.kind(body) == Some(NodeKind::BlockExpression) {
        return None;
    }
    let result = value_identity(db, AstNodeKey { node: body, ..key })?;
    Some(Identity::Function { parameters: parameters.into(), result: Box::new(result) })
}

/// When `subject` is a `match` expression, judge each value arm against the contextual
/// destination and return `true`; a primitive arm against a primitive destination is left to
/// `typing_obligations`.
fn judge_contextual_match(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    subject: AstNodeId,
    expected: &Destination,
    findings: &mut Vec<ValueObligation>,
) -> bool {
    let Some(expression) = index.node_at(program, subject).and_then(|node| node.of::<MatchExpression>()) else {
        return false;
    };
    for arm in &expression.arms {
        let Some(arm_node) = index.direct_child_id(program, subject, DynNodeRef::from(arm)) else { continue };
        let Some((site, value)) = expression_nodes(program, index, arm_node, &arm.node.value) else { continue };
        judge_value(db, program, index, key, expected, site, value, true, findings);
    }
    true
}

fn collect_let_values(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<ValueObligation>,
) {
    for statement in nodes_of_kind(index, key.node, NodeKind::LetStatement) {
        let Some(let_statement) = index.node_at(program, statement).and_then(|node| node.of::<LetStatement>()) else {
            continue;
        };
        let Some(annotation) = let_statement.type_annotation.as_ref() else { continue };
        let Some((_, subject)) = expression_nodes(program, index, statement, &let_statement.value) else { continue };
        let Some(expected) = declared_destination(db, AstNodeKey { node: statement, ..key }, &annotation.node) else {
            continue;
        };
        if judge_contextual_match(db, program, index, key, subject, &expected, findings) {
            continue;
        }
        let Some(name) = index.direct_child_id(program, statement, DynNodeRef::from(&let_statement.name)) else {
            continue;
        };
        judge_value(db, program, index, key, &expected, name, subject, primitive_destination(&annotation.node), findings);
    }
}

fn collect_return_values(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    item: DynNodeRef<'_>,
    findings: &mut Vec<ValueObligation>,
) {
    let (expected, primitive_expected) = if let Some(function) = item.of::<FunctionDefinition>() {
        if !function.generics.is_empty() {
            return;
        }
        let Some(declared) = function.return_type.as_ref() else { return };
        let Some(expected) = declared_destination(db, key, &declared.node) else { return };
        (expected, primitive_destination(&declared.node))
    } else if let Some(method) = item.of::<MethodDefinition>() {
        let Some(declared) = method.return_type.as_ref() else { return };
        let Some(expected) = declared_destination(db, key, &declared.node) else { return };
        (expected, primitive_destination(&declared.node))
    } else if item.of::<TestDefinition>().is_some() {
        // The legacy `TestDefinition` item typing sets `current_return_type` to `unit`.
        (Destination::Identity(Identity::Abi(SemanticTypeId::UNIT)), true)
    } else {
        return;
    };
    for statement in nodes_of_kind(index, key.node, NodeKind::ReturnStatement) {
        if enclosing_callable(index, statement) != Some(key.node) {
            continue;
        }
        let Some(return_statement) = index.node_at(program, statement).and_then(|node| node.of::<ReturnStatement>())
        else {
            continue;
        };
        let Some(value) = return_statement.value.as_ref() else { continue };
        let Some((_, subject)) = expression_nodes(program, index, statement, value) else { continue };
        if judge_contextual_match(db, program, index, key, subject, &expected, findings) {
            continue;
        }
        judge_value(db, program, index, key, &expected, statement, subject, primitive_expected, findings);
    }
}

/// Local assignments (`=`), and event subscriptions (`+=`, `-=`) whose handler's function type
/// is compared with the event field's declared type at the assignment, as the legacy
/// `require_same_type(assign.span, target, value)` does for every assignment before the
/// operator is dispatched.
fn collect_assignment_values(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<ValueObligation>,
) {
    for assignment in nodes_of_kind(index, key.node, NodeKind::AssignExpression) {
        let Some(assign) = index.node_at(program, assignment).and_then(|node| node.of::<AssignExpression>()) else {
            continue;
        };
        let assignment_key = AstNodeKey { node: assignment, ..key };
        let Some((_, value)) = expression_nodes(program, index, assignment, assign.value.as_ref()) else { continue };
        let value_key = AstNodeKey { node: value, ..key };
        match assign.op.node {
            AssignOp::Assign => {
                if !matches!(assign.target.node, Expression::Path(_)) {
                    continue;
                }
                let Some((_, target)) = expression_nodes(program, index, assignment, assign.target.as_ref()) else {
                    continue;
                };
                let target = AstNodeKey { node: target, ..key };
                if proven_primitive(db, target) && proven_primitive(db, value_key) {
                    continue;
                }
                let Some(expected) = value_identity(db, target) else { continue };
                let Some(actual) = value_identity(db, value_key) else { continue };
                judge(db, findings, &Destination::Identity(expected), &actual, assignment_key);
            }
            AssignOp::AddAssign | AssignOp::SubAssign => {
                let Ok(Some(fact)) = event_operation(db, assignment_key) else { continue };
                let Some(field_syntax) = db.syntax_unit(fact.field.unit).filter(|syntax| syntax.accepts_key(db, fact.field))
                else {
                    continue;
                };
                let Some(field) = field_syntax
                    .syntax_index(db)
                    .node_at(field_syntax.expanded_program(db), fact.field.node)
                    .and_then(|node| node.of::<beskid_analysis::syntax::Field>())
                else {
                    continue;
                };
                if owner_generics_mentioned(db, fact.declaration, &field.ty.node) {
                    continue;
                }
                let Some(expected) = declared_destination(db, fact.field, &field.ty.node) else { continue };
                let actual = match index.node_at(program, value).and_then(|node| node.of::<LambdaExpression>()) {
                    Some(lambda) => lambda_own_signature(db, program, index, key, value, lambda),
                    None => value_identity(db, value_key),
                };
                let Some(actual) = actual else { continue };
                judge(db, findings, &expected, &actual, assignment_key);
            }
            _ => {}
        }
    }
}

/// Whether `ty`, declared inside the `type` definition at `owner`, mentions one of the owner's
/// generic parameters.
fn owner_generics_mentioned(db: &dyn Db, owner: AstNodeKey, ty: &Type) -> bool {
    type_definition(db, owner).is_some_and(|definition| {
        definition.generics.iter().any(|generic| type_syntax_mentions_generic_parameter(ty, &generic.node.name))
    })
}

/// The call argument sites and their normalized value nodes, in source order.
fn call_argument_sites(
    program: &Program,
    index: &SyntaxIndex,
    call_node: AstNodeId,
    call: &CallExpression,
) -> Option<Vec<(AstNodeId, AstNodeId)>> {
    call.args.iter().map(|argument| expression_nodes(program, index, call_node, argument)).collect()
}

/// The proven substitution of a generic callee's parameters at one call: the explicit type
/// arguments of the callee path (legacy `generic_args` from the terminal segment), or else the
/// call's proven specialization. Explicit arguments are resolved in the calling unit; the
/// parameter type they are substituted into stays resolved in the declaring unit.
fn callee_substitutions(
    db: &dyn Db,
    call_key: AstNodeKey,
    call: &CallExpression,
    declaration: AstNodeKey,
    generics: &[&str],
    explicit_generics: bool,
) -> Option<Vec<(String, Identity)>> {
    if explicit_generics
        && let Expression::Path(path) = &call.callee.node
        && let Some(arguments) = explicit_generic_type_argument_syntax(&path.node.path.node)
    {
        if arguments.len() != generics.len() {
            return None;
        }
        return generics
            .iter()
            .zip(arguments)
            .map(|(generic, argument)| {
                generic_source_type_identity(db, call_key, &argument.node).ok().map(|identity| ((*generic).to_owned(), identity))
            })
            .collect();
    }
    let specialization = generic_specialization_instance_for_call(db, call_key).ok()?;
    if specialization.declaration != declaration {
        return None;
    }
    Some(
        specialization
            .substitutions
            .iter()
            .map(|binding| (binding.parameter.to_string(), binding.source_identity().clone()))
            .collect(),
    )
}

/// Direct calls to a function, a method, or a contract method signature (a contract-typed or
/// bounded receiver, legacy `ContractDispatch`), without a `bulk` parameter. A pair
/// `typing_obligations` judges (a path-callee non-generic function whose parameters are all
/// primitive, a primitive parameter, and a proven primitive argument) is left to it.
fn collect_call_argument_values(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<ValueObligation>,
) {
    for call_node in nodes_of_kind(index, key.node, NodeKind::CallExpression) {
        let Some(call) = index.node_at(program, call_node).and_then(|node| node.of::<CallExpression>()) else {
            continue;
        };
        let call_key = AstNodeKey { node: call_node, ..key };
        let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(db, call_key) else { continue };
        let Some(declaration_syntax) = db.syntax_unit(declaration.unit) else { continue };
        if !declaration_syntax.accepts_key(db, AstNodeKey { node: declaration.node, ..declaration }) {
            continue;
        }
        let declaration_index = declaration_syntax.syntax_index(db);
        let declaration_program = declaration_syntax.expanded_program(db);
        let Some(declaration_node) = declaration_index.node_at(declaration_program, declaration.node) else {
            continue;
        };
        let generics = generic_callable_parameters(db, declaration).map(|(names, _)| names).unwrap_or_default();
        let (parameters, receiver, typing_judged, explicit_generics, contract_member) =
            if let Some(function) = declaration_node.of::<FunctionDefinition>() {
                let typing_judged = function.generics.is_empty()
                    && matches!(call.callee.node, Expression::Path(_))
                    && function
                        .parameters
                        .iter()
                        .all(|parameter| semantic_type_from_syntax(&parameter.node.ty.node).is_ok());
                (&function.parameters, 0, typing_judged, true, false)
            } else if let Some(method) = declaration_node.of::<MethodDefinition>() {
                (&method.parameters, 1, false, false, false)
            } else if let Some(signature) = declaration_node.of::<ContractMethodSignature>() {
                // `call_arguments` carries the receiver only for a member callee or a path the
                // contract member receiver authority claims; a contract-as-namespace call has none.
                let receiver = match &call.callee.node {
                    Expression::Member(_) => 1,
                    Expression::Path(path)
                        if contract_member_receiver(db, program, index, call_key, &path.node.path.node).is_some() =>
                    {
                        1
                    }
                    _ => 0,
                };
                (&signature.parameters, receiver, false, false, true)
            } else {
                continue;
            };
        if parameters.iter().any(|parameter| parameter.node.bulk) {
            continue;
        }
        let Ok(Some(arguments)) = call_arguments(db, call_key) else { continue };
        if arguments.len() != parameters.len() + receiver {
            continue;
        }
        let Some(sites) = call_argument_sites(program, index, call_node, call) else { continue };
        if sites.len() != parameters.len() {
            continue;
        }
        let needs_substitution = parameters.iter().any(|parameter| {
            generics.iter().any(|generic| type_syntax_mentions_generic_parameter(&parameter.node.ty.node, generic))
        });
        let substitutions =
            needs_substitution.then(|| callee_substitutions(db, call_key, call, declaration, &generics, explicit_generics)).flatten();
        let substitution_map =
            substitutions.as_ref().map(|bindings| bindings.iter().map(|(name, identity)| (name.as_str(), identity)).collect::<HashMap<_, _>>());
        for (parameter, (site, subject)) in parameters.iter().zip(sites) {
            let ty = &parameter.node.ty.node;
            // `This` in a contract member is the receiver's own type, fixed only by a witness.
            if contract_member && type_syntax_mentions_this(ty) {
                continue;
            }
            let Some(expected) = generic_declared_destination(db, declaration, &generics, ty, substitution_map.as_ref())
            else {
                continue;
            };
            judge_value(db, program, index, key, &expected, site, subject, typing_judged && primitive_destination(ty), findings);
        }
    }
}

fn type_syntax_mentions_this(ty: &Type) -> bool {
    match ty {
        Type::This | Type::Associated { .. } => true,
        Type::Primitive(_) => false,
        Type::Complex(path) => path
            .node
            .segments
            .iter()
            .any(|segment| segment.node.type_args.iter().any(|argument| type_syntax_mentions_this(&argument.node))),
        Type::Array(element) => type_syntax_mentions_this(&element.node),
        Type::Function { return_type, parameters } => {
            type_syntax_mentions_this(&return_type.node)
                || parameters.iter().any(|parameter| type_syntax_mentions_this(&parameter.node))
        }
    }
}

/// Calls through a local or parameter of declared function type (legacy `CallableValueCall`):
/// each argument against the declared parameter type (site: the argument).
fn collect_callable_value_call_values(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<ValueObligation>,
) {
    for call_node in nodes_of_kind(index, key.node, NodeKind::CallExpression) {
        let Some(call) = index.node_at(program, call_node).and_then(|node| node.of::<CallExpression>()) else {
            continue;
        };
        let call_key = AstNodeKey { node: call_node, ..key };
        let Some((parameters, _)) = callable_value_signature(db, program, index, call_key, call) else { continue };
        let Some(sites) = call_argument_sites(program, index, call_node, call) else { continue };
        if sites.len() != parameters.len() {
            continue;
        }
        for (parameter, (site, subject)) in parameters.iter().zip(sites) {
            judge_value(db, program, index, key, &Destination::Identity(parameter.clone()), site, subject, false, findings);
        }
    }
}

/// Event raises `receiver.Event(args)` (legacy `EventInvoke` path of `type_call_expression`):
/// each argument against the event field's declared parameter type (site: the argument). The
/// `event_operation` fact is absent for a raise whose arguments do not match, so the declaring
/// field is reached through the field access receiver of the callee path.
fn collect_event_raise_values(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<ValueObligation>,
) {
    for call_node in nodes_of_kind(index, key.node, NodeKind::CallExpression) {
        let Some(call) = index.node_at(program, call_node).and_then(|node| node.of::<CallExpression>()) else {
            continue;
        };
        match &call.callee.node {
            Expression::Member(_) => {}
            Expression::Path(path) => {
                let [receiver, field] = path.node.path.node.segments.as_slice() else { continue };
                if !receiver.node.type_args.is_empty() || !field.node.type_args.is_empty() {
                    continue;
                }
            }
            _ => continue,
        }
        let Some(callee) = index.direct_child_id(program, call_node, DynNodeRef::from(call.callee.as_ref())) else {
            continue;
        };
        let callee = normalized_expression_node(index, callee);
        let Some(reference) = index.node_at(program, callee) else { continue };
        let callee_key = AstNodeKey { node: callee, ..key };
        let Some(Ok(receiver)) = field_access_receiver(db, program, index, callee_key, reference, None, None) else {
            continue;
        };
        let Some(definition) = type_definition(db, receiver.declaration) else { continue };
        let Some(field) = definition
            .fields
            .iter()
            .find(|field| field.node.kind == FieldKind::Event && field.node.name.node.name == receiver.field_name)
        else {
            continue;
        };
        let Type::Function { parameters, .. } = &field.node.ty.node else { continue };
        if definition.generics.iter().any(|generic| {
            parameters.iter().any(|parameter| type_syntax_mentions_generic_parameter(&parameter.node, &generic.node.name))
        }) {
            continue;
        }
        let Some(sites) = call_argument_sites(program, index, call_node, call) else { continue };
        if sites.len() != parameters.len() {
            continue;
        }
        for (parameter, (site, subject)) in parameters.iter().zip(sites) {
            let Some(expected) = declared_destination(db, receiver.declaration, &parameter.node) else { continue };
            judge_value(db, program, index, key, &expected, site, subject, false, findings);
        }
    }
}

/// The declared type of the position a literal at `node` is written into: a typed `let`, the
/// `return` of a non-generic function or method, or a call parameter whose declared type the
/// call fixes. Transparent expression wrappers are climbed; any other context has no
/// destination.
fn contextual_destination_type(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    node: AstNodeId,
) -> Option<(AstNodeKey, Type)> {
    let mut position = node;
    let context = loop {
        let parent = parent_node(index, position)?;
        match index.kind(parent)? {
            NodeKind::Expression | NodeKind::GroupedExpression => position = parent,
            _ => break parent,
        }
    };
    let context_key = AstNodeKey { node: context, ..key };
    let context_node = index.node_at(program, context)?;
    if let Some(statement) = context_node.of::<LetStatement>() {
        return statement.type_annotation.as_ref().map(|annotation| (context_key, annotation.node.clone()));
    }
    if context_node.of::<ReturnStatement>().is_some() {
        let callable = enclosing_callable(index, context)?;
        let callable_key = AstNodeKey { node: callable, ..key };
        let callable_node = index.node_at(program, callable)?;
        let declared = if let Some(function) = callable_node.of::<FunctionDefinition>() {
            if !function.generics.is_empty() {
                return None;
            }
            function.return_type.as_ref()?
        } else if let Some(method) = callable_node.of::<MethodDefinition>() {
            if generic_callable_parameters(db, callable_key).is_some() {
                return None;
            }
            method.return_type.as_ref()?
        } else {
            return None;
        };
        return Some((callable_key, declared.node.clone()));
    }
    if context_node.of::<CallExpression>().is_some() {
        let expected = expected_explicit_call_argument_type(db, program, index, AstNodeKey { node: position, ..key })?;
        return Some((context_key, expected));
    }
    None
}

/// The generic arguments a nominal literal of `declaration` (a generic `type` or `enum` with
/// `generics`) takes: from `spelled` (its own type path with explicit arguments) when it names
/// them, otherwise from the contextual destination resolving to the same declaration.
#[expect(clippy::too_many_arguments, reason = "one literal argument authority with its declaration facts")]
fn literal_generic_arguments(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    node: AstNodeId,
    declaration: AstNodeKey,
    generics: &[&str],
    spelled: &Type,
) -> Option<Vec<Identity>> {
    let qualified_name = stable_declaration_identity(db, declaration)?;
    let literal_key = AstNodeKey { node, ..key };
    let applied = |scope: AstNodeKey, ty: &Type| match generic_source_type_identity(db, scope, ty).ok()? {
        Identity::Nominal { qualified_name: name, arguments } if name == qualified_name && arguments.len() == generics.len() => {
            Some(arguments.to_vec())
        }
        _ => None,
    };
    if let Type::Complex(path) = spelled
        && path.node.segments.iter().any(|segment| !segment.node.type_args.is_empty())
    {
        return applied(literal_key, spelled);
    }
    let (scope, ty) = contextual_destination_type(db, program, index, key, node)?;
    if type_syntax_is_enclosing_generic_parameter_reference(db, scope, &ty) {
        return None;
    }
    applied(scope, &ty)
}

fn collect_struct_literal_values(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<ValueObligation>,
) {
    for node in nodes_of_kind(index, key.node, NodeKind::StructLiteralExpression) {
        let Some(literal) = index.node_at(program, node).and_then(|node| node.of::<StructLiteralExpression>()) else {
            continue;
        };
        let literal_key = AstNodeKey { node, ..key };
        let Some(declaration) = resolve_nominal_layout_declaration(db, program, index, literal_key, &literal.path.node)
        else {
            continue;
        };
        let Some(definition) = type_definition(db, declaration) else { continue };
        let generics = definition.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>();
        let arguments = (!generics.is_empty())
            .then(|| {
                literal_generic_arguments(
                    db,
                    program,
                    index,
                    key,
                    node,
                    declaration,
                    &generics,
                    &Type::Complex(literal.path.clone()),
                )
            })
            .flatten();
        let substitutions = arguments
            .as_ref()
            .map(|arguments| generics.iter().copied().zip(arguments.iter()).collect::<HashMap<_, _>>());
        for field in &literal.fields {
            let Some(declared) =
                definition.fields.iter().find(|declared| declared.node.name.node.name == field.node.name.node.name)
            else {
                continue;
            };
            if declared.node.kind != FieldKind::Value {
                continue;
            }
            let Some(field_node) = index.direct_child_id(program, node, DynNodeRef::from(field)) else { continue };
            let Some((site, value)) = expression_nodes(program, index, field_node, &field.node.value) else { continue };
            let Some(expected) =
                generic_declared_destination(db, declaration, &generics, &declared.node.ty.node, substitutions.as_ref())
            else {
                continue;
            };
            judge_value(db, program, index, key, &expected, site, value, false, findings);
        }
    }
}

fn collect_enum_constructor_values(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<ValueObligation>,
) {
    for node in nodes_of_kind(index, key.node, NodeKind::EnumConstructorExpression) {
        let Some(constructor) = index.node_at(program, node).and_then(|node| node.of::<EnumConstructorExpression>())
        else {
            continue;
        };
        let site = AstNodeKey { node, ..key };
        let own_path = &constructor.path.node.type_path;
        let type_path = contextual_enum_constructor_type_path(db, program, index, site, constructor)
            .map(|path| Spanned::new(path, own_path.span))
            .unwrap_or_else(|| own_path.clone());
        let Some(declaration) = resolve_type_declaration(db, site, &type_path.node) else { continue };
        let Some(definition) = enum_definition(db, declaration) else { continue };
        let generics = definition.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>();
        let arguments = (!generics.is_empty())
            .then(|| literal_generic_arguments(db, program, index, key, node, declaration, &generics, &Type::Complex(type_path)))
            .flatten();
        let substitutions = arguments
            .as_ref()
            .map(|arguments| generics.iter().copied().zip(arguments.iter()).collect::<HashMap<_, _>>());
        let variant_name = constructor.path.node.variant.node.name.as_str();
        let Some(variant) = definition.variants.iter().find(|variant| variant.node.name.node.name == variant_name)
        else {
            continue;
        };
        if variant.node.fields.len() != constructor.args.len() {
            continue;
        }
        for (field, argument) in variant.node.fields.iter().zip(&constructor.args) {
            let Some((argument_site, value)) = expression_nodes(program, index, node, argument) else { continue };
            let Some(expected) =
                generic_declared_destination(db, declaration, &generics, &field.node.ty.node, substitutions.as_ref())
            else {
                continue;
            };
            judge_value(db, program, index, key, &expected, argument_site, value, false, findings);
        }
    }
}

/// Literal and enum patterns of every `match` against the scrutinee's identity, recursively
/// through enum pattern payloads.
fn collect_pattern_values(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<ValueObligation>,
) {
    for node in nodes_of_kind(index, key.node, NodeKind::MatchExpression) {
        let Some(expression) = index.node_at(program, node).and_then(|node| node.of::<MatchExpression>()) else {
            continue;
        };
        let Some((_, scrutinee)) = expression_nodes(program, index, node, &expression.scrutinee) else { continue };
        let Some(expected) = value_identity(db, AstNodeKey { node: scrutinee, ..key }) else { continue };
        for arm in &expression.arms {
            let Some(arm_node) = index.direct_child_id(program, node, DynNodeRef::from(arm)) else { continue };
            let Some(pattern_node) = index.direct_child_id(program, arm_node, DynNodeRef::from(&arm.node.pattern))
            else {
                continue;
            };
            judge_pattern(db, program, index, key, &expected, &arm.node.pattern.node, pattern_node, findings);
        }
    }
}

/// One pattern against the identity of the value it destructures (legacy `type_pattern` for an
/// enum pattern, `type_pattern_with_expected` for a literal; site: the pattern). An enum
/// pattern's payload patterns are judged against the variant's declared field types, with the
/// enum's generic parameters substituted from the scrutinee identity's arguments or the
/// pattern path's explicit arguments; a payload whose declared type mentions an unfixed generic
/// is not judged. A payload count that differs from the variant is reported elsewhere (E1307).
#[expect(clippy::too_many_arguments, reason = "one recursive pattern judgement with its proven expectation")]
fn judge_pattern(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    expected: &Identity,
    pattern: &Pattern,
    pattern_node: AstNodeId,
    findings: &mut Vec<ValueObligation>,
) {
    let pattern_site = AstNodeKey { node: pattern_node, ..key };
    match pattern {
        Pattern::Literal(literal) => {
            let actual = Identity::Abi(semantic_type_for_literal(&literal.node));
            judge(db, findings, &Destination::Identity(expected.clone()), &actual, pattern_site);
        }
        Pattern::Enum(enum_pattern) => {
            let type_path = &enum_pattern.node.path.node.type_path;
            let Some(declaration) = resolve_type_declaration(db, pattern_site, &type_path.node) else { return };
            let Some(qualified_name) = stable_declaration_identity(db, declaration) else { return };
            let actual = Identity::Nominal { qualified_name: Arc::clone(&qualified_name), arguments: Arc::from([]) };
            judge(db, findings, &Destination::Identity(expected.clone()), &actual, pattern_site);
            let Some(definition) = enum_definition(db, declaration) else { return };
            let generics = definition.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>();
            let variant_name = enum_pattern.node.path.node.variant.node.name.as_str();
            let Some(variant) = definition.variants.iter().find(|variant| variant.node.name.node.name == variant_name)
            else {
                return;
            };
            if variant.node.fields.len() != enum_pattern.node.items.len() {
                return;
            }
            let arguments = match expected {
                Identity::Nominal { qualified_name: name, arguments }
                    if *name == qualified_name && arguments.len() == generics.len() =>
                {
                    Some(arguments.to_vec())
                }
                _ => match generic_source_type_identity(db, pattern_site, &Type::Complex(type_path.clone())).ok() {
                    Some(Identity::Nominal { arguments, .. }) if arguments.len() == generics.len() => {
                        Some(arguments.to_vec())
                    }
                    _ => None,
                },
            };
            let substitutions = arguments
                .as_ref()
                .map(|arguments| generics.iter().copied().zip(arguments.iter()).collect::<HashMap<_, _>>());
            let enum_node = index.direct_child_id(program, pattern_node, DynNodeRef::from(enum_pattern)).unwrap_or(pattern_node);
            for (field, item) in variant.node.fields.iter().zip(&enum_pattern.node.items) {
                let Some(item_node) = index.direct_child_id(program, enum_node, DynNodeRef::from(item)) else { continue };
                let Some(Destination::Identity(expected_item)) =
                    generic_declared_destination(db, declaration, &generics, &field.node.ty.node, substitutions.as_ref())
                else {
                    continue;
                };
                judge_pattern(db, program, index, key, &expected_item, &item.node, item_node, findings);
            }
        }
        Pattern::Identifier(_) | Pattern::Wildcard => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abi(ty: SemanticTypeId) -> Identity {
        Identity::Abi(ty)
    }

    fn nominal(name: &str) -> Identity {
        Identity::Nominal { qualified_name: Arc::from(name), arguments: Arc::from([]) }
    }

    #[test]
    fn unit_destination_rejects_nominal_values_and_accepts_never() {
        assert!(!identities_compatible(&abi(SemanticTypeId::UNIT), &nominal("Pair")));
        assert!(identities_compatible(&abi(SemanticTypeId::UNIT), &abi(SemanticTypeId::NEVER)));
        assert!(identities_compatible(&abi(SemanticTypeId::UNIT), &abi(SemanticTypeId::POINTER)));
    }

    #[test]
    fn function_identities_compare_structurally_and_leniently() {
        let function = |parameters: Vec<Identity>, result: Identity| Identity::Function {
            parameters: parameters.into(),
            result: Box::new(result),
        };
        let expected = function(vec![abi(SemanticTypeId::I64)], abi(SemanticTypeId::BOOL));
        assert!(identities_compatible(&expected, &function(vec![abi(SemanticTypeId::I64)], abi(SemanticTypeId::BOOL))));
        assert!(!identities_compatible(&expected, &function(vec![abi(SemanticTypeId::BOOL)], abi(SemanticTypeId::BOOL))));
        assert!(!identities_compatible(&expected, &function(vec![], abi(SemanticTypeId::BOOL))));
        assert!(!identities_compatible(&expected, &function(vec![abi(SemanticTypeId::I64)], nominal("Pair"))));
        assert!(identities_compatible(&expected, &abi(SemanticTypeId::I64)));
    }
}
