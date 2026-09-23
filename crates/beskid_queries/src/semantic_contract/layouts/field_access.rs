//! Canonical semantic layout implementation.

use super::super::*;
use beskid_analysis::syntax_query::DynNodeRef;

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
        .map(|binding| (binding.parameter.to_string(), AggregateFieldShape::Scalar(binding.argument)))
        .collect::<HashMap<_, _>>();
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
    if let Some(projection) = nominal_field_projection(db, key) {
        return Some(projection.map(|(access, _)| access));
    }
    if let Some(member) = node.of::<beskid_analysis::syntax::MemberExpression>() {
        let receiver = index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(member.target.as_ref()),
        )?;
        let receiver = AstNodeKey { node: normalized_expression_node(index, receiver), ..key };
        let resolved = applied_call_result_layout(db, receiver, &member.target.node, ambient, enclosing);
        return Some(resolved.and_then(|(declaration, layout)| {
            let field_name = member.member.node.name.as_str();
            let index = layout
                .fields
                .iter()
                .position(|(name, _)| name.as_ref() == field_name)
                .and_then(|index| u32::try_from(index).ok())
                .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
            Ok(AggregateFieldAccess { declaration, receiver, index, layout })
        }));
    }
    let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
    let resolved = match path.path.node.segments.as_slice() {
        [receiver, field] if receiver.node.type_args.is_empty() && field.node.type_args.is_empty() => {
            applied_local_receiver_layout(db, program, index, key, receiver.node.name.node.name.as_str(), ambient).map(
                |(declaration, receiver, layout)| (declaration, receiver, layout, field.node.name.node.name.as_str()),
            )
        }
        [field] if field.node.type_args.is_empty() => applied_method_receiver_layout(db, program, index, key, ambient)
            .map(|(declaration, receiver, layout)| (declaration, receiver, layout, field.node.name.node.name.as_str())),
        _ => return None,
    };
    Some(resolved.and_then(|(declaration, receiver, layout, field_name)| {
        let index = layout
            .fields
            .iter()
            .position(|(name, _)| name.as_ref() == field_name)
            .and_then(|index| u32::try_from(index).ok())
            .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
        Ok(AggregateFieldAccess { declaration, receiver, index, layout })
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
        return Some(
            generic_source_expression_identity(db, receiver)
                .and_then(|identity| project_nominal_field(db, key, receiver, &identity, &member.member.node.name)),
        );
    }
    let (path_key, last) = if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>() {
        if path.path.node.segments.len() < 3 {
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
            return implicit_receiver_field_projection(db, program, index, expression, field_name);
        }
        (expression, position)
    } else {
        return None;
    };
    let path = &index.node_at(program, path_key.node)?.of::<beskid_analysis::syntax::PathExpression>()?.path.node;
    if path.segments[..=last].iter().any(|segment| !segment.node.type_args.is_empty()) {
        return None;
    }
    let root = resolve_lexical_declaration(program, index, path_key.node, &path.segments[0].node.name.node.name)?;
    let annotation = explicit_local_complex_type_path(program, index, root);
    // An enum-pattern binding has no written annotation; its enum match fact is the sole
    // authority for the exact applied payload identity. An unannotated `let` takes the shared
    // source-proven local identity of its initializer. Other unannotated roots stay unproven.
    let binding = match annotation {
        Some(_) => None,
        None => Some(match pattern_binding_fact(db, index, path_key, root) {
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
            (Some(annotation), _) => generic_source_type_identity(
                db,
                path_key,
                &beskid_analysis::syntax::Type::Complex(beskid_analysis::syntax::Spanned::new(
                    annotation.clone(),
                    path.segments[0].span,
                )),
            )?,
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
) -> Option<Result<(AggregateFieldAccess, GenericSourceTypeIdentity), SemanticError>> {
    let method = nearest_ancestor(index, reference.node, |kind| {
        kind == beskid_analysis::syntax_query::NodeKind::MethodDefinition
    })?;
    let declaration = parent_node(index, method)?;
    let definition = index.node_at(program, declaration)?.of::<beskid_analysis::syntax::TypeDefinition>()?;
    // A generic enclosing receiver needs its applied arguments; only the non-generic
    // spelling is proven by the definition alone.
    if !definition.generics.is_empty() {
        return None;
    }
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
        let layout =
            aggregate_layout_from_definition(db, program, index, declaration, definition, None)?;
        let identity =
            generic_source_type_identity_with_substitutions(db, declaration, &field_type, &HashMap::new())?;
        let access = AggregateFieldAccess {
            declaration,
            receiver: AstNodeKey { node: method, ..reference },
            index: field_index,
            layout,
        };
        Ok((access, identity))
    })())
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
    if declaration.unit != key.unit && field.node.visibility.node != beskid_analysis::syntax::Visibility::Public {
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
                    super::super::contracts::concrete_declaration(db, key, argument)
                        .ok_or_else(|| SemanticError::unavailable("nominal_field_projection"))?,
                )
            } else {
                AggregateFieldShape::Scalar(argument.abi_type())
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

fn applied_call_result_layout(
    db: &dyn Db,
    receiver: AstNodeKey,
    expression: &beskid_analysis::syntax::Expression,
    ambient: Option<&HashMap<String, AggregateFieldShape>>,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<(AstNodeKey, AggregateLayoutFact), SemanticError> {
    let beskid_analysis::syntax::Expression::Call(call) = expression else {
        return Err(SemanticError::unavailable("aggregate_field_access"));
    };
    let specialization = if let Some(enclosing) = enclosing {
        generic_call_specialization_in_environment(db, receiver, enclosing)?
    } else {
        Some(generic_specialization_instance_for_call(db, receiver)?)
    }
    .ok_or_else(|| SemanticError::unavailable("aggregate_field_access.call_specialization"))?;
    let declaration_syntax = db
        .syntax_unit(specialization.declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, specialization.declaration))
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access.declaration_syntax"))?;
    let declaration_node = declaration_syntax
        .syntax_index(db)
        .node_at(declaration_syntax.expanded_program(db), specialization.declaration.node)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access.declaration_node"))?;
    let function = declaration_node
        .of::<beskid_analysis::syntax::FunctionDefinition>()
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access.function"))?;
    let result =
        function.return_type.as_ref().ok_or_else(|| SemanticError::unavailable("aggregate_field_access.result"))?;
    let beskid_analysis::syntax::Expression::Path(callee) = &call.node.callee.node else {
        return Err(SemanticError::unavailable("aggregate_field_access.callee_path"));
    };
    let arguments = explicit_generic_type_argument_syntax(&callee.node.path.node)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access.type_arguments"))?;
    if function.generics.len() != arguments.len() {
        return Err(SemanticError::unavailable("aggregate_field_access.type_argument_arity"));
    }
    let substitutions = function
        .generics
        .iter()
        .zip(arguments)
        .map(|(parameter, argument)| (parameter.node.name.as_str(), argument))
        .collect::<HashMap<_, _>>();
    let result = substitute_explicit_type(&result.node, &substitutions)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access.substituted_result"))?;
    let beskid_analysis::syntax::Type::Complex(path) = &result else {
        return Err(SemanticError::unavailable("aggregate_field_access.nominal_result"));
    };
    instantiated_aggregate_layout_for_path(db, receiver, &path.node, ambient)
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
    let declaration = parent_node(index, method).ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
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
    let scrutinee_path = match &expression.scrutinee.node {
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
