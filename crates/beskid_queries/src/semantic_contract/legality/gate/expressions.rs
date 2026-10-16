//! Expression-shape, member, call, and local-declaration obligations (E1102, E1201, E1202,
//! E1204, E1211, E1213, E1228, E1250, E1606, E1610).
//!
//! Legacy sites mirrored here: `type_index_expression` and `type_array_literal_expression`
//! (E1202 at the expression), `type_lambda_expression_with_expected` without an expected
//! signature (E1202 at the parameter), the primitive conversion argument check (E1228 at the
//! argument), `type_member_expression` and `type_struct_field_path` (E1213 at the member
//! expression or path segment, E1201 for a method named as a value, E1211 at the member name of
//! an unknown method call; for field paths of three or more segments, `field_path_failure` walks
//! the declared field types: E1213, E1211, or E1201 at the first failing segment), `type_struct_literal_expression` and
//! `type_enum_constructor_expression` (E1201 at the literal or constructor), the explicit
//! type-argument count check of `type_call_expression` (E1204 at the call), the non-callable
//! field call (E1606 at the call), a method call through a local or `this` receiver path that no
//! method claims (`type_struct_field_path` then no signature: E1213 or E1211 at the member
//! segment, E1606 at the call), `check_generic_function_bounds` (E1610 at the call) and the
//! resolver's `declare_local` (E1102 at the second declaration).

use super::super::members::{declared_member_names, is_call_callee};
use super::super::typing::{is_numeric, nodes_of_kind};
use super::*;
use beskid_analysis::syntax::{
    ArrayLiteralExpression, CallExpression, EnumConstructorExpression, EnumDefinition, Expression,
    ExtendTypeDefinition, FunctionDefinition, IndexExpression, LambdaParameter, LetStatement, MemberExpression,
    Parameter, PathExpression, StructLiteralExpression, Type, TypeDefinition, Visibility,
};
use beskid_analysis::syntax_query::DynNodeRef;

pub(super) fn collect_expression_obligations(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<GateObligation>,
) {
    for node in nodes_of_kind(index, key.node, NodeKind::IndexExpression) {
        let Some(expression) = index.node_at(program, node).and_then(|node| node.of::<IndexExpression>()) else {
            continue;
        };
        let Some((_, _, target)) = proven_operand(db, program, index, key, node, expression.target.as_ref()) else {
            continue;
        };
        if !matches!(target, SemanticTypeId::POINTER | SemanticTypeId::NEVER | SemanticTypeId::STRING) {
            findings.push(GateObligation { kind: GateObligationKind::UnsupportedExpression, site: AstNodeKey { node, ..key } });
        }
    }
    for node in nodes_of_kind(index, key.node, NodeKind::ArrayLiteralExpression) {
        let Some(literal) = index.node_at(program, node).and_then(|node| node.of::<ArrayLiteralExpression>()) else {
            continue;
        };
        let types = literal
            .elements
            .iter()
            .map(|element| proven_operand(db, program, index, key, node, element).map(|(_, _, ty)| ty))
            .collect::<Option<Vec<_>>>();
        let Some(types) = types else { continue };
        if types.iter().any(|ty| matches!(*ty, SemanticTypeId::POINTER | SemanticTypeId::NEVER)) {
            continue;
        }
        if let Some((first, rest)) = types.split_first()
            && rest.iter().any(|ty| ty != first)
        {
            findings.push(GateObligation { kind: GateObligationKind::UnsupportedExpression, site: AstNodeKey { node, ..key } });
        }
    }
    for node in nodes_of_kind(index, key.node, NodeKind::LambdaParameter) {
        let Some(parameter) = index.node_at(program, node).and_then(|node| node.of::<LambdaParameter>()) else {
            continue;
        };
        if parameter.ty.is_some() {
            continue;
        }
        let Some(lambda) = parent_node(index, node) else { continue };
        // Only a lambda bound by `let` or returned directly is typed without an expected
        // signature by the legacy checker in every case; call arguments and subscriptions take
        // their parameter types from the callee or the event field.
        let mut owner = parent_node(index, lambda);
        while let Some(candidate) = owner
            && matches!(index.kind(candidate), Some(NodeKind::Expression | NodeKind::GroupedExpression))
        {
            owner = parent_node(index, candidate);
        }
        // An annotated local or a declared function-typed return supplies the signature through
        // `expected_type`; only a position proven to declare no function type is a finding.
        let lambda_key = AstNodeKey { node: lambda, ..key };
        let expected_function = matches!(expected_type(db, lambda_key), Ok(Some(expected)) if expected.function_signature().is_some());
        if !expected_function
            && owner.is_some_and(|owner| matches!(index.kind(owner), Some(NodeKind::LetStatement | NodeKind::ReturnStatement)))
        {
            findings.push(GateObligation {
                kind: GateObligationKind::MissingTypeAnnotation { name: Arc::from(parameter.name.node.name.as_str()) },
                site: AstNodeKey { node, ..key },
            });
        }
    }
    for node in nodes_of_kind(index, key.node, NodeKind::CallExpression) {
        let Some(call) = index.node_at(program, node).and_then(|node| node.of::<CallExpression>()) else { continue };
        let Some(to) = primitive_numeric_conversion_target(call) else { continue };
        let [argument] = call.args.as_slice() else { continue };
        let Some((site, _, from)) = proven_operand(db, program, index, key, node, argument) else { continue };
        if matches!(from, SemanticTypeId::POINTER | SemanticTypeId::NEVER) {
            continue;
        }
        let allowed = if to == SemanticTypeId::CHAR {
            from == SemanticTypeId::U32
        } else if from == SemanticTypeId::CHAR {
            to == SemanticTypeId::U32
        } else {
            is_numeric(from)
        };
        if !allowed {
            findings.push(GateObligation {
                kind: GateObligationKind::InvalidPrimitiveConversionArgument,
                site: AstNodeKey { node: site, ..key },
            });
        }
    }
}

pub(super) fn collect_member_obligations(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<GateObligation>,
) {
    for node in nodes_of_kind(index, key.node, NodeKind::MemberExpression) {
        let Some(reference) = index.node_at(program, node) else { continue };
        let Some(member) = reference.of::<MemberExpression>() else { continue };
        // A member that is a call's callee is a method call: a primitive receiver may carry
        // `extend` methods, so only `collect_call_obligations` judges it.
        if is_call_callee(program, index, node) {
            continue;
        }
        let site = AstNodeKey { node, ..key };
        // `pointer` is also the ABI of every nominal value: such a receiver is judged by its
        // declaration below, never as a primitive.
        if let Some((_, _, target)) = proven_operand(db, program, index, key, node, member.target.as_ref()) {
            if target == SemanticTypeId::NEVER {
                continue;
            }
            if target != SemanticTypeId::POINTER {
                findings.push(GateObligation { kind: GateObligationKind::InvalidMemberTarget, site });
                continue;
            }
        }
        let field_name = member.member.node.name.as_str();
        // `field_access_receiver` proves member receivers that are call results only; any other
        // receiver expression (a grouped local `(b).Get`, a field chain) takes its declaration
        // from the receiver's source identity.
        let declaration = match field_access_receiver(db, program, index, site, reference, None, None) {
            Some(Ok(receiver)) => {
                if receiver.layout.fields.iter().any(|(name, _)| name.as_ref() == receiver.field_name) {
                    continue;
                }
                receiver.declaration
            }
            _ => {
                let Some(receiver) = index.direct_child_id(program, node, DynNodeRef::from(member.target.as_ref()))
                else {
                    continue;
                };
                let receiver = AstNodeKey { node: receiver, ..key };
                let Ok(identity) = generic_source_expression_identity(db, receiver) else { continue };
                let Some(declaration) = crate::semantic_contract::contracts::concrete_declaration(db, receiver, &identity)
                else {
                    continue;
                };
                declaration
            }
        };
        let Some(declared) = declared_member_names(db, declaration) else { continue };
        if !declared.fields.contains(field_name)
            && (declared.methods.contains(field_name)
                || unique_nominal_method_declaration(db, declaration, field_name).is_some())
        {
            findings.push(GateObligation { kind: GateObligationKind::UnknownValueType, site });
        }
    }
    for node in nodes_of_kind(index, key.node, NodeKind::PathExpression) {
        let Some(path) = index.node_at(program, node).and_then(|node| node.of::<PathExpression>()) else { continue };
        let segments = path.path.node.segments.as_slice();
        if segments.len() < 2
            || segments.iter().any(|segment| !segment.node.type_args.is_empty())
            || is_call_callee(program, index, node)
        {
            continue;
        }
        let Some(local) = resolve_lexical_declaration(program, index, node, segments[0].node.name.node.name.as_str())
        else {
            continue;
        };
        let segment_site = |position: usize| {
            let site = index
                .direct_child_id(program, node, DynNodeRef::from(&path.path))
                .and_then(|path_node| index.direct_child_id(program, path_node, DynNodeRef::from(&segments[position])))
                .unwrap_or(node);
            AstNodeKey { node: site, ..key }
        };
        if let Some(Ok(ty)) = local_declaration_type(program, index, local)
            && !matches!(ty, SemanticTypeId::POINTER | SemanticTypeId::NEVER)
        {
            findings.push(GateObligation { kind: GateObligationKind::InvalidMemberTarget, site: segment_site(1) });
            continue;
        }
        // A two-segment read through a nominal local is `member_reference_legality`'s
        // (`unknown_field_read`, E1211 at the path); longer chains are walked here.
        if segments.len() < 3 {
            continue;
        }
        if let Some((kind, position)) = field_path_failure(db, program, index, AstNodeKey { node, ..key }, local, segments)
        {
            findings.push(GateObligation { kind, site: segment_site(position) });
        }
    }
    for node in nodes_of_kind(index, key.node, NodeKind::StructLiteralExpression) {
        let Some(literal) = index.node_at(program, node).and_then(|node| node.of::<StructLiteralExpression>()) else {
            continue;
        };
        let site = AstNodeKey { node, ..key };
        let Some(declaration) = resolve_type_declaration(db, site, &literal.path.node) else { continue };
        if declaration_kind(db, declaration) == Some(NodeKind::EnumDefinition) {
            findings.push(GateObligation { kind: GateObligationKind::UnknownStructType, site });
        }
    }
    for node in nodes_of_kind(index, key.node, NodeKind::EnumConstructorExpression) {
        let Some(constructor) = index.node_at(program, node).and_then(|node| node.of::<EnumConstructorExpression>())
        else {
            continue;
        };
        let site = AstNodeKey { node, ..key };
        let Some(declaration) = resolve_type_declaration(db, site, &constructor.path.node.type_path.node) else {
            continue;
        };
        if declaration_kind(db, declaration) == Some(NodeKind::TypeDefinition) {
            findings.push(GateObligation { kind: GateObligationKind::UnknownEnumType, site });
        }
    }
}

/// The first failing segment of a field path of three or more segments rooted at a nominal local
/// (`type_struct_field_path`): the walk starts at the local's declared type and follows declared
/// field types, stopping at the first finding as the legacy checker does.
///
/// * an intermediate field of proven primitive type followed by another segment: E1213
///   `InvalidMemberTarget` at that next segment;
/// * a `type` without the segment's field: E1211 `UnknownStructField` at the segment;
/// * an `enum` intermediate (a nominal without a fields table): E1201 `UnknownStructType` at the
///   segment.
///
/// The walk is fail-closed: a local without a declared nominal type, a type that does not
/// resolve, a contract, array, or function field type, a generic parameter of the declaring
/// type, and a field without `pub` declared in another unit (`member_reference_legality`'s
/// `InaccessibleStructField`) end it without a finding.
fn field_path_failure(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    site: AstNodeKey,
    local: beskid_analysis::syntax::AstNodeId,
    segments: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::PathSegment>],
) -> Option<(GateObligationKind, usize)> {
    let declared = declared_local_type(program, index, local)?;
    let Type::Complex(local_path) = &declared else { return None };
    let mut declaration = resolve_type_declaration(db, site, &local_path.node)?;
    for position in 1..segments.len() {
        let name = segments[position].node.name.node.name.as_str();
        let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
        let nominal = syntax.syntax_index(db).node_at(syntax.expanded_program(db), declaration.node)?;
        if nominal.of::<EnumDefinition>().is_some() {
            return Some((GateObligationKind::UnknownStructType, position));
        }
        let definition = nominal.of::<TypeDefinition>()?;
        let Some(field) = definition.fields.iter().find(|field| field.node.name.node.name == name) else {
            return Some((GateObligationKind::UnknownStructField { name: Arc::from(name) }, position));
        };
        if field.node.visibility.node != Visibility::Public && declaration.unit != site.unit {
            return None;
        }
        if position + 1 == segments.len() {
            return None;
        }
        match &field.node.ty.node {
            primitive @ Type::Primitive(_) => {
                let ty = semantic_type_from_syntax(primitive).ok()?;
                if matches!(ty, SemanticTypeId::POINTER | SemanticTypeId::NEVER) {
                    return None;
                }
                return Some((GateObligationKind::InvalidMemberTarget, position + 1));
            }
            Type::Complex(field_path) => {
                if let [single] = field_path.node.segments.as_slice()
                    && definition.generics.iter().any(|generic| generic.node.name == single.node.name.node.name)
                {
                    return None;
                }
                declaration = resolve_type_declaration(db, declaration, &field_path.node)?;
            }
            _ => return None,
        }
    }
    None
}

/// The type a local's declaration spells: a parameter's type or a `let` annotation.
fn declared_local_type(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    local: beskid_analysis::syntax::AstNodeId,
) -> Option<Type> {
    let declaring = parent_node(index, local)?;
    let node = index.node_at(program, declaring)?;
    if let Some(parameter) = node.of::<Parameter>() {
        return Some(parameter.ty.node.clone());
    }
    node.of::<LetStatement>()?.type_annotation.as_ref().map(|annotation| annotation.node.clone())
}

fn declaration_kind(db: &dyn Db, declaration: AstNodeKey) -> Option<NodeKind> {
    db.syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .and_then(|syntax| syntax.syntax_index(db).kind(declaration.node))
}

pub(super) fn collect_call_obligations(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<GateObligation>,
) {
    for node in nodes_of_kind(index, key.node, NodeKind::CallExpression) {
        let Some(call) = index.node_at(program, node).and_then(|node| node.of::<CallExpression>()) else { continue };
        let call_key = AstNodeKey { node, ..key };
        match &call.callee.node {
            Expression::Member(member) => {
                if let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(db, call_key) {
                    let explicit = member.node.type_args.len();
                    if explicit != 0
                        && let Some(expected) = declared_generic_count(db, declaration)
                        && expected != explicit
                    {
                        findings.push(GateObligation {
                            kind: GateObligationKind::GenericArgumentMismatch { expected, actual: explicit },
                            site: call_key,
                        });
                        continue;
                    }
                    match call_abi_signature(db, call_key) {
                        Err(error) if error.bound_violation().is_some() => {
                            let violation = error.bound_violation().expect("checked");
                            findings.push(GateObligation {
                                kind: GateObligationKind::GenericBoundNotSatisfied {
                                    type_name: Arc::clone(&violation.type_name),
                                    contract_name: Arc::clone(&violation.contract_name),
                                    reason: violation.reason.clone(),
                                },
                                site: call_key,
                            });
                        }
                        Err(error) if error.is_unavailable() => {
                            if let Some(parameter) = uninferred_type_parameter(db, key, call_key, call, declaration) {
                                findings.push(GateObligation {
                                    kind: GateObligationKind::UninferredTypeParameter { parameter },
                                    site: call_key,
                                });
                            }
                        }
                        _ => {}
                    }
                    continue;
                }
                if call_lowering(db, call_key).is_ok() {
                    continue;
                }
                let Some(callee) = index.direct_child_id(program, node, DynNodeRef::from(call.callee.as_ref())) else {
                    continue;
                };
                let callee = normalized_expression_node(index, callee);
                let Some(reference) = index.node_at(program, callee) else { continue };
                let Some(member) = reference.of::<MemberExpression>() else { continue };
                let member_key = AstNodeKey { node: callee, ..key };
                let Some(Ok(receiver)) = field_access_receiver(db, program, index, member_key, reference, None, None)
                else {
                    continue;
                };
                let name = member.member.node.name.as_str();
                let Some(declared) = declared_member_names(db, receiver.declaration) else { continue };
                if declared.fields.contains(name) {
                    if !matches!(declared_field_type(db, receiver.declaration, name), Some(Type::Function { .. })) {
                        findings.push(GateObligation { kind: GateObligationKind::UnknownCallTarget, site: call_key });
                    }
                    continue;
                }
                if declared.methods.contains(name)
                    || unique_nominal_method_declaration(db, receiver.declaration, name).is_some()
                {
                    continue;
                }
                let site = index.direct_child_id(program, callee, DynNodeRef::from(&member.member)).unwrap_or(callee);
                findings.push(GateObligation {
                    kind: GateObligationKind::UnknownStructField { name: Arc::from(name) },
                    site: AstNodeKey { node: site, ..key },
                });
            }
            Expression::Path(path) => {
                let path = &path.node.path.node;
                let lowering = call_lowering(db, call_key);
                if lowering.is_err()
                    && let Some(obligation) = generic_receiver_method_obligation(db, program, index, key, node, call)
                {
                    findings.push(obligation);
                    continue;
                }
                if lowering.is_err() && judge_path_method_call(db, program, index, key, node, call, findings) {
                    continue;
                }
                let explicit = path.segments.last().map_or(0, |segment| segment.node.type_args.len());
                if explicit != 0 {
                    let declaration = match lowering {
                        Ok(Some(CallLowering::Direct(declaration))) => Some(declaration),
                        _ => resolve_item_declaration_candidate(db, program, index, call_key, path),
                    };
                    if let Some(declaration) = declaration
                        && let Some(expected) = declared_generic_count(db, declaration)
                        && expected != explicit
                    {
                        findings.push(GateObligation {
                            kind: GateObligationKind::GenericArgumentMismatch { expected, actual: explicit },
                            site: call_key,
                        });
                        continue;
                    }
                }
                if let Ok(Some(CallLowering::Direct(declaration))) = lowering {
                    match call_abi_signature(db, call_key) {
                        Ok(Some(_)) => {}
                        Err(error) if error.bound_violation().is_some() => {
                            let violation = error.bound_violation().expect("checked");
                            findings.push(GateObligation {
                                kind: GateObligationKind::GenericBoundNotSatisfied {
                                    type_name: Arc::clone(&violation.type_name),
                                    contract_name: Arc::clone(&violation.contract_name),
                                    reason: violation.reason.clone(),
                                },
                                site: call_key,
                            });
                        }
                        Err(error)
                            if error.is_unavailable()
                                && let Some(parameter) =
                                    uninferred_type_parameter(db, key, call_key, call, declaration) =>
                        {
                            findings.push(GateObligation {
                                kind: GateObligationKind::UninferredTypeParameter { parameter },
                                site: call_key,
                            });
                        }
                        // No signature: inside a generic body the call has no specialization
                        // instance (`Ok(None)`) or its substitution is unavailable.
                        Ok(None) | Err(_) => {
                            judge_unconstrained_forwarding(db, program, index, key, node, declaration, findings);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// A method call through a local or implicit receiver path (`local.Name(..)`, `this.Name(..)`)
/// that `call_lowering` cannot resolve, judged as the legacy `type_call_expression` does once
/// no method or contract signature claims it: the callee path is typed as a field path
/// (`type_struct_field_path`) and the call then has no callable signature.
///
/// * a receiver of proven primitive type: E1213 at the member segment and E1606 at the call;
/// * a nominal receiver whose type declares the name as a non-function field: E1606 at the call;
/// * a nominal receiver whose type declares neither a field nor a method of that name: E1211 at
///   the member segment and E1606 at the call.
///
/// Returns `true` when the call is such a receiver path (judged or proven legal), so the caller
/// does not judge it again as a module-qualified function call.
fn judge_path_method_call(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    call_node: beskid_analysis::syntax::AstNodeId,
    call: &CallExpression,
    findings: &mut Vec<GateObligation>,
) -> bool {
    let Expression::Path(path_expression) = &call.callee.node else { return false };
    let [receiver, method] = path_expression.node.path.node.segments.as_slice() else { return false };
    if !receiver.node.type_args.is_empty() || !method.node.type_args.is_empty() {
        return false;
    }
    let receiver_name = receiver.node.name.node.name.as_str();
    let local = resolve_lexical_declaration(program, index, call_node, receiver_name);
    if local.is_none() && !matches!(receiver_name, "this" | "self") {
        return false;
    }
    let Some(callee) = index.direct_child_id(program, call_node, DynNodeRef::from(call.callee.as_ref())) else {
        return false;
    };
    let callee = normalized_expression_node(index, callee);
    let call_key = AstNodeKey { node: call_node, ..key };
    // Event raises, closure calls through a field, and the other call authorities ISLE consults
    // before `call_lowering` own these calls.
    if matches!(event_operation(db, call_key), Ok(Some(_)))
        || super::super::calls::claimed_by_another_call_authority(db, call_key, true)
    {
        return true;
    }
    // An `extend type` method of this unit may claim the name for any receiver type; the legacy
    // checker resolves those and `call_lowering` does not, so such a call is not judged here.
    let method_name = method.node.name.node.name.as_str();
    let extended = index.ids_of_kind(NodeKind::ExtendTypeDefinition).any(|extension| {
        index.node_at(program, extension).and_then(|node| node.of::<ExtendTypeDefinition>()).is_some_and(|extension| {
            extension.methods.iter().any(|candidate| candidate.node.name.node.name == method_name)
        })
    });
    if extended {
        return true;
    }
    let segment_site = index
        .direct_child_id(program, callee, DynNodeRef::from(&path_expression.node.path))
        .and_then(|path_node| index.direct_child_id(program, path_node, DynNodeRef::from(method)))
        .map_or(AstNodeKey { node: callee, ..key }, |node| AstNodeKey { node, ..key });
    if let Some(local) = local
        && let Some(Ok(ty)) = local_declaration_type(program, index, local)
        && !matches!(ty, SemanticTypeId::POINTER | SemanticTypeId::NEVER)
    {
        findings.push(GateObligation { kind: GateObligationKind::InvalidMemberTarget, site: segment_site });
        findings.push(GateObligation { kind: GateObligationKind::UnknownCallTarget, site: call_key });
        return true;
    }
    // An array or function value has no members: `rest.Length()` on a `T[]` is a check error
    // here, never a call left for lowering to reject (E2102).
    if let Some(local) = local
        && local_declares_array_or_function_value(program, index, local)
    {
        findings.push(GateObligation { kind: GateObligationKind::InvalidMemberTarget, site: segment_site });
        findings.push(GateObligation { kind: GateObligationKind::UnknownCallTarget, site: call_key });
        return true;
    }
    let Some(reference) = index.node_at(program, callee) else { return true };
    let callee_key = AstNodeKey { node: callee, ..key };
    let Some(Ok(receiver)) = field_access_receiver(db, program, index, callee_key, reference, None, None) else {
        return true;
    };
    let Some(declared) = declared_member_names(db, receiver.declaration) else { return true };
    if declared.fields.contains(method_name) {
        if !matches!(declared_field_type(db, receiver.declaration, method_name), Some(Type::Function { .. })) {
            findings.push(GateObligation { kind: GateObligationKind::UnknownCallTarget, site: call_key });
        }
        return true;
    }
    if declared.methods.contains(method_name)
        || unique_nominal_method_declaration(db, receiver.declaration, method_name).is_some()
    {
        return true;
    }
    findings.push(GateObligation {
        kind: GateObligationKind::UnknownStructField { name: Arc::from(method_name) },
        site: segment_site,
    });
    findings.push(GateObligation { kind: GateObligationKind::UnknownCallTarget, site: call_key });
    true
}

/// Whether the local declared at `declaration` is written or initialised as an array (`T[]`,
/// `[a, b]`) or a function value (`R(A)`): a non-nominal receiver kind that has no methods.
fn local_declares_array_or_function_value(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> bool {
    let non_nominal = |ty: &Type| matches!(ty, Type::Array(_) | Type::Function { .. });
    let Some(parent) = parent_node(index, declaration) else { return false };
    let Some(node) = index.node_at(program, parent) else { return false };
    if let Some(parameter) = node.of::<Parameter>() {
        return non_nominal(&parameter.ty.node);
    }
    if let Some(parameter) = node.of::<LambdaParameter>() {
        return parameter.ty.as_ref().is_some_and(|ty| non_nominal(&ty.node));
    }
    if let Some(statement) = node.of::<LetStatement>() {
        return match &statement.type_annotation {
            Some(ty) => non_nominal(&ty.node),
            None => matches!(statement.value.node, Expression::ArrayLiteral(_)),
        };
    }
    false
}

/// E1250: `value.Method(..)` that `call_lowering` cannot resolve, where `value` is a parameter or
/// annotated `let` typed by a type parameter of the enclosing callable. Its members are exactly
/// the methods of its `where` bound contracts; a method no bound declares is a check error, never
/// a call left for lowering to reject.
fn generic_receiver_method_obligation(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    call_node: beskid_analysis::syntax::AstNodeId,
    call: &CallExpression,
) -> Option<GateObligation> {
    let Expression::Path(path_expression) = &call.callee.node else { return None };
    let path = &path_expression.node.path.node;
    let [receiver, method] = path.segments.as_slice() else { return None };
    if !receiver.node.type_args.is_empty() {
        return None;
    }
    let local = resolve_lexical_declaration(program, index, call_node, receiver.node.name.node.name.as_str())?;
    let declaration = parent_node(index, local)?;
    let declared = match index.kind(declaration)? {
        NodeKind::Parameter => &index.node_at(program, declaration)?.of::<Parameter>()?.ty.node,
        NodeKind::LetStatement => &index.node_at(program, declaration)?.of::<LetStatement>()?.type_annotation.as_ref()?.node,
        _ => return None,
    };
    let parameter = crate::semantic_contract::generic_parameter_reference_name(declared)?;
    if !crate::semantic_contract::type_syntax_is_enclosing_generic_parameter_reference(
        db,
        AstNodeKey { node: declaration, ..key },
        declared,
    ) {
        return None;
    }
    let call_key = AstNodeKey { node: call_node, ..key };
    if matches!(crate::semantic_contract::contract_member_receiver(db, program, index, call_key, path), Some(Ok(_))) {
        return None;
    }
    let callee = index.direct_child_id(program, call_node, DynNodeRef::from(call.callee.as_ref()))?;
    let callee = normalized_expression_node(index, callee);
    let site = index
        .direct_child_id(program, callee, DynNodeRef::from(&path_expression.node.path))
        .and_then(|path_node| index.direct_child_id(program, path_node, DynNodeRef::from(method)))
        .unwrap_or(callee);
    Some(GateObligation {
        kind: GateObligationKind::GenericReceiverMethodUndeclared {
            method: Arc::from(method.node.name.node.name.as_str()),
            parameter: Arc::from(parameter),
        },
        site: AstNodeKey { node: site, ..key },
    })
}

/// The declared generic parameter count of a resolved function declaration; methods declare no
/// generics of their own and are not judged.
fn declared_generic_count(db: &dyn Db, declaration: AstNodeKey) -> Option<usize> {
    let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
    syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)?
        .of::<FunctionDefinition>()
        .map(|function| function.generics.len())
        .or_else(|| {
            // Explicit arguments on a method call bind the method's own type parameters.
            syntax
                .syntax_index(db)
                .node_at(syntax.expanded_program(db), declaration.node)?
                .of::<beskid_analysis::syntax::MethodDefinition>()
                .map(|method| method.generics.len())
        })
}

fn declared_field_type(db: &dyn Db, declaration: AstNodeKey, name: &str) -> Option<Type> {
    let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
    syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)?
        .of::<TypeDefinition>()?
        .fields
        .iter()
        .find(|field| field.node.name.node.name == name)
        .map(|field| field.node.ty.node.clone())
}

/// A second `let` of one name directly in one block (the legacy block scope), and a repeated
/// parameter name of the item, reported at the later declaration's name.
pub(super) fn collect_duplicate_local_obligations(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    findings: &mut Vec<GateObligation>,
) {
    let mut parameters: Vec<(String, beskid_analysis::syntax::AstNodeId)> = Vec::new();
    if let Some(children) = index.children(key.node) {
        for child in children {
            let Some(parameter) = index.node_at(program, *child).and_then(|node| node.of::<Parameter>()) else {
                continue;
            };
            let Some(name_node) = index.direct_child_id(program, *child, DynNodeRef::from(&parameter.name)) else {
                continue;
            };
            parameters.push((parameter.name.node.name.clone(), name_node));
        }
    }
    report_duplicates(index, key, parameters, findings);
    for block in nodes_of_kind(index, key.node, NodeKind::Block) {
        let Some(children) = index.children(block) else { continue };
        let mut locals = Vec::new();
        for child in children {
            let statement = if index.kind(*child) == Some(NodeKind::Statement) {
                match index.children(*child).and_then(|inner| inner.first()) {
                    Some(inner) => *inner,
                    None => continue,
                }
            } else {
                *child
            };
            let Some(binding) = index.node_at(program, statement).and_then(|node| node.of::<LetStatement>()) else {
                continue;
            };
            let Some(name_node) = index.direct_child_id(program, statement, DynNodeRef::from(&binding.name)) else {
                continue;
            };
            locals.push((binding.name.node.name.clone(), name_node));
        }
        report_duplicates(index, key, locals, findings);
    }
}

fn report_duplicates(
    index: &SyntaxIndex,
    key: AstNodeKey,
    declarations: Vec<(String, beskid_analysis::syntax::AstNodeId)>,
    findings: &mut Vec<GateObligation>,
) {
    let mut seen: HashMap<String, beskid_analysis::syntax::AstNodeId> = HashMap::new();
    for (name, node) in declarations {
        match seen.get(&name) {
            Some(previous) => {
                let previous = index.metadata_for(key.generation, *previous).and_then(|metadata| metadata.span).unwrap_or_default();
                findings.push(GateObligation {
                    kind: GateObligationKind::DuplicateLocal { name: Arc::from(name.as_str()), previous },
                    site: AstNodeKey { node, ..key },
                });
            }
            None => {
                seen.insert(name, node);
            }
        }
    }
}

/// A call of a bounded callee from inside a generic callable that forwards one of its own
/// generic parameters (`i64 Forward<U>(U value) { return Consume(value); }` with
/// `Consume<T>(T value) where T: Reader`): the argument has no concrete identity, so
/// `call_abi_signature` cannot prove the bound. The legacy `check_generic_function_bounds`
/// accepts the forwarding only when the caller declares the same bound on that parameter
/// (`where U: Reader`); otherwise it reports E1610 naming the caller's parameter. Mirrored here:
/// the argument is a path to a parameter or local declared with a bare generic parameter of the
/// enclosing callable, and that callable's `where` bounds name no contract equal to the callee's.
fn judge_unconstrained_forwarding(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    call_node: beskid_analysis::syntax::AstNodeId,
    declaration: AstNodeKey,
    findings: &mut Vec<GateObligation>,
) {
    let call_key = AstNodeKey { node: call_node, ..key };
    let Ok(bounds) = crate::semantic_contract::contracts::declared_where_bounds(db, declaration) else { return };
    let Ok(Some(arguments)) = call_arguments(db, call_key) else { return };
    let Some(enclosing) = super::super::typing::enclosing_callable(index, call_node) else { return };
    let Some(enclosing_node) = index.node_at(program, enclosing) else { return };
    let Some(function) = enclosing_node.of::<FunctionDefinition>() else { return };
    let enclosing_key = AstNodeKey { node: enclosing, ..key };
    for bound in bounds {
        for (_, position) in &bound.parameters {
            let Some(argument) = arguments.get(*position as usize) else { continue };
            let argument_node = normalized_expression_node(index, argument.node);
            let Some(path) = index.node_at(program, argument_node).and_then(|node| node.of::<PathExpression>()) else {
                continue;
            };
            let [segment] = path.path.node.segments.as_slice() else { continue };
            let Some(local) = resolve_lexical_declaration(program, index, argument_node, segment.node.name.node.name.as_str())
            else {
                continue;
            };
            let Some(owner) = parent_node(index, local).and_then(|owner| index.node_at(program, owner)) else { continue };
            let declared = if let Some(parameter) = owner.of::<Parameter>() {
                parameter.ty.node.clone()
            } else if let Some(statement) = owner.of::<LetStatement>() {
                match statement.type_annotation.as_ref() {
                    Some(annotation) => annotation.node.clone(),
                    None => continue,
                }
            } else {
                continue;
            };
            let Type::Complex(declared_path) = &declared else { continue };
            let [declared_segment] = declared_path.node.segments.as_slice() else { continue };
            if !declared_segment.node.type_args.is_empty() {
                continue;
            }
            let generic = declared_segment.node.name.node.name.as_str();
            if !function.generics.iter().any(|candidate| candidate.node.name == generic) {
                continue;
            }
            if let Some(marker) = transfer_marker_contract(db, bound.contract) {
                // A marker bound is covered when the caller's own marker bounds give the generic
                // a class that satisfies it (`T: Moved` covers `Transferable`).
                let markers = function
                    .where_bounds
                    .iter()
                    .filter(|caller_bound| caller_bound.parameter.node.name == generic)
                    .filter_map(|caller_bound| {
                        let mut contract_path = caller_bound.contract.node.clone();
                        if let Some(last) = contract_path.segments.last_mut() {
                            last.node.type_args.clear();
                        }
                        resolve_contract(db, enclosing_key, &contract_path)
                    })
                    .filter_map(|contract| transfer_marker_contract(db, contract))
                    .collect::<Vec<_>>();
                let class = crate::semantic_contract::transfer::bound_class(generic, &markers);
                if !class.satisfies(marker) {
                    findings.push(GateObligation {
                        kind: GateObligationKind::GenericBoundNotSatisfied {
                            type_name: Arc::from(generic),
                            contract_name: Arc::from(marker.name()),
                            reason: Some(Arc::from(crate::semantic_contract::transfer::marker_bound_reason(&class, marker))),
                        },
                        site: call_key,
                    });
                    return;
                }
                continue;
            }
            let covered = function.where_bounds.iter().any(|caller_bound| {
                if caller_bound.parameter.node.name != generic {
                    return false;
                }
                let mut contract_path = caller_bound.contract.node.clone();
                if let Some(last) = contract_path.segments.last_mut() {
                    last.node.type_args.clear();
                }
                resolve_contract(db, enclosing_key, &contract_path) == Some(bound.contract)
            });
            if !covered {
                let contract_name = bound
                    .path
                    .segments
                    .last()
                    .map(|segment| segment.node.name.node.name.clone())
                    .unwrap_or_default();
                findings.push(GateObligation {
                    kind: GateObligationKind::GenericBoundNotSatisfied {
                        type_name: Arc::from(generic),
                        contract_name: Arc::from(contract_name.as_str()),
                        reason: None,
                    },
                    site: call_key,
                });
                return;
            }
        }
    }
}

/// The first own type parameter of a generic call's callee that inference cannot bind: a call
/// with no explicit type arguments in a non-generic item whose own type parameter is named only
/// by the result of function-typed parameters that receive lambda literals (or by no parameter
/// at all), and whose specialization is unavailable. Reported as E1203 naming the parameter.
fn uninferred_type_parameter(
    db: &dyn Db,
    item: AstNodeKey,
    call_key: AstNodeKey,
    call: &CallExpression,
    declaration: AstNodeKey,
) -> Option<Arc<str>> {
    if call_explicit_member_type_arguments(db, call_key).is_some() {
        return None;
    }
    if let Expression::Path(path) = &call.callee.node
        && explicit_generic_type_argument_syntax(&path.node.path.node).is_some()
    {
        return None;
    }
    if generic_callable_parameters(db, item).is_some() {
        return None;
    }
    let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
    let node = syntax.syntax_index(db).node_at(syntax.expanded_program(db), declaration.node)?;
    let (own, parameters) = if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
        (method.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>(), &method.parameters)
    } else {
        let function = node.of::<FunctionDefinition>()?;
        (function.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>(), &function.parameters)
    };
    if parameters.len() != call.args.len() {
        return None;
    }
    own.into_iter()
        .find(|name| {
            !parameters.iter().zip(call.args.iter()).any(|(parameter, argument)| {
                let lambda = expression_is_lambda(&argument.node);
                match &parameter.node.ty.node {
                    Type::Function { parameters, .. } if lambda => parameters
                        .iter()
                        .any(|parameter| type_syntax_mentions_generic_parameter(&parameter.node, name)),
                    other => !lambda && type_syntax_mentions_generic_parameter(other, name),
                }
            })
        })
        .map(Arc::from)
}
