//! Canonical semantic layout implementation.

use super::super::*;
use beskid_abi::runtime_source::{
    CANONICAL_CORELIB_MUTEX_GUARD_SOURCE_PATH, CANONICAL_CORELIB_MUTEX_SOURCE_PATH,
    CANONICAL_FOUNDATION_DEADLINE_SOURCE_PATH, CANONICAL_FOUNDATION_PROCESS_SOURCE_PATH,
    CANONICAL_NETWORK_INTERNAL_SOURCE_PATH, CANONICAL_NETWORK_RESOURCES_SOURCE_PATH,
    CANONICAL_NETWORK_TCP_LISTENER_SOURCE_PATH, CANONICAL_NETWORK_TCP_STREAM_SOURCE_PATH,
    CANONICAL_NETWORK_UDP_SOCKET_SOURCE_PATH,
};
use beskid_analysis::syntax_query::DynNodeRef;
use crate::semantic_contract::typing::pattern_binding_fact_in_environment;

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn aggregate_field_access_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AggregateFieldAccess> {
    with_node(db, syntax, key, |program, index, node| {
        aggregate_field_access_for_environment(db, program, index, key, node, None, None)
    })?
    .transpose()
}

/// Resolve a field access while preserving the complete enclosing specialization.
pub fn aggregate_field_access_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: &GenericSpecializationInstance,
) -> SemanticQueryResult<AggregateFieldAccess> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return Ok(None);
    };
    let ambient = enclosing
        .substitutions
        .iter()
        .map(|binding| {
            crate::semantic_contract::layouts::aggregate_shape_for_binding(db, key, binding)
                .map(|shape| (binding.parameter.to_string(), shape))
        })
        .collect::<Result<HashMap<_, _>, SemanticError>>()?;
    with_node(db, syntax, key, |program, index, node| {
        aggregate_field_access_for_environment(db, program, index, key, node, Some(&ambient), Some(enclosing))
    })?
    .transpose()
}

fn aggregate_field_access_for_environment(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
    ambient: Option<&HashMap<String, AggregateFieldShape>>,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Option<Result<AggregateFieldAccess, SemanticError>> {
    // A projection rooted in an enum-pattern binding of a generic item (`success.rest` inside
    // `Rest<T>`) is proven only through the enclosing specialization's match fact; the
    // target-neutral projection cannot name `T` and must not pre-empt the specialized answer.
    if let Some(projection) =
        nominal_field_projection_in_environment(db, key, enclosing.map(|enclosing| &enclosing.substitutions))
    {
        return Some(projection.map(|(access, _)| access));
    }
    let resolved = field_access_receiver(db, program, index, key, node, ambient, enclosing)?;
    Some(resolved.and_then(|FieldAccessReceiver { declaration, receiver, layout, field_name }| {
        let field_index = layout
            .fields
            .iter()
            .position(|(name, _)| name.as_ref() == field_name)
            .and_then(|index| u32::try_from(index).ok())
            .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
        if let Some(target) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))
            && let Some(definition) = target
                .syntax_index(db)
                .node_at(target.expanded_program(db), declaration.node)
                .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        {
            let field = definition
                .fields
                .iter()
                .filter(|field| field.node.kind == beskid_analysis::syntax::FieldKind::Value)
                .nth(field_index as usize)
                .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
            if field_inaccessible(db, key, declaration, definition, field, PrivateFieldUse::Read) {
                return Err(SemanticError::unavailable("aggregate_field_access.visibility"));
            }
        }
        Ok(AggregateFieldAccess { declaration, receiver, index: field_index, layout })
    }))
}

/// The nominal receiver a single-step field access resolves against, before its field name is
/// looked up in the receiver layout.
pub(in crate::semantic_contract) struct FieldAccessReceiver<'a> {
    pub(in crate::semantic_contract) declaration: AstNodeKey,
    pub(in crate::semantic_contract) receiver: AstNodeKey,
    pub(in crate::semantic_contract) layout: AggregateLayoutFact,
    pub(in crate::semantic_contract) field_name: &'a str,
}

/// Resolve the receiver of a single-step field access (`call().field`, `this.field`,
/// `local.field`, or a bare implicit-receiver `field`). This is the receiver authority of
/// `aggregate_field_access` after `nominal_field_projection` declined the node; the legality gate
/// reads it to tell an unknown field name (E1211) from a receiver that does not resolve.
/// `None` when `node` is not such a shape; `Some(Err)` when its receiver does not resolve.
pub(in crate::semantic_contract) fn field_access_receiver<'a>(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'a>,
    ambient: Option<&HashMap<String, AggregateFieldShape>>,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Option<Result<FieldAccessReceiver<'a>, SemanticError>> {
    if let Some(member) = node.of::<beskid_analysis::syntax::MemberExpression>() {
        let receiver = index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(member.target.as_ref()),
        )?;
        let receiver = AstNodeKey { node: normalized_expression_node(index, receiver), ..key };
        let field_name = member.member.node.name.as_str();
        // `(this).field` / `Member(Local("this"), field)`: the implicit method receiver, exactly
        // like the `this.field` path spelling below.
        if is_implicit_receiver_expression(program, index, receiver) {
            return Some(applied_method_receiver_layout(db, program, index, key, ambient).map(
                |(declaration, receiver, layout)| FieldAccessReceiver { declaration, receiver, layout, field_name },
            ));
        }
        // `(value).field` / `Member(Local("value"), field)`: a named local or parameter, exactly
        // like the `value.field` path spelling below (generic in the enclosing specialization).
        // Parentheses around the receiver name (`(value).field`) do not change it.
        let mut named = receiver.node;
        while let Some(grouped) =
            index.node_at(program, named).and_then(|node| node.of::<beskid_analysis::syntax::GroupedExpression>())
        {
            named = normalized_expression_node(
                index,
                index.direct_child_id(
                    program,
                    named,
                    beskid_analysis::syntax_query::DynNodeRef::from(grouped.expr.as_ref()),
                )?,
            );
        }
        if let Some(path) = index
            .node_at(program, named)
            .and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
            && let [segment] = path.path.node.segments.as_slice()
            && segment.node.type_args.is_empty()
            && resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str()).is_some()
        {
            return Some(
                applied_local_receiver_layout(db, program, index, key, segment.node.name.node.name.as_str(), ambient)
                    .map(|(declaration, receiver, layout)| FieldAccessReceiver {
                        declaration,
                        receiver,
                        layout,
                        field_name,
                    }),
            );
        }
        let resolved = applied_call_result_layout(db, receiver, &member.target.node, enclosing);
        return Some(resolved.map(|(declaration, layout)| FieldAccessReceiver {
            declaration,
            receiver,
            layout,
            field_name,
        }));
    }
    let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
    let resolved = match path.path.node.segments.as_slice() {
        // `this.field` (or the older `self.field`) names a field of the implicit method receiver,
        // exactly like the bare `field` spelling below, unless a lexical declaration shadows it.
        [receiver, field]
            if receiver.node.type_args.is_empty()
                && field.node.type_args.is_empty()
                && is_implicit_receiver_name(program, index, key, receiver.node.name.node.name.as_str()) =>
        {
            applied_method_receiver_layout(db, program, index, key, ambient).map(|(declaration, receiver, layout)| {
                (declaration, receiver, layout, field.node.name.node.name.as_str())
            })
        }
        [receiver, field] if receiver.node.type_args.is_empty() && field.node.type_args.is_empty() => {
            applied_local_receiver_layout(db, program, index, key, receiver.node.name.node.name.as_str(), ambient).map(
                |(declaration, receiver, layout)| (declaration, receiver, layout, field.node.name.node.name.as_str()),
            )
        }
        // A bare `field` names a field of the implicit method receiver only when no parameter or
        // local of that name is in scope: a lexical declaration shadows the method-owned field.
        [field]
            if field.node.type_args.is_empty()
                && resolve_lexical_declaration(program, index, key.node, field.node.name.node.name.as_str()).is_none() =>
        {
            applied_method_receiver_layout(db, program, index, key, ambient).map(|(declaration, receiver, layout)| {
                (declaration, receiver, layout, field.node.name.node.name.as_str())
            })
        }
        _ => return None,
    };
    Some(resolved.map(|(declaration, receiver, layout, field_name)| FieldAccessReceiver {
        declaration,
        receiver,
        layout,
        field_name,
    }))
}

/// A dotted value path has real indexed segment nodes. A segment denotes the prefix ending
/// there; these keys let consumers lower intermediate projections without synthesizing syntax.
pub(in crate::semantic_contract) fn path_projection_segment(
    db: &dyn Db,
    key: AstNodeKey,
    position: usize,
) -> Option<AstNodeKey> {
    let syntax = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key))?;
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let node = index.node_at(program, key.node)?;
    let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
    let path_node = index.direct_child_id(program, key.node, DynNodeRef::from(&path.path))?;
    let segment = path.path.node.segments.get(position)?;
    let node = index.direct_child_id(program, path_node, DynNodeRef::from(segment))?;
    Some(AstNodeKey { node, ..key })
}

/// Prove a nominal projection from an explicitly typed lexical root or a source-proven value. Source substitutions
/// follow each declared field type, never the pointer-shaped ABI. Unknown and inaccessible
/// fields fail closed. A full path with only one field keeps its established fact path.
pub(in crate::semantic_contract) fn nominal_field_projection(
    db: &dyn Db,
    key: AstNodeKey,
) -> Option<Result<(AggregateFieldAccess, GenericSourceTypeIdentity), SemanticError>> {
    nominal_field_projection_in_environment(db, key, None)
}

/// [`nominal_field_projection`] inside the immutable specialization of the enclosing item.
///
/// An enum-pattern binding root (`Result::Ok(success) => success.rest`) has no written
/// annotation; its identity comes from the enclosing match fact. When that match scrutinizes a
/// type that still names an enclosing generic (`Result<T> result` inside `Rest<T>`), only the
/// specialized match fact can bind it, so the enclosing substitutions are applied to the root
/// before any field is projected. Every other root keeps the target-neutral resolution.
pub(in crate::semantic_contract) fn nominal_field_projection_in_environment(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&Arc<[GenericSubstitution]>>,
) -> Option<Result<(AggregateFieldAccess, GenericSourceTypeIdentity), SemanticError>> {
    nominal_field_projection_at(db, key, enclosing).map(|result| {
        result.map_err(|error| {
            // Name an unsited projection gap at the projected path, keeping its exact family.
            let Some(query) = error.unavailable_query().filter(|query| {
                error.unavailable_site().is_none()
                    && (*query == "nominal_field_projection" || query.starts_with("nominal_field_projection."))
            }) else {
                return error;
            };
            let rendered_site = crate::semantic_contract::format_ast_node_site(db, key);
            let reason = if query == "nominal_field_projection.visibility" {
                "the projected field is not `pub` and is private to its declaring source unit"
            } else {
                "the receiver identity or projected field is not proven"
            };
            SemanticError::unavailable_at_described(query, key, &rendered_site, reason)
        })
    })
}

fn nominal_field_projection_at(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&Arc<[GenericSubstitution]>>,
) -> Option<Result<(AggregateFieldAccess, GenericSourceTypeIdentity), SemanticError>> {
    let syntax = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key))?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let node = index.node_at(program, key.node)?;
    if let Some(member) = node.of::<beskid_analysis::syntax::MemberExpression>() {
        // Existing generic-call-result projections keep their established specialized path.
        if matches!(member.target.node, beskid_analysis::syntax::Expression::Call(_)) {
            return None;
        }
        let receiver = index.direct_child_id(program, key.node, DynNodeRef::from(member.target.as_ref()))?;
        let receiver = AstNodeKey { node: normalized_expression_node(index, receiver), ..key };
        // `this.field` as a member access (`(this).field`, or the `Member(Local("this"), field)`
        // a Mod spells) is the same projection from the implicit receiver as the path spelling.
        if is_implicit_receiver_expression(program, index, receiver) {
            return implicit_receiver_field_projection(db, program, index, key, &member.member.node.name, enclosing);
        }
        let identity = generic_source_expression_identity(db, receiver);
        // A named local or parameter whose type names the enclosing generic parameters
        // (`Member(Local("value"), field)` with `Boxed<T> value`) has no target-neutral identity;
        // the receiver layout of its declared type, applied in the environment, owns that access.
        if identity.as_ref().is_err_and(|error| error.is_unavailable())
            && index
                .node_at(program, receiver.node)
                .and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
                .is_some_and(|path| match path.path.node.segments.as_slice() {
                    [segment] => {
                        segment.node.type_args.is_empty()
                            && resolve_lexical_declaration(program, index, key.node, &segment.node.name.node.name)
                                .is_some()
                    }
                    _ => false,
                })
        {
            return None;
        }
        return Some(
            identity.and_then(|identity| project_nominal_field(db, key, receiver, &identity, &member.member.node.name)),
        );
    }
    let (path_key, last) = if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>() {
        // `field.member` inside a method is a two-step projection from the implicit receiver
        // when `field` is not a lexical local; other two-segment spellings keep their own facts.
        let implicit_field_root = path.path.node.segments.len() == 2
            && path.path.node.segments.iter().all(|segment| segment.node.type_args.is_empty())
            && !is_implicit_receiver_name(program, index, key, &path.path.node.segments[0].node.name.node.name)
            && resolve_lexical_declaration(program, index, key.node, &path.path.node.segments[0].node.name.node.name)
                .is_none();
        // `binding.field` where `binding` is an enum-pattern binding (`Option::Some(binding)`):
        // the binding has no written annotation, so its enum match fact is the only authority for
        // the receiver identity. That is exactly the lexical-root chain below; parameter and
        // `let` roots keep their own two-segment facts.
        let pattern_binding_root = path.path.node.segments.len() == 2
            && path.path.node.segments.iter().all(|segment| segment.node.type_args.is_empty())
            && resolve_lexical_declaration(program, index, key.node, &path.path.node.segments[0].node.name.node.name)
                .and_then(|declaration| parent_node(index, declaration))
                .is_some_and(|parent| index.kind(parent) == Some(beskid_analysis::syntax_query::NodeKind::Pattern));
        if path.path.node.segments.len() < 3 && !implicit_field_root && !pattern_binding_root {
            return None;
        }
        (key, path.path.node.segments.len() - 1)
    } else if node.of::<beskid_analysis::syntax::PathSegment>().is_some() {
        let path_node = parent_node(index, key.node)?;
        let expression = parent_node(index, path_node)?;
        let path = index.node_at(program, expression)?.of::<beskid_analysis::syntax::PathExpression>()?;
        let expression = AstNodeKey { node: expression, ..key };
        let position = (0..path.path.node.segments.len())
            .find(|position| path_projection_segment(db, expression, *position) == Some(key))?;
        let root = path.path.node.segments.first()?;
        if root.node.type_args.is_empty()
            && is_implicit_receiver_name(program, index, expression, root.node.name.node.name.as_str())
        {
            // `this.field...`: `this` itself is the receiver, not a projection; its first field
            // is a projection from the implicit receiver.
            match position {
                0 => return None,
                1 => {
                    let field = path.path.node.segments.get(1)?;
                    if !field.node.type_args.is_empty() {
                        return None;
                    }
                    return implicit_receiver_field_projection(
                        db,
                        program,
                        index,
                        expression,
                        field.node.name.node.name.as_str(),
                        enclosing,
                    );
                }
                _ => {}
            }
        }
        if position == 0 {
            // The first segment of a dotted path is ordinarily a lexical root, not a
            // projection. Inside a method the same spelling can instead name a field of the
            // implicit receiver, which is a real projection from `this`.
            let segment = path.path.node.segments.first()?;
            if !segment.node.type_args.is_empty()
                || resolve_lexical_declaration(program, index, expression.node, segment.node.name.node.name.as_str())
                    .is_some()
            {
                return None;
            }
            let field_name = segment.node.name.node.name.as_str();
            return implicit_receiver_field_projection(db, program, index, expression, field_name, enclosing);
        }
        (expression, position)
    } else {
        return None;
    };
    let path = &index.node_at(program, path_key.node)?.of::<beskid_analysis::syntax::PathExpression>()?.path.node;
    if path.segments[..=last].iter().any(|segment| !segment.node.type_args.is_empty()) {
        return None;
    }
    if is_implicit_receiver_name(program, index, path_key, &path.segments[0].node.name.node.name) {
        // `this.a.b`: project `a` from the implicit receiver, then each later field from the
        // previous segment, exactly like a lexical root's chain.
        let first =
            implicit_receiver_field_projection(db, program, index, path_key, &path.segments[1].node.name.node.name, enclosing)?;
        return Some((|| {
            let (mut result, mut identity) = first?;
            let mut receiver = path_projection_segment(db, path_key, 1)
                .ok_or_else(|| SemanticError::unavailable("nominal_field_projection"))?;
            for position in 2..=last {
                let (access, next_identity) =
                    project_nominal_field(db, key, receiver, &identity, &path.segments[position].node.name.node.name)?;
                receiver = path_projection_segment(db, path_key, position)
                    .ok_or_else(|| SemanticError::unavailable("nominal_field_projection"))?;
                identity = next_identity;
                result = access;
            }
            Ok((result, identity))
        })());
    }
    let Some(root) = resolve_lexical_declaration(program, index, path_key.node, &path.segments[0].node.name.node.name)
    else {
        // `field.a...` inside a method: the root names a field of the implicit receiver.
        let first =
            implicit_receiver_field_projection(db, program, index, path_key, &path.segments[0].node.name.node.name, enclosing)?;
        return Some((|| {
            let (mut result, mut identity) = first?;
            let mut receiver = path_projection_segment(db, path_key, 0)
                .ok_or_else(|| SemanticError::unavailable("nominal_field_projection"))?;
            for position in 1..=last {
                let (access, next_identity) =
                    project_nominal_field(db, key, receiver, &identity, &path.segments[position].node.name.node.name)?;
                receiver = path_projection_segment(db, path_key, position)
                    .ok_or_else(|| SemanticError::unavailable("nominal_field_projection"))?;
                identity = next_identity;
                result = access;
            }
            Ok((result, identity))
        })());
    };
    let annotation = explicit_local_complex_type_path(program, index, root);
    // An enum-pattern binding has no written annotation; its enum match fact is the sole
    // authority for the exact applied payload identity. An unannotated `let` takes the shared
    // source-proven local identity of its initializer. Other unannotated roots stay unproven.
    let binding = match annotation {
        Some(_) => None,
        None => Some(match pattern_binding_fact_in_environment(db, index, path_key, root, enclosing) {
            Some(binding) => binding.and_then(|binding| {
                binding.source_identity.ok_or_else(|| SemanticError::unavailable("nominal_field_projection"))
            }),
            // Only a proven nominal identity is a projection root; arrays, scalars and
            // unproven locals keep their established non-projection paths.
            None => Ok(generic_source_local_identity(db, program, index, path_key, root)
                .ok()
                .filter(|identity| matches!(identity, GenericSourceTypeIdentity::Nominal { .. }))?),
        }),
    };
    Some((|| {
        let mut identity = match (annotation, binding) {
            // A root whose written type names the enclosing generics (`Outer<T> o` in
            // `o.inner.slot`) is applied in the specialization's environment when there is one.
            (Some(annotation), _) => {
                let written = beskid_analysis::syntax::Type::Complex(beskid_analysis::syntax::Spanned::new(
                    annotation.clone(),
                    path.segments[0].span,
                ));
                match enclosing {
                    Some(enclosing) => generic_source_type_identity_with_substitutions(
                        db,
                        path_key,
                        &written,
                        &enclosing
                            .iter()
                            .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
                            .collect(),
                    )?,
                    None => generic_source_type_identity(db, path_key, &written)?,
                }
            }
            (None, Some(binding)) => binding?,
            (None, None) => return Err(SemanticError::unavailable("nominal_field_projection")),
        };
        let mut receiver = AstNodeKey { node: root, ..key };
        let mut result = None;
        for position in 1..=last {
            let (access, next_identity) =
                project_nominal_field(db, key, receiver, &identity, &path.segments[position].node.name.node.name)?;
            receiver = path_projection_segment(db, path_key, position)
                .ok_or_else(|| SemanticError::unavailable("nominal_field_projection"))?;
            identity = next_identity;
            result = Some(access);
        }
        Ok((result.ok_or_else(|| SemanticError::unavailable("nominal_field_projection"))?, identity))
    })())
}

/// Project one field of the implicit method receiver named by an unqualified path root.
///
/// Inside a method body `field.Member` spells a projection from `this`. The enclosing method's
/// own type definition is the sole authority: a unique declared value field proves the
/// projection, and every other spelling (no enclosing method, a generic enclosing type, an
/// unknown or ambiguous field name) stays unproven rather than guessed.
pub(in crate::semantic_contract) fn implicit_receiver_field_projection(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: AstNodeKey,
    field_name: &str,
    enclosing: Option<&Arc<[GenericSubstitution]>>,
) -> Option<Result<(AggregateFieldAccess, GenericSourceTypeIdentity), SemanticError>> {
    let method = nearest_ancestor(index, reference.node, |kind| {
        kind == beskid_analysis::syntax_query::NodeKind::MethodDefinition
    })?;
    let declaration = method_owner_node(program, index, method)?;
    let definition = index.node_at(program, declaration)?.of::<beskid_analysis::syntax::TypeDefinition>()?;
    // A generic enclosing receiver is proven only inside the specialization of the enclosing
    // method, whose substitutions bind the owner's parameters by name; the non-generic spelling
    // is proven by the definition alone.
    let bindings = if definition.generics.is_empty() {
        Vec::new()
    } else {
        let enclosing = enclosing?;
        definition
            .generics
            .iter()
            .map(|generic| {
                enclosing.iter().find(|binding| binding.parameter.as_ref() == generic.node.name.as_str())
            })
            .collect::<Option<Vec<_>>>()?
    };
    let declaration = AstNodeKey { node: declaration, ..reference };
    let matches = definition
        .fields
        .iter()
        .filter(|field| field.node.kind == beskid_analysis::syntax::FieldKind::Value)
        .enumerate()
        .filter(|(_, field)| field.node.name.node.name == field_name)
        .collect::<Vec<_>>();
    let [(field_index, field)] = matches.as_slice() else {
        return None;
    };
    let field_index = u32::try_from(*field_index).ok()?;
    let field_type = field.node.ty.node.clone();
    Some((|| {
        let shapes = bindings
            .iter()
            .map(|binding| {
                crate::semantic_contract::layouts::aggregate_shape_for_binding(db, reference, binding)
                    .map(|shape| (binding.parameter.to_string(), shape))
            })
            .collect::<Result<HashMap<_, _>, SemanticError>>()?;
        let environment = bindings
            .iter()
            .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
            .collect::<HashMap<_, _>>();
        let layout = aggregate_layout_from_definition(
            db,
            program,
            index,
            declaration,
            definition,
            (!bindings.is_empty()).then_some(&shapes),
        )?;
        let identity = generic_source_type_identity_with_substitutions(db, declaration, &field_type, &environment)?;
        let access = AggregateFieldAccess {
            declaration,
            receiver: AstNodeKey { node: method, ..reference },
            index: field_index,
            layout,
        };
        Ok((access, identity))
    })())
}

/// The applied source identity of a field read inside the immutable specialization of the
/// enclosing item: `this.slot`, a bare implicit-receiver `slot`, `h.slot`, `(h).slot`, or a
/// longer chain such as `o.inner.slot`.
///
/// The target-neutral projection cannot name a receiver whose declared type mentions an enclosing
/// generic (`Holder<T> h`, the receiver of a method of `Holder<T>`), so a field typed by it
/// (`Option<T[]> slot`) has no identity there. Here the root is read in the specialization's
/// environment (a parameter or annotated local through its written type, an enum-pattern binding
/// through its specialized match, the implicit receiver through the owner's bound parameters)
/// and every later field is projected from that applied identity. `None` when `key` is not such a
/// field read; an unproven root or field stays an error rather than a guess.
pub(in crate::semantic_contract) fn field_read_identity_in_environment(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: &Arc<[GenericSubstitution]>,
) -> Option<Result<GenericSourceTypeIdentity, SemanticError>> {
    let syntax = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key))?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let key = AstNodeKey { node: normalized_expression_node(index, key.node), ..key };
    let node = index.node_at(program, key.node)?;
    if let Some(member) = node.of::<beskid_analysis::syntax::MemberExpression>() {
        let receiver = index.direct_child_id(program, key.node, DynNodeRef::from(member.target.as_ref()))?;
        let receiver = AstNodeKey { node: normalized_expression_node(index, receiver), ..key };
        let field = member.member.node.name.as_str();
        if is_implicit_receiver_expression(program, index, receiver) {
            return implicit_receiver_field_projection(db, program, index, key, field, Some(enclosing))
                .map(|projection| projection.map(|(_, identity)| identity));
        }
        let identity = field_read_identity_in_environment(db, receiver, enclosing)
            .or_else(|| local_root_identity_in_environment(db, program, index, receiver, enclosing))?;
        return Some(identity.and_then(|identity| {
            project_nominal_field(db, key, receiver, &identity, field).map(|(_, identity)| identity)
        }));
    }
    let path = &node.of::<beskid_analysis::syntax::PathExpression>()?.path.node;
    if path.segments.iter().any(|segment| !segment.node.type_args.is_empty()) {
        return None;
    }
    let names = path.segments.iter().map(|segment| segment.node.name.node.name.as_str()).collect::<Vec<_>>();
    let lexical_root = resolve_lexical_declaration(program, index, key.node, names[0]).is_some();
    // The root identity and the first field still to project from it.
    let (root, first) = match names.as_slice() {
        [field] if !lexical_root => {
            return implicit_receiver_field_projection(db, program, index, key, field, Some(enclosing))
                .map(|projection| projection.map(|(_, identity)| identity));
        }
        [_] => return None,
        [receiver, field, ..] if is_implicit_receiver_name(program, index, key, receiver) => {
            (implicit_receiver_field_projection(db, program, index, key, field, Some(enclosing))?.map(|(_, identity)| identity), 2)
        }
        [field, ..] if !lexical_root => {
            (implicit_receiver_field_projection(db, program, index, key, field, Some(enclosing))?.map(|(_, identity)| identity), 1)
        }
        [root, ..] => {
            let declaration = resolve_lexical_declaration(program, index, key.node, root)?;
            (local_declaration_identity_in_environment(db, program, index, key, declaration, enclosing)?, 1)
        }
        [] => return None,
    };
    Some(root.and_then(|mut identity| {
        for field in &names[first..] {
            identity = project_nominal_field(db, key, key, &identity, field)?.1;
        }
        Ok(identity)
    }))
}

/// The identity of a single-name local or parameter read (`h` in `(h).slot`) in the environment.
fn local_root_identity_in_environment(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    enclosing: &Arc<[GenericSubstitution]>,
) -> Option<Result<GenericSourceTypeIdentity, SemanticError>> {
    let path = index.node_at(program, key.node)?.of::<beskid_analysis::syntax::PathExpression>()?;
    let [segment] = path.path.node.segments.as_slice() else {
        return None;
    };
    if !segment.node.type_args.is_empty() {
        return None;
    }
    let declaration = resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())?;
    local_declaration_identity_in_environment(db, program, index, key, declaration, enclosing)
}

/// The identity a lexical declaration binds in the environment: a parameter or annotated `let`
/// through its written type with the enclosing parameters substituted, an enum-pattern binding
/// through its specialized match, and an unannotated `let` through its initializer's own
/// target-neutral identity.
fn local_declaration_identity_in_environment(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: beskid_analysis::syntax::AstNodeId,
    enclosing: &Arc<[GenericSubstitution]>,
) -> Option<Result<GenericSourceTypeIdentity, SemanticError>> {
    let parent = parent_node(index, declaration)?;
    let environment = enclosing
        .iter()
        .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
        .collect::<HashMap<_, _>>();
    let parent_key = AstNodeKey { node: parent, ..key };
    let written = match index.kind(parent)? {
        beskid_analysis::syntax_query::NodeKind::Parameter => {
            index.node_at(program, parent)?.of::<beskid_analysis::syntax::Parameter>().map(|parameter| &parameter.ty.node)
        }
        beskid_analysis::syntax_query::NodeKind::LetStatement => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::LetStatement>()?
            .type_annotation
            .as_ref()
            .map(|annotation| &annotation.node),
        beskid_analysis::syntax_query::NodeKind::Pattern => {
            return Some(pattern_binding_fact_in_environment(db, index, key, declaration, Some(enclosing))?.and_then(
                |binding| binding.source_identity.ok_or_else(|| SemanticError::unavailable("nominal_field_projection")),
            ));
        }
        _ => None,
    };
    Some(match written {
        Some(written) => generic_source_type_identity_with_substitutions(db, parent_key, written, &environment),
        None => generic_source_local_identity(db, program, index, key, declaration),
    })
}

fn project_nominal_field(
    db: &dyn Db,
    key: AstNodeKey,
    receiver: AstNodeKey,
    identity: &GenericSourceTypeIdentity,
    field_name: &str,
) -> Result<(AggregateFieldAccess, GenericSourceTypeIdentity), SemanticError> {
    let (declaration, layout) = nominal_identity_layout(db, key, identity)?;
    let target = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .ok_or_else(|| SemanticError::unavailable("nominal_field_projection"))?;
    let definition = target
        .syntax_index(db)
        .node_at(target.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        .ok_or_else(|| SemanticError::unavailable("nominal_field_projection"))?;
    let GenericSourceTypeIdentity::Nominal { arguments, .. } = identity else { unreachable!() };
    let environment = definition
        .generics
        .iter()
        .zip(arguments.iter())
        .map(|(generic, argument)| (generic.node.name.as_str(), argument))
        .collect::<HashMap<_, _>>();
    let matches = definition
        .fields
        .iter()
        .filter(|field| field.node.kind == beskid_analysis::syntax::FieldKind::Value)
        .enumerate()
        .filter(|(_, field)| field.node.name.node.name == field_name)
        .collect::<Vec<_>>();
    let [(field_index, field)] = matches.as_slice() else {
        return Err(SemanticError::unavailable("nominal_field_projection"));
    };
    if field_inaccessible(db, key, declaration, definition, field, PrivateFieldUse::Read) {
        return Err(SemanticError::unavailable("nominal_field_projection.visibility"));
    }
    let next_identity =
        generic_source_type_identity_with_substitutions(db, declaration, &field.node.ty.node, &environment)?;
    let access = AggregateFieldAccess {
        declaration,
        receiver,
        index: u32::try_from(*field_index).map_err(|_| SemanticError::unavailable("nominal_field_projection"))?,
        layout,
    };
    Ok((access, next_identity))
}

/// How a source unit uses a field of a nominal type declared in another unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::semantic_contract) enum PrivateFieldUse {
    /// A field read: `value.field`, `call().field`, or one step of a projection chain.
    Read,
    /// A field supplied (or required) by a struct literal of the type.
    Construct,
}

/// The single field visibility policy shared by direct field access, nominal projection chains
/// (`request.targets.targetIds`), and aggregate literals. A field without `pub` is private to the
/// source unit (the logical module) that declares its type: that unit, including the type's
/// inline methods, may read and construct it, and every other unit is denied. `extend type`
/// bodies in another unit follow the same rule. The exact canonical admissions in
/// [`canonical_private_field_admission`] are the only cross-unit exceptions.
pub(in crate::semantic_contract) fn field_inaccessible(
    db: &dyn Db,
    key: AstNodeKey,
    declaration: AstNodeKey,
    definition: &beskid_analysis::syntax::TypeDefinition,
    field: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Field>,
    usage: PrivateFieldUse,
) -> bool {
    // A Mod-generated serialization group appended to the entry is judged in the unit that
    // declares its target (`contribution_attributed_unit`); every other node in its own unit.
    declaration.unit != crate::contribution_attribution::contribution_attributed_unit(db, key)
        && field.node.visibility.node != beskid_analysis::syntax::Visibility::Public
        && !canonical_private_field_admission(
            db,
            key,
            declaration,
            definition.name.node.name.as_str(),
            field.node.name.node.name.as_str(),
            usage,
        )
}

/// Permit a private field across units only when both sides are exact compiler-owned source
/// units. The nominal declaration key, rather than its pointer-shaped ABI or written name alone,
/// proves that a lookalike type cannot reach the private field:
/// - the canonical Network facade and Process source read the checked monotonic `Deadline`
///   sample;
/// - the canonical Network resource authority constructs the `handle` of the canonical
///   `TcpListener`, `TcpStream`, and `UdpSocket` declarations;
/// - the canonical Mutex facade constructs and reads the `mutexHandle` of the canonical
///   `MutexGuard` declaration.
fn canonical_private_field_admission(
    db: &dyn Db,
    key: AstNodeKey,
    declaration: AstNodeKey,
    type_name: &str,
    field_name: &str,
    usage: PrivateFieldUse,
) -> bool {
    let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    let (Some(owner_path), Some(declaration_path)) = (
        registry.corelib_source_paths.get(&(key.unit, key.generation)),
        registry.corelib_source_paths.get(&(declaration.unit, declaration.generation)),
    ) else {
        return false;
    };
    match (usage, type_name, field_name) {
        (PrivateFieldUse::Read, "Deadline", "monotonicNanos") => {
            (owner_path == CANONICAL_NETWORK_INTERNAL_SOURCE_PATH
                || owner_path == CANONICAL_FOUNDATION_PROCESS_SOURCE_PATH)
                && declaration_path == CANONICAL_FOUNDATION_DEADLINE_SOURCE_PATH
        }
        (PrivateFieldUse::Construct, "TcpListener", "handle") => {
            owner_path == CANONICAL_NETWORK_RESOURCES_SOURCE_PATH
                && declaration_path == CANONICAL_NETWORK_TCP_LISTENER_SOURCE_PATH
        }
        (PrivateFieldUse::Construct, "TcpStream", "handle") => {
            owner_path == CANONICAL_NETWORK_RESOURCES_SOURCE_PATH
                && declaration_path == CANONICAL_NETWORK_TCP_STREAM_SOURCE_PATH
        }
        (PrivateFieldUse::Construct, "UdpSocket", "handle") => {
            owner_path == CANONICAL_NETWORK_RESOURCES_SOURCE_PATH
                && declaration_path == CANONICAL_NETWORK_UDP_SOCKET_SOURCE_PATH
        }
        (_, "MutexGuard", "mutexHandle") => {
            owner_path == CANONICAL_CORELIB_MUTEX_SOURCE_PATH
                && declaration_path == CANONICAL_CORELIB_MUTEX_GUARD_SOURCE_PATH
        }
        _ => false,
    }
}

/// Instantiate the aggregate layout denoted by an exact source-proven nominal identity. Each
/// applied argument keeps its own nominal declaration; only scalar arguments use their ABI.
fn nominal_identity_layout(
    db: &dyn Db,
    key: AstNodeKey,
    identity: &GenericSourceTypeIdentity,
) -> Result<(AstNodeKey, AggregateLayoutFact), SemanticError> {
    let declaration = super::super::contracts::concrete_declaration(db, key, identity)
        .ok_or_else(|| SemanticError::unavailable("nominal_field_projection"))?;
    let target = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .ok_or_else(|| SemanticError::unavailable("nominal_field_projection"))?;
    let definition = target
        .syntax_index(db)
        .node_at(target.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        .ok_or_else(|| SemanticError::unavailable("nominal_field_projection"))?;
    let GenericSourceTypeIdentity::Nominal { arguments, .. } = identity else {
        return Err(SemanticError::unavailable("nominal_field_projection"));
    };
    if definition.generics.len() != arguments.len() {
        return Err(SemanticError::unavailable("nominal_field_projection"));
    }
    let shapes = definition
        .generics
        .iter()
        .zip(arguments.iter())
        .map(|(generic, argument)| {
            let shape = if matches!(argument, GenericSourceTypeIdentity::Nominal { .. }) {
                AggregateFieldShape::Nominal(
                    super::super::contracts::concrete_type_or_enum_declaration(db, key, argument)
                        .ok_or_else(|| SemanticError::unavailable("nominal_field_projection"))?,
                )
            } else {
                if argument.managed_reference_kind() == ManagedReferenceKind::GcManaged {
                    AggregateFieldShape::ManagedReference(argument.abi_type())
                } else {
                    AggregateFieldShape::Scalar(argument.abi_type())
                }
            };
            Ok((generic.node.name.clone(), shape))
        })
        .collect::<Result<HashMap<_, _>, SemanticError>>()?;
    let layout = aggregate_layout_from_definition(
        db,
        target.expanded_program(db),
        target.syntax_index(db),
        declaration,
        definition,
        (!definition.generics.is_empty()).then_some(&shapes),
    )?;
    Ok((declaration, layout))
}

/// Layout of the nominal value a direct call returns (`Make().field`, `bag.Put(x).field`).
///
/// The callee's own specialization is the authority for its result type: a non-generic callee
/// has no substitutions, an explicit or inferred generic call carries them, and a call nested in
/// a specialized body resolves them in the enclosing environment. The declared return type is
/// then interpreted in that environment and in the callee's declaring unit, so a result type
/// the call site never imports still projects.
fn applied_call_result_layout(
    db: &dyn Db,
    receiver: AstNodeKey,
    expression: &beskid_analysis::syntax::Expression,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<(AstNodeKey, AggregateLayoutFact), SemanticError> {
    let beskid_analysis::syntax::Expression::Call(_) = expression else {
        return Err(SemanticError::unavailable("aggregate_field_access"));
    };
    let specialization = match enclosing {
        Some(enclosing) => generic_call_specialization_in_environment(db, receiver, enclosing)?,
        None => None,
    }
    .map(Ok)
    .unwrap_or_else(|| generic_specialization_instance_for_call(db, receiver))?;
    let declaration_syntax = db
        .syntax_unit(specialization.declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, specialization.declaration))
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access.declaration_syntax"))?;
    let declaration_node = declaration_syntax
        .syntax_index(db)
        .node_at(declaration_syntax.expanded_program(db), specialization.declaration.node)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access.declaration_node"))?;
    let result = declaration_node
        .of::<beskid_analysis::syntax::FunctionDefinition>()
        .and_then(|function| function.return_type.as_ref())
        .or_else(|| {
            declaration_node.of::<beskid_analysis::syntax::MethodDefinition>().and_then(|method| method.return_type.as_ref())
        })
        .or_else(|| {
            declaration_node
                .of::<beskid_analysis::syntax::ContractMethodSignature>()
                .and_then(|signature| signature.return_type.as_ref())
        })
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access.result"))?;
    let environment = specialization
        .substitutions
        .iter()
        .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
        .collect::<HashMap<_, _>>();
    let identity =
        generic_source_type_identity_with_substitutions(db, specialization.declaration, &result.node, &environment)?;
    nominal_identity_layout(db, receiver, &identity).map_err(|_| SemanticError::unavailable("aggregate_field_access.nominal_result"))
}

fn applied_local_receiver_layout(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    receiver_name: &str,
    ambient: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<(AstNodeKey, AstNodeKey, AggregateLayoutFact), SemanticError> {
    let local = resolve_lexical_declaration(program, index, key.node, receiver_name)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let receiver = AstNodeKey { node: local, ..key };
    let parent = parent_node(index, local).ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    if index.kind(parent) == Some(beskid_analysis::syntax_query::NodeKind::Pattern) {
        return applied_pattern_binding_layout(db, program, index, key, local, ambient)
            .map(|(declaration, layout)| (declaration, receiver, layout));
    }
    if let Some(path) = explicit_local_complex_type_path(program, index, local) {
        return instantiated_aggregate_layout_for_path(db, key, path, ambient)
            .map(|(declaration, layout)| (declaration, receiver, layout));
    }
    // An unannotated local takes its type from the shared source-proven local identity
    // (its initializer's identity). A non-nominal or unproven identity stays unavailable.
    let identity = generic_source_local_identity(db, program, index, key, local)
        .map_err(|_| SemanticError::unavailable("aggregate_field_access"))?;
    nominal_identity_layout(db, key, &identity).map(|(declaration, layout)| (declaration, receiver, layout))
}

/// Whether the expression at `key` is the bare implicit method receiver (`this` or `self`), the
/// target of a member access such as `(this).field`.
fn is_implicit_receiver_expression(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> bool {
    index
        .node_at(program, key.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
        .is_some_and(|path| match path.path.node.segments.as_slice() {
            [root] => {
                root.node.type_args.is_empty()
                    && is_implicit_receiver_name(program, index, key, root.node.name.node.name.as_str())
            }
            _ => false,
        })
}

/// Whether `name` spells the implicit method receiver at `key`: `this` or `self` inside a
/// method body, with no lexical declaration of that name in scope.
pub(in crate::semantic_contract) fn is_implicit_receiver_name(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    name: &str,
) -> bool {
    matches!(name, "this" | "self")
        && resolve_lexical_declaration(program, index, key.node, name).is_none()
        && nearest_ancestor(index, key.node, |kind| kind == beskid_analysis::syntax_query::NodeKind::MethodDefinition)
            .is_some()
}

fn applied_method_receiver_layout(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    ambient: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<(AstNodeKey, AstNodeKey, AggregateLayoutFact), SemanticError> {
    let method =
        nearest_ancestor(index, key.node, |kind| kind == beskid_analysis::syntax_query::NodeKind::MethodDefinition)
            .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let declaration = method_owner_node(program, index, method)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let definition = index
        .node_at(program, declaration)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let declaration = AstNodeKey { node: declaration, ..key };
    let substitutions = if definition.generics.is_empty() {
        None
    } else {
        ambient
            .map(|ambient| {
                definition
                    .generics
                    .iter()
                    .map(|generic| {
                        let name = generic.node.name.clone();
                        ambient
                            .get(name.as_str())
                            .copied()
                            .map(|shape| (name, shape))
                            .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))
                    })
                    .collect::<Result<HashMap<_, _>, SemanticError>>()
            })
            .transpose()?
    };
    aggregate_layout_from_definition(db, program, index, declaration, definition, substitutions.as_ref())
        .map(|layout| (declaration, AstNodeKey { node: method, ..key }, layout))
}

fn applied_pattern_binding_layout(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: beskid_analysis::syntax::AstNodeId,
    ambient: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<(AstNodeKey, AggregateLayoutFact), SemanticError> {
    let arm_id = nearest_ancestor(index, declaration, |kind| kind == beskid_analysis::syntax_query::NodeKind::MatchArm)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let match_id =
        nearest_ancestor(index, arm_id, |kind| kind == beskid_analysis::syntax_query::NodeKind::MatchExpression)
            .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let expression = index
        .node_at(program, match_id)
        .and_then(|node| node.of::<beskid_analysis::syntax::MatchExpression>())
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let arm = expression
        .arms
        .iter()
        .find(|arm| {
            index.direct_child_id(program, match_id, beskid_analysis::syntax_query::DynNodeRef::from(*arm))
                == Some(arm_id)
        })
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let beskid_analysis::syntax::Pattern::Enum(pattern) = &arm.node.pattern.node else {
        return Err(SemanticError::unavailable("aggregate_field_access"));
    };
    let scrutinee_path = match super::enum_layout::ungrouped_scrutinee(expression) {
        beskid_analysis::syntax::Expression::Path(path) => &path.node.path.node,
        // A call scrutinee has no written local type. The enum match fact proves the applied
        // payload identity from the call's own source result type; that exact identity is the
        // only authority here, so an unproven or non-nominal payload stays unavailable.
        beskid_analysis::syntax::Expression::Call(_) => {
            let binding = pattern_binding_fact(db, index, key, declaration)
                .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))??;
            let identity =
                binding.source_identity.ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
            return nominal_identity_layout(db, key, &identity);
        }
        _ => return Err(SemanticError::unavailable("aggregate_field_access")),
    };
    let [scrutinee] = scrutinee_path.segments.as_slice() else {
        return Err(SemanticError::unavailable("aggregate_field_access"));
    };
    let scrutinee_local = resolve_lexical_declaration(program, index, match_id, scrutinee.node.name.node.name.as_str())
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let scrutinee_type = explicit_local_complex_type_path(program, index, scrutinee_local)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let enum_declaration = resolve_type_declaration(db, key, scrutinee_type)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let enum_syntax = db
        .syntax_unit(enum_declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, enum_declaration))
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let enum_definition = enum_syntax
        .syntax_index(db)
        .node_at(enum_syntax.expanded_program(db), enum_declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let terminal =
        scrutinee_type.segments.last().ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    if terminal.node.type_args.len() != enum_definition.generics.len() {
        return Err(SemanticError::unavailable("aggregate_field_access"));
    }
    let enum_substitutions = enum_definition
        .generics
        .iter()
        .zip(terminal.node.type_args.iter())
        .map(|(generic, argument)| {
            applied_aggregate_shape(db, key, &argument.node, ambient).map(|shape| (generic.node.name.clone(), shape))
        })
        .collect::<Result<HashMap<_, _>, SemanticError>>()?;
    let variant_name = pattern.node.path.node.variant.node.name.as_str();
    let variant = enum_definition
        .variants
        .iter()
        .find(|variant| variant.node.name.node.name == variant_name)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let [field] = variant.node.fields.as_slice() else {
        return Err(SemanticError::unavailable("aggregate_field_access"));
    };
    // A payload declared as a bare enum generic (`Ok(TValue value)`) takes its nominal identity
    // from the scrutinee's applied type argument. That argument is source written at the use
    // site, so it resolves there with its own type arguments intact rather than through the
    // enum declaration's scope, where the generic name denotes no type.
    if let Some(parameter) = generic_parameter_reference_name(&field.node.ty.node)
        && let Some(position) = enum_definition.generics.iter().position(|generic| generic.node.name == parameter)
    {
        let beskid_analysis::syntax::Type::Complex(argument) = &terminal.node.type_args[position].node else {
            return Err(SemanticError::unavailable("aggregate_field_access"));
        };
        return instantiated_aggregate_layout_for_path(db, key, &argument.node, ambient);
    }
    let beskid_analysis::syntax::Type::Complex(payload) = &field.node.ty.node else {
        return Err(SemanticError::unavailable("aggregate_field_access"));
    };
    instantiated_aggregate_layout_for_path(db, enum_declaration, &payload.node, Some(&enum_substitutions))
}

fn explicit_local_complex_type_path<'a>(
    program: &'a beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<&'a beskid_analysis::syntax::Path> {
    let parent = parent_node(index, declaration)?;
    let annotation = match index.kind(parent)? {
        beskid_analysis::syntax_query::NodeKind::Parameter => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::Parameter>()
            .map(|parameter| &parameter.ty.node),
        beskid_analysis::syntax_query::NodeKind::LetStatement => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::LetStatement>()
            .and_then(|statement| statement.type_annotation.as_ref())
            .map(|annotation| &annotation.node),
        _ => None,
    }?;
    let beskid_analysis::syntax::Type::Complex(path) = annotation else {
        return None;
    };
    Some(&path.node)
}

/// Resolve the smallest generation-safe member receiver: an unqualified local with an explicit
/// nominal parameter or let annotation. Calls, inferred locals, and chained receivers remain
/// unavailable rather than reconstructing retired HIR type information.
pub(in crate::semantic_contract) fn nominal_local_receiver_declaration(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    receiver_name: &str,
) -> Option<(AstNodeKey, AstNodeKey)> {
    let local = resolve_lexical_declaration(program, index, key.node, receiver_name)?;
    let receiver = AstNodeKey { node: local, ..key };
    let parent = parent_node(index, local)?;
    if index.kind(parent) == Some(beskid_analysis::syntax_query::NodeKind::Pattern) {
        let binding = pattern_binding_fact(db, index, key, local).and_then(Result::ok)?;
        let AggregateFieldShape::Nominal(declaration) = binding.payload else {
            return None;
        };
        return Some((declaration, receiver));
    }
    if let Some(handle) = inferred_spawn_handle(db, receiver) {
        return Some((handle.declaration, receiver));
    }
    let path = explicit_local_complex_type_path(program, index, local)?;
    resolve_type_declaration(db, key, path).map(|declaration| (declaration, receiver))
}
