//! E1206 value obligations of one item that `typing_obligations` does not judge: positions whose
//! destination or value is a nominal, array, or function type, and the value positions the
//! proven-primitive fact has no destination for.
//!
//! `typing_obligations` judges proven primitives only, because `SemanticTypeId` represents every
//! nominal value as `pointer`. This fact judges the same legacy `require_same_type` sites on the
//! source type identity instead (`generic_source_type_identity` for a declared destination,
//! `generic_source_expression_identity` for a value), the identity authority generic
//! specialization already uses:
//!
//! * a typed `let` initializer (site: the declared name), a `return` value against the declared
//!   result (site: the return statement), and a local assignment (site: the assignment), when one
//!   side is not a proven primitive;
//! * every value arm of a `match` that is the direct initializer of a typed `let` or the value of
//!   a `return`, against that contextual destination (site: the arm value), as the legacy
//!   `type_match_expression_with_expected` does;
//! * direct call arguments against a non-generic function's or method's declared parameters
//!   (site: the argument), when `typing_obligations` does not already judge the pair;
//! * struct literal field values against the declared field types, and enum constructor
//!   arguments against the variant's declared field types (site: the value);
//! * literal and enum patterns against the scrutinee (site: the pattern).
//!
//! Compatibility follows the legacy `require_same_type`: two nominal values are compatible when
//! they name the same declaration (generic arguments are not compared, as `named_item_id` does
//! not compare them); arrays when their proven primitive elements are equal; `u8[]` and `i64`
//! share the byte-array representation; `never`, `pointer`, and function values are never
//! judged; two primitives follow `typing::compatible`.
//!
//! The fact is fail-closed: a destination that is a contract (conformance is not judged here), a
//! generic parameter, or anything `generic_source_type_identity` cannot prove is never judged, and
//! neither is a value whose identity is unavailable. Generic callees and generic `type`/`enum`
//! declarations are not judged.

use super::members::enum_definition;
use super::typing::{compatible, enclosing_callable, expression_nodes, nodes_of_kind, proven_type};
use super::*;
use beskid_analysis::syntax::{
    AssignExpression, AssignOp, AstNodeId, CallExpression, EnumConstructorExpression, Expression, FieldKind,
    FunctionDefinition, LetStatement, MatchExpression, MethodDefinition, Pattern, ReturnStatement, Spanned,
    StructLiteralExpression, Type, TypeDefinition,
};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

type Identity = GenericSourceTypeIdentity;

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
        collect_struct_literal_values(db, program, index, key, &mut findings);
        collect_enum_constructor_values(db, program, index, key, &mut findings);
        collect_pattern_values(db, program, index, key, &mut findings);
        findings.sort_by_key(|finding| finding.site.node.0);
        findings.dedup();
        Some(Arc::from(findings))
    })
}

/// Whether `ty` is a primitive other than `pointer`: the destinations `typing_obligations` judges.
fn primitive_destination(ty: &Type) -> bool {
    semantic_type_from_syntax(ty).is_ok_and(|ty| ty != SemanticTypeId::POINTER)
}

/// Whether `node_type` proves `value` a primitive other than `pointer` (the ABI of nominal values).
fn proven_primitive(db: &dyn Db, value: AstNodeKey) -> bool {
    proven_type(db, value).is_some_and(|ty| ty != SemanticTypeId::POINTER)
}

/// The identity of a declared destination type, resolved in the declaring unit at `scope`.
/// Contracts and generic parameters are not judged.
fn declared_identity(db: &dyn Db, scope: AstNodeKey, ty: &Type) -> Option<Identity> {
    if type_syntax_is_enclosing_generic_parameter_reference(db, scope, ty) {
        return None;
    }
    if let Type::Complex(path) = ty
        && resolve_contract(db, scope, &path.node).is_some()
    {
        return None;
    }
    generic_source_type_identity(db, scope, ty).ok()
}

/// The identity of a value: its proven primitive, or else its source expression identity.
fn value_identity(db: &dyn Db, value: AstNodeKey) -> Option<Identity> {
    match proven_type(db, value) {
        Some(ty) if ty != SemanticTypeId::POINTER => Some(Identity::Abi(ty)),
        _ => generic_source_expression_identity(db, value).ok(),
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

fn judge(findings: &mut Vec<ValueObligation>, expected: &Identity, actual: &Identity, site: AstNodeKey) {
    if !identities_compatible(expected, actual) {
        findings.push(ValueObligation {
            expected: Arc::from(identity_name(expected).as_str()),
            actual: Arc::from(identity_name(actual).as_str()),
            site,
        });
    }
}

/// When `subject` is a `match` expression, judge each value arm against the contextual
/// destination and return `true`; a primitive arm against a primitive destination is left to
/// `typing_obligations`.
fn judge_contextual_match(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    subject: AstNodeId,
    expected: &Identity,
    findings: &mut Vec<ValueObligation>,
) -> bool {
    let Some(expression) = index.node_at(program, subject).and_then(|node| node.of::<MatchExpression>()) else {
        return false;
    };
    for arm in &expression.arms {
        let Some(arm_node) = index.direct_child_id(program, subject, DynNodeRef::from(arm)) else { continue };
        let Some((site, value)) = expression_nodes(program, index, arm_node, &arm.node.value) else { continue };
        let value = AstNodeKey { node: value, ..key };
        if matches!(expected, Identity::Abi(_)) && proven_primitive(db, value) {
            continue;
        }
        let Some(actual) = value_identity(db, value) else { continue };
        judge(findings, expected, &actual, AstNodeKey { node: site, ..key });
    }
    true
}

fn collect_let_values(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
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
        let Some(expected) = declared_identity(db, AstNodeKey { node: statement, ..key }, &annotation.node) else {
            continue;
        };
        if judge_contextual_match(db, program, index, key, subject, &expected, findings) {
            continue;
        }
        let subject = AstNodeKey { node: subject, ..key };
        if primitive_destination(&annotation.node) && proven_primitive(db, subject) {
            continue;
        }
        let Some(actual) = value_identity(db, subject) else { continue };
        let Some(name) = index.direct_child_id(program, statement, DynNodeRef::from(&let_statement.name)) else {
            continue;
        };
        judge(findings, &expected, &actual, AstNodeKey { node: name, ..key });
    }
}

fn collect_return_values(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    item: DynNodeRef<'_>,
    findings: &mut Vec<ValueObligation>,
) {
    let declared = if let Some(function) = item.of::<FunctionDefinition>() {
        if !function.generics.is_empty() {
            return;
        }
        function.return_type.as_ref()
    } else if let Some(method) = item.of::<MethodDefinition>() {
        method.return_type.as_ref()
    } else {
        return;
    };
    let Some(declared) = declared else { return };
    let Some(expected) = declared_identity(db, key, &declared.node) else { return };
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
        let subject = AstNodeKey { node: subject, ..key };
        if primitive_destination(&declared.node) && proven_primitive(db, subject) {
            continue;
        }
        let Some(actual) = value_identity(db, subject) else { continue };
        judge(findings, &expected, &actual, AstNodeKey { node: statement, ..key });
    }
}

fn collect_assignment_values(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<ValueObligation>,
) {
    for assignment in nodes_of_kind(index, key.node, NodeKind::AssignExpression) {
        let Some(assign) = index.node_at(program, assignment).and_then(|node| node.of::<AssignExpression>()) else {
            continue;
        };
        if assign.op.node != AssignOp::Assign || !matches!(assign.target.node, Expression::Path(_)) {
            continue;
        }
        let Some((_, target)) = expression_nodes(program, index, assignment, assign.target.as_ref()) else { continue };
        let Some((_, value)) = expression_nodes(program, index, assignment, assign.value.as_ref()) else { continue };
        let (target, value) = (AstNodeKey { node: target, ..key }, AstNodeKey { node: value, ..key });
        if proven_primitive(db, target) && proven_primitive(db, value) {
            continue;
        }
        let Some(expected) = value_identity(db, target) else { continue };
        let Some(actual) = value_identity(db, value) else { continue };
        judge(findings, &expected, &actual, AstNodeKey { node: assignment, ..key });
    }
}

/// Direct calls to a non-generic, non-`bulk` function or method. A pair `typing_obligations`
/// judges (a path-callee function whose parameters are all primitive, a primitive parameter, and
/// a proven primitive argument) is left to it.
fn collect_call_argument_values(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
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
        let (parameters, receiver, typing_judged) = if let Some(function) = declaration_node.of::<FunctionDefinition>()
        {
            if !function.generics.is_empty() {
                continue;
            }
            let typing_judged = matches!(call.callee.node, Expression::Path(_))
                && function
                    .parameters
                    .iter()
                    .all(|parameter| semantic_type_from_syntax(&parameter.node.ty.node).is_ok());
            (&function.parameters, 0, typing_judged)
        } else if let Some(method) = declaration_node.of::<MethodDefinition>() {
            (&method.parameters, 1, false)
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
        for (parameter, argument) in parameters.iter().zip(arguments.iter().skip(receiver)) {
            if typing_judged && primitive_destination(&parameter.node.ty.node) && proven_primitive(db, *argument) {
                continue;
            }
            let Some(expected) = declared_identity(db, declaration, &parameter.node.ty.node) else { continue };
            let Some(actual) = value_identity(db, *argument) else { continue };
            judge(findings, &expected, &actual, *argument);
        }
    }
}

fn collect_struct_literal_values(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
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
        let Some(declaration_syntax) =
            db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))
        else {
            continue;
        };
        let Some(definition) = declaration_syntax
            .syntax_index(db)
            .node_at(declaration_syntax.expanded_program(db), declaration.node)
            .and_then(|node| node.of::<TypeDefinition>())
        else {
            continue;
        };
        if !definition.generics.is_empty() {
            continue;
        }
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
            let Some(expected) = declared_identity(db, declaration, &declared.node.ty.node) else { continue };
            let Some(actual) = value_identity(db, AstNodeKey { node: value, ..key }) else { continue };
            judge(findings, &expected, &actual, AstNodeKey { node: site, ..key });
        }
    }
}

fn collect_enum_constructor_values(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
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
        let type_path = contextual_enum_constructor_type_path(db, program, index, site, constructor)
            .unwrap_or_else(|| constructor.path.node.type_path.node.clone());
        let Some(declaration) = resolve_type_declaration(db, site, &type_path) else { continue };
        let Some(definition) = enum_definition(db, declaration) else { continue };
        if !definition.generics.is_empty() {
            continue;
        }
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
            let Some(expected) = declared_identity(db, declaration, &field.node.ty.node) else { continue };
            let Some(actual) = value_identity(db, AstNodeKey { node: value, ..key }) else { continue };
            judge(findings, &expected, &actual, AstNodeKey { node: argument_site, ..key });
        }
    }
}

/// Top-level literal and enum patterns of every `match` against the scrutinee's identity.
fn collect_pattern_values(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
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
            let pattern_site = AstNodeKey { node: pattern_node, ..key };
            let actual = match &arm.node.pattern.node {
                Pattern::Literal(literal) => Identity::Abi(semantic_type_for_literal(&literal.node)),
                Pattern::Enum(pattern) => {
                    let type_path = &pattern.node.path.node.type_path.node;
                    let Some(declaration) = resolve_type_declaration(db, pattern_site, type_path) else { continue };
                    let Some(qualified_name) = stable_declaration_identity(db, declaration) else { continue };
                    Identity::Nominal { qualified_name, arguments: Arc::from([]) }
                }
                Pattern::Identifier(_) | Pattern::Wildcard => continue,
            };
            judge(findings, &expected, &actual, pattern_site);
        }
    }
}
