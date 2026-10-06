//! Expression-shape, member, call, and local-declaration obligations (E1102, E1201, E1202,
//! E1204, E1211, E1213, E1228, E1606, E1610).
//!
//! Legacy sites mirrored here: `type_index_expression` and `type_array_literal_expression`
//! (E1202 at the expression), `type_lambda_expression_with_expected` without an expected
//! signature (E1202 at the parameter), the primitive conversion argument check (E1228 at the
//! argument), `type_member_expression` and `type_struct_field_path` (E1213 at the member
//! expression or path segment, E1201 for a method named as a value, E1211 at the member name of
//! an unknown method call), `type_struct_literal_expression` and
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
    ArrayLiteralExpression, CallExpression, EnumConstructorExpression, Expression, ExtendTypeDefinition,
    FunctionDefinition, IndexExpression, LambdaParameter, LetStatement, MemberExpression, Parameter, PathExpression,
    StructLiteralExpression, Type, TypeDefinition,
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
        if owner.is_some_and(|owner| matches!(index.kind(owner), Some(NodeKind::LetStatement | NodeKind::ReturnStatement)))
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
        let [base, segment] = path.path.node.segments.as_slice() else { continue };
        if !base.node.type_args.is_empty() || is_call_callee(program, index, node) {
            continue;
        }
        let Some(local) = resolve_lexical_declaration(program, index, node, base.node.name.node.name.as_str()) else {
            continue;
        };
        let Some(Ok(ty)) = local_declaration_type(program, index, local) else { continue };
        if matches!(ty, SemanticTypeId::POINTER | SemanticTypeId::NEVER) {
            continue;
        }
        let site = index
            .direct_child_id(program, node, DynNodeRef::from(&path.path))
            .and_then(|path_node| index.direct_child_id(program, path_node, DynNodeRef::from(segment)))
            .unwrap_or(node);
        findings.push(GateObligation { kind: GateObligationKind::InvalidMemberTarget, site: AstNodeKey { node: site, ..key } });
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
            Expression::Member(_) => {
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
                if matches!(lowering, Ok(Some(CallLowering::Direct(_))))
                    && let Err(error) = call_abi_signature(db, call_key)
                    && let Some(violation) = error.bound_violation()
                {
                    findings.push(GateObligation {
                        kind: GateObligationKind::GenericBoundNotSatisfied {
                            type_name: Arc::clone(&violation.type_name),
                            contract_name: Arc::clone(&violation.contract_name),
                        },
                        site: call_key,
                    });
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

/// The declared generic parameter count of a resolved function declaration; methods declare no
/// generics of their own and are not judged.
fn declared_generic_count(db: &dyn Db, declaration: AstNodeKey) -> Option<usize> {
    let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
    syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)?
        .of::<FunctionDefinition>()
        .map(|function| function.generics.len())
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
