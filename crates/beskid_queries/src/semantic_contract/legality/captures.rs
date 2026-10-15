//! Lambda capture legality.
//!
//! E1233: a lambda captures each outer local by value into its closure environment. A `mut` local
//! or a native pointer is a stack reference: snapshotting it would silently stop aliasing the
//! source storage, so the closure static plan rejects it. This fact reports that as a user error
//! instead of an unavailable closure fact. Spawn operands are judged by `spawn_legality` (E1225)
//! and lambdas judged by a `[Captures]` bound (below) are skipped here.
//!
//! `[Captures(Bound: C)]` (0.6.2, `language-meta--evaluation--lambdas-and-closures`, "Capture
//! bounds on function-typed parameters"): the declaration check (E1248) and the judgement of the
//! argument for a bounded parameter or the initializer of a bounded field: a lambda literal's
//! captures (E1235, E1236, E1237, E1241), a function item, a value read from a bounded parameter
//! or field (or an immutable binding initialized only from one), or else E1238.

use super::super::transfer::{TransferClass, TransferMarker, indexed_site, transfer_class, transfer_class_of_type};
use super::*;
use beskid_analysis::syntax::AstNodeId;
use beskid_analysis::syntax_query::DynNodeRef;

/// Every non-spawn, unbounded lambda in the item `key` that captures a stack reference, reported
/// once per lambda for its first such capture, in node order.
pub(super) fn lambda_capture_findings(db: &dyn Db, key: AstNodeKey) -> Vec<SemanticFinding> {
    let Some(syntax) = db.syntax_unit(key.unit) else { return Vec::new() };
    let lambdas = match with_node(db, syntax, key, |_program, index, _node| {
        let mut lambdas = Vec::new();
        collect_nodes_of_kind(index, key.node, NodeKind::LambdaExpression, &mut lambdas);
        Some(lambdas.into_iter().filter(|lambda| !is_spawn_operand(index, *lambda)).collect::<Vec<_>>())
    }) {
        Ok(Some(lambdas)) => lambdas,
        Ok(None) | Err(_) => return Vec::new(),
    };
    lambdas
        .into_iter()
        .map(|node| AstNodeKey { node, ..key })
        .filter(|lambda| bound_of_position(db, *lambda).is_none())
        .filter_map(|lambda| {
            let environment = closure_environment(db, lambda).ok().flatten()?;
            let capture =
                environment.captures.iter().find(|capture| capture.class == CaptureStorageClass::StackReference)?;
            let name = with_node(db, syntax, capture.declaration, |_program, _index, node| {
                node.of::<beskid_analysis::syntax::Identifier>().map(|identifier| identifier.name.clone())
            })
            .ok()
            .flatten()?;
            Some(SemanticFinding {
                kind: SemanticIssueKind::LambdaCapturesStackReference { name },
                site: lambda,
                related: Vec::new(),
            })
        })
        .collect()
}

fn is_spawn_operand(index: &SyntaxIndex, lambda: beskid_analysis::syntax::AstNodeId) -> bool {
    let mut current = lambda;
    while let Some(parent) = parent_node(index, current) {
        match index.kind(parent) {
            Some(NodeKind::Expression | NodeKind::GroupedExpression) => current = parent,
            Some(NodeKind::SpawnExpression) => return true,
            _ => return false,
        }
    }
    false
}

/// The expression position of `value` (wrappers skipped): an explicit call argument or the value
/// of a struct literal field, with the capture bound its declaration carries.
fn bound_of_position(db: &dyn Db, value: AstNodeKey) -> Option<CapturesBound> {
    let syntax = db.syntax_unit(value.unit)?;
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let mut child = value.node;
    loop {
        let parent = parent_node(index, child)?;
        match index.kind(parent)? {
            NodeKind::Expression | NodeKind::GroupedExpression => child = parent,
            NodeKind::CallExpression => {
                let call = index.node_at(program, parent)?.of::<beskid_analysis::syntax::CallExpression>()?;
                let position = call
                    .args
                    .iter()
                    .position(|argument| index.direct_child_id(program, parent, DynNodeRef::from(argument)) == Some(child))?;
                // The task body of `__isolate_spawn[_local]<T>(() => T body, i64 scope)` runs in
                // another runtime instance: it is judged like `[Captures(Bound: Transferable)]`.
                if position == 0 && isolate_spawn_call(call) {
                    return Some(CapturesBound::Marker(TransferMarker::Transferable));
                }
                return call_argument_captures_bound(db, AstNodeKey { node: parent, ..value }, position);
            }
            NodeKind::StructLiteralField => {
                let field = index.node_at(program, parent)?.of::<beskid_analysis::syntax::StructLiteralField>()?;
                let literal = parent_node(index, parent)?;
                let path = &index.node_at(program, literal)?.of::<beskid_analysis::syntax::StructLiteralExpression>()?.path;
                let owner = resolve_type_declaration(db, AstNodeKey { node: literal, ..value }, &path.node)?;
                return field_captures_bound(db, owner, &field.name.node.name);
            }
            _ => return None,
        }
    }
}

/// E1248 for every `[Captures]` attribute of the unit: the bound is not a marker contract, or the
/// parameter (field) is not of function type.
pub(super) fn captures_declaration_findings(db: &dyn Db, unit_root: AstNodeKey) -> Vec<SemanticFinding> {
    let Some(syntax) = db.syntax_unit(unit_root.unit).filter(|syntax| syntax.accepts_key(db, unit_root)) else {
        return Vec::new();
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let mut findings = Vec::new();
    let mut judge = |node: AstNodeId,
                     attributes: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Attribute>],
                     ty: &beskid_analysis::syntax::Type| {
        let key = AstNodeKey { node, ..unit_root };
        let Some((bound, attribute)) = captures_bound(db, key, attributes) else { return };
        let bound = match bound {
            CapturesBound::NotMarker(spelling) => Some(spelling.unwrap_or_else(|| "(missing)".to_owned())),
            CapturesBound::Marker(_) if !matches!(ty, beskid_analysis::syntax::Type::Function { .. }) => None,
            CapturesBound::Marker(_) => return,
        };
        findings.push(SemanticFinding {
            kind: SemanticIssueKind::CapturesBoundNotMarker { bound },
            site: indexed_site(db, key, DynNodeRef::from(attribute)),
            related: Vec::new(),
        });
    };
    for node in index.ids_of_kind(NodeKind::Parameter) {
        if let Some(parameter) = index.node_at(program, node).and_then(|node| node.of::<beskid_analysis::syntax::Parameter>()) {
            judge(node, &parameter.attributes, &parameter.ty.node);
        }
    }
    for node in index.ids_of_kind(NodeKind::Field) {
        if let Some(field) = index.node_at(program, node).and_then(|node| node.of::<beskid_analysis::syntax::Field>()) {
            judge(node, &field.attributes, &field.ty.node);
        }
    }
    findings
}

/// The judgement of every bounded argument and field initializer in the item `key`.
pub(super) fn captures_argument_findings(db: &dyn Db, key: AstNodeKey) -> Vec<SemanticFinding> {
    let Some(syntax) = db.syntax_unit(key.unit) else { return Vec::new() };
    let positions = match with_node(db, syntax, key, |program, index, _node| {
        let mut calls = Vec::new();
        collect_nodes_of_kind(index, key.node, NodeKind::CallExpression, &mut calls);
        let mut values = Vec::new();
        for call in calls {
            let Some(expression) =
                index.node_at(program, call).and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
            else {
                continue;
            };
            values.extend(
                expression.args.iter().filter_map(|argument| index.direct_child_id(program, call, DynNodeRef::from(argument))),
            );
        }
        let mut fields = Vec::new();
        collect_nodes_of_kind(index, key.node, NodeKind::StructLiteralField, &mut fields);
        for field in fields {
            if let Some(value) = index
                .node_at(program, field)
                .and_then(|node| node.of::<beskid_analysis::syntax::StructLiteralField>())
                .and_then(|literal_field| index.direct_child_id(program, field, DynNodeRef::from(&literal_field.value)))
            {
                values.push(value);
            }
        }
        Some(values)
    }) {
        Ok(Some(values)) => values,
        Ok(None) | Err(_) => return Vec::new(),
    };
    let mut findings = Vec::new();
    for value in positions {
        let value = AstNodeKey { node: value, ..key };
        let Some(CapturesBound::Marker(bound)) = bound_of_position(db, value) else { continue };
        let _ = with_node(db, syntax, value, |program, index, _node| {
            judge_bounded_value(db, program, index, value, bound, &mut findings);
            Some(())
        });
    }
    findings
}

fn admits(bound: TransferMarker, class: &TransferClass) -> bool {
    class.satisfies(bound)
}

fn judge_bounded_value(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    value: AstNodeKey,
    bound: TransferMarker,
    findings: &mut Vec<SemanticFinding>,
) {
    let node = normalized_expression_node(index, value.node);
    let normalized = AstNodeKey { node, ..value };
    match index.kind(node) {
        Some(NodeKind::LambdaExpression) => judge_lambda(db, program, index, normalized, bound, findings),
        Some(NodeKind::PathExpression) => {
            let accepted = matches!(function_item_value(db, normalized), Ok(Some(value)) if value.specialization.is_none())
                || bounded_source(db, program, index, normalized, bound);
            if !accepted {
                findings.push(SemanticFinding {
                    kind: SemanticIssueKind::CapturesArgumentNotLiteral,
                    site: normalized,
                    related: Vec::new(),
                });
            }
        }
        _ => findings.push(SemanticFinding {
            kind: SemanticIssueKind::CapturesArgumentNotLiteral,
            site: normalized,
            related: Vec::new(),
        }),
    }
}

/// A value read from a `[Captures]` parameter or field whose bound satisfies `bound`, or an
/// immutable binding initialized only from one (`par` R-lang-3).
fn bounded_source(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    path_key: AstNodeKey,
    bound: TransferMarker,
) -> bool {
    let compatible = |declared: TransferMarker| declared == bound || bound == TransferMarker::Transferable;
    let Some(path) = index.node_at(program, path_key.node).and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
    else {
        return false;
    };
    let segments = &path.path.node.segments;
    // `this.field` (or `owner.field`): the declared field's bound.
    if segments.len() == 2 {
        let receiver = segments[0].node.name.node.name.as_str();
        let field = segments[1].node.name.node.name.as_str();
        let owner_type = if receiver == "this" {
            method_this_type(db, path_key)
        } else {
            resolve_lexical_declaration(program, index, path_key.node, receiver).and_then(|declaration| {
                let parent = parent_node(index, declaration)?;
                index.node_at(program, parent)?.of::<beskid_analysis::syntax::Parameter>().map(|parameter| parameter.ty.node.clone())
            })
        };
        if let Some(beskid_analysis::syntax::Type::Complex(owner_path)) = owner_type
            && let Some(owner) = resolve_type_declaration(db, path_key, &owner_path.node)
            && let Some(CapturesBound::Marker(declared)) = field_captures_bound(db, owner, field)
        {
            return compatible(declared);
        }
        return false;
    }
    let [segment] = segments.as_slice() else { return false };
    let Some(declaration) = resolve_lexical_declaration(program, index, path_key.node, &segment.node.name.node.name) else {
        return false;
    };
    let Some(parent) = parent_node(index, declaration) else { return false };
    let Some(parent_syntax) = index.node_at(program, parent) else { return false };
    if let Some(parameter) = parent_syntax.of::<beskid_analysis::syntax::Parameter>() {
        return matches!(
            captures_bound(db, AstNodeKey { node: parent, ..path_key }, &parameter.attributes),
            Some((CapturesBound::Marker(declared), _)) if compatible(declared)
        );
    }
    if let Some(binding) = parent_syntax.of::<beskid_analysis::syntax::LetStatement>()
        && !binding.mutable
        && let Some(initializer) = index.direct_child_id(program, parent, DynNodeRef::from(&binding.value))
    {
        let initializer = normalized_expression_node(index, initializer);
        return index.kind(initializer) == Some(NodeKind::PathExpression)
            && initializer != path_key.node
            && bounded_source(db, program, index, AstNodeKey { node: initializer, ..path_key }, bound);
    }
    false
}

/// The first use of `declaration` inside `lambda`, and whether any use is written `move`.
fn capture_uses(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    lambda: AstNodeId,
    declaration: AstNodeId,
    name: &str,
) -> (Option<AstNodeId>, bool, Option<AstNodeId>) {
    let mut first = None;
    let mut moved = false;
    let mut assigned = None;
    let mut uses = index
        .ids_of_kind(NodeKind::PathExpression)
        .filter(|node| is_ancestor(index, lambda, *node))
        .filter_map(|node| {
            let path = index.node_at(program, node)?.of::<beskid_analysis::syntax::PathExpression>()?;
            let head = path.path.node.segments.first()?.node.name.node.name.as_str();
            (head == name && resolve_lexical_declaration(program, index, node, head) == Some(declaration))
                .then(|| (index.node_at(program, node).and_then(|node| node.span()).map_or(0, |span| span.start), node))
        })
        .collect::<Vec<_>>();
    uses.sort();
    for (_, node) in uses {
        first.get_or_insert(node);
        let mut child = node;
        while let Some(parent) = parent_node(index, child) {
            match index.kind(parent) {
                Some(NodeKind::Expression | NodeKind::GroupedExpression) => child = parent,
                Some(NodeKind::MoveExpression) => {
                    moved = true;
                    break;
                }
                Some(NodeKind::AssignExpression) => {
                    if assigned.is_none()
                        && let Some(assignment) =
                            index.node_at(program, parent).and_then(|node| node.of::<beskid_analysis::syntax::AssignExpression>())
                        && index.direct_child_id(program, parent, DynNodeRef::from(assignment.target.as_ref())) == Some(child)
                    {
                        assigned = Some(parent);
                    }
                    break;
                }
                _ => break,
            }
        }
    }
    (first, moved, assigned)
}

fn judge_lambda(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    lambda: AstNodeKey,
    bound: TransferMarker,
    findings: &mut Vec<SemanticFinding>,
) {
    for (declaration, name) in lambda_captures(program, index, lambda) {
        let (first, moved, assigned) = capture_uses(program, index, lambda.node, declaration.node, &name);
        let site = AstNodeKey { node: first.unwrap_or(lambda.node), ..lambda };
        let parent = parent_node(index, declaration.node);
        let parent_syntax = parent.and_then(|parent| index.node_at(program, parent));
        let mutable = parent_syntax.as_ref().is_some_and(|node| {
            node.of::<beskid_analysis::syntax::LetStatement>().is_some_and(|binding| binding.mutable)
                || node.of::<beskid_analysis::syntax::Parameter>().is_some_and(|parameter| parameter.mutable)
        });
        if mutable {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::CapturedLocalAssignedInBoundLambda { name },
                site: assigned.map_or(site, |node| AstNodeKey { node, ..lambda }),
                related: Vec::new(),
            });
            continue;
        }
        // A function value read from a bounded parameter or binding satisfies the bound itself.
        if let Some(first) = first
            && bounded_source(db, program, index, AstNodeKey { node: first, ..lambda }, bound)
        {
            continue;
        }
        let parameter_type = parent_syntax
            .as_ref()
            .and_then(|node| node.of::<beskid_analysis::syntax::Parameter>().map(|parameter| parameter.ty.node.clone()));
        let identity = generic_source_local_identity(db, program, index, declaration, declaration.node).ok();
        let class = match parameter_type.as_ref() {
            Some(ty) => transfer_class_of_type(db, AstNodeKey { node: parent.unwrap_or(declaration.node), ..declaration }, ty).ok(),
            None => identity.as_ref().and_then(|identity| transfer_class(db, declaration, identity).ok()),
        };
        let Some(class) = class else { continue };
        if !admits(bound, &class) {
            let class_text = match &class {
                TransferClass::Never(reason) => format!("Never ({reason})"),
                other => format!("{} (bound `{}`)", other.name(), bound.name()),
            };
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::CaptureNotTransferable {
                    name,
                    type_name: identity.as_ref().map(identity_spelling).unwrap_or_default(),
                    class: class_text,
                },
                site,
                related: Vec::new(),
            });
            continue;
        }
        if class != TransferClass::Moved || moved {
            continue;
        }
        let owning = containing_callable(index, declaration.node)
            .and_then(|callable| affine_ownership(db, AstNodeKey { node: callable, ..declaration }).ok().flatten())
            .and_then(|ownership| ownership.bindings.iter().find(|binding| binding.declaration == declaration).map(|b| b.owning))
            .unwrap_or(true);
        let kind = if owning {
            SemanticIssueKind::MovedCaptureWithoutMove { name }
        } else {
            SemanticIssueKind::BorrowEscapes { name }
        };
        let related = if owning { Vec::new() } else { vec![(declaration, "borrowed here")] };
        findings.push(SemanticFinding { kind, site, related });
    }
}

/// The outer bindings a lambda reads, by the first segment of each path inside it, in source
/// order (`x`, `x.f` and `x.M()` all capture `x`).
fn lambda_captures(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    lambda: AstNodeKey,
) -> Vec<(AstNodeKey, String)> {
    let mut uses = index
        .ids_of_kind(NodeKind::PathExpression)
        .filter(|node| is_ancestor(index, lambda.node, *node))
        .filter_map(|node| {
            let syntax = index.node_at(program, node)?;
            let path = syntax.of::<beskid_analysis::syntax::PathExpression>()?;
            let head = path.path.node.segments.first()?.node.name.node.name.clone();
            let declaration = resolve_lexical_declaration(program, index, node, &head)?;
            (!is_ancestor(index, lambda.node, declaration)).then(|| (syntax.span().map_or(0, |span| span.start), declaration, head))
        })
        .collect::<Vec<_>>();
    uses.sort();
    let mut captures: Vec<(AstNodeKey, String)> = Vec::new();
    for (_, declaration, name) in uses {
        let declaration = AstNodeKey { node: declaration, ..lambda };
        if !captures.iter().any(|(known, _)| *known == declaration) {
            captures.push((declaration, name));
        }
    }
    captures
}

fn containing_callable(index: &SyntaxIndex, node: AstNodeId) -> Option<AstNodeId> {
    nearest_ancestor(index, node, |kind| {
        matches!(
            kind,
            NodeKind::FunctionDefinition | NodeKind::MethodDefinition | NodeKind::TestDefinition | NodeKind::LambdaExpression
        )
    })
}

/// The bound and the capture classes of a lambda literal passed to a `[Captures]` parameter or
/// initializing a `[Captures]` field, for hover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedLambdaCaptures {
    pub bound: TransferMarker,
    /// `(name, type, class)` per capture, in capture order.
    pub captures: Vec<(String, String, String)>,
}

pub fn bounded_lambda_captures(db: &dyn Db, lambda: AstNodeKey) -> Option<BoundedLambdaCaptures> {
    let Some(CapturesBound::Marker(bound)) = bound_of_position(db, lambda) else { return None };
    let syntax = db.syntax_unit(lambda.unit)?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let captures = lambda_captures(program, index, lambda)
        .into_iter()
        .filter_map(|(declaration, name)| {
            let identity = generic_source_local_identity(db, program, index, declaration, declaration.node).ok()?;
            let class = transfer_class(db, declaration, &identity).ok()?;
            Some((name, identity_spelling(&identity), class.name().to_owned()))
        })
        .collect();
    Some(BoundedLambdaCaptures { bound, captures })
}

/// The `[Captures]` bound name of the parameter (or field) declaration `declaration`.
pub fn declared_captures_bound(db: &dyn Db, declaration: AstNodeKey) -> Option<String> {
    with_declaration(db, declaration, |node| {
        let attributes = node
            .of::<beskid_analysis::syntax::Parameter>()
            .map(|parameter| &parameter.attributes)
            .or_else(|| node.of::<beskid_analysis::syntax::Field>().map(|field| &field.attributes))?;
        match captures_bound(db, declaration, attributes)? {
            (CapturesBound::Marker(marker), _) => Some(marker.name().to_owned()),
            (CapturesBound::NotMarker(spelling), _) => spelling,
        }
    })
}

/// `__isolate_spawn<T>(body, scope)` or `__isolate_spawn_local<T>(body, scope)`.
fn isolate_spawn_call(call: &beskid_analysis::syntax::CallExpression) -> bool {
    let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else { return false };
    matches!(path.node.path.node.segments.as_slice(),
        [segment] if matches!(segment.node.name.node.name.as_str(), "__isolate_spawn" | "__isolate_spawn_local"))
}

/// `rt` R7: the body of an isolate spawn is a function value without parameters (`() => T`), and
/// its result `T` crosses back to the parent, so it must be transferable. Captures are judged by
/// [`captures_argument_findings`] through the implied `Transferable` bound.
pub(super) fn isolate_spawn_findings(db: &dyn Db, key: AstNodeKey) -> Vec<SemanticFinding> {
    let Some(syntax) = db.syntax_unit(key.unit) else { return Vec::new() };
    let bodies = match with_node(db, syntax, key, |program, index, _node| {
        let mut calls = Vec::new();
        collect_nodes_of_kind(index, key.node, NodeKind::CallExpression, &mut calls);
        Some(
            calls
                .into_iter()
                .filter_map(|call| {
                    let expression = index.node_at(program, call)?.of::<beskid_analysis::syntax::CallExpression>()?;
                    if !isolate_spawn_call(expression) {
                        return None;
                    }
                    let body = index.direct_child_id(program, call, DynNodeRef::from(expression.args.first()?))?;
                    Some((call, normalized_expression_node(index, body)))
                })
                .collect::<Vec<_>>(),
        )
    }) {
        Ok(Some(bodies)) => bodies,
        Ok(None) | Err(_) => return Vec::new(),
    };
    let mut findings = Vec::new();
    for (call, body) in bodies {
        let (call, body) = (AstNodeKey { node: call, ..key }, AstNodeKey { node: body, ..key });
        let Ok(identity) = generic_source_expression_identity(db, body) else { continue };
        let GenericSourceTypeIdentity::Function { parameters, result } = &identity else {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::TypeCallArgumentMismatch {
                    expected_name: "() => T".to_owned(),
                    actual_name: identity_spelling(&identity),
                },
                site: body,
                related: vec![(call, "in this call")],
            });
            continue;
        };
        if !parameters.is_empty() {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::TypeCallArgumentMismatch {
                    expected_name: "() => T".to_owned(),
                    actual_name: identity_spelling(&identity),
                },
                site: body,
                related: vec![(call, "in this call")],
            });
            continue;
        }
        if let Ok(class) = transfer_class(db, body, result)
            && !class.is_transferable()
        {
            findings.push(SemanticFinding {
                kind: SemanticIssueKind::CaptureNotTransferable {
                    name: "result".to_owned(),
                    type_name: identity_spelling(result),
                    class: class.to_string(),
                },
                site: body,
                related: vec![(call, "in this call")],
            });
        }
    }
    findings
}
