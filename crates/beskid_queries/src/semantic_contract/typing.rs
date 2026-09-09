//! Focused semantic-contract implementation cluster.

use super::*;

#[salsa::tracked(persist)]
pub(super) fn managed_reference_kind_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ManagedReferenceKind> {
    with_node(db, syntax, key, |program, index, node| {
        let normalized = normalized_expression_node(index, key.node);
        if normalized != key.node {
            return Some(
                managed_reference_kind(db, AstNodeKey { node: normalized, ..key })
                    .and_then(|kind| kind.ok_or_else(|| SemanticError::unavailable("managed_reference_kind"))),
            );
        }
        if let Some(syntax_type) = node.of::<beskid_analysis::syntax::Type>() {
            return Some(managed_reference_kind_for_syntax_type(syntax_type));
        }
        if let Some(parameter) = node.of::<beskid_analysis::syntax::Parameter>() {
            if type_syntax_is_enclosing_generic_parameter_reference(db, key, &parameter.ty.node) {
                return Some(Err(SemanticError::unavailable("managed_reference_kind")));
            }
            return Some(managed_reference_kind_for_syntax_type(&parameter.ty.node));
        }
        if let Some(statement) = node.of::<beskid_analysis::syntax::LetStatement>() {
            return Some(
                statement
                    .type_annotation
                    .as_ref()
                    .ok_or_else(|| SemanticError::unavailable("managed_reference_kind"))
                    .and_then(|annotation| {
                        if type_syntax_is_enclosing_generic_parameter_reference(db, key, &annotation.node) {
                            Err(SemanticError::unavailable("managed_reference_kind"))
                        } else {
                            managed_reference_kind_for_syntax_type(&annotation.node)
                        }
                    }),
            );
        }
        if node.of::<beskid_analysis::syntax::Identifier>().is_some()
            && let Some(parent) = parent_node(index, key.node)
        {
            if let Some(parameter) =
                index.node_at(program, parent).and_then(|parent| parent.of::<beskid_analysis::syntax::Parameter>())
            {
                if type_syntax_is_enclosing_generic_parameter_reference(db, key, &parameter.ty.node) {
                    return Some(Err(SemanticError::unavailable("managed_reference_kind")));
                }
                return Some(managed_reference_kind_for_syntax_type(&parameter.ty.node));
            }
            if let Some(statement) =
                index.node_at(program, parent).and_then(|parent| parent.of::<beskid_analysis::syntax::LetStatement>())
            {
                return Some(
                    statement
                        .type_annotation
                        .as_ref()
                        .ok_or_else(|| SemanticError::unavailable("managed_reference_kind"))
                        .and_then(|annotation| {
                            if type_syntax_is_enclosing_generic_parameter_reference(db, key, &annotation.node) {
                                Err(SemanticError::unavailable("managed_reference_kind"))
                            } else {
                                managed_reference_kind_for_syntax_type(&annotation.node)
                            }
                        }),
                );
            }
        }
        if node.of::<beskid_analysis::syntax::ArrayLiteralExpression>().is_some()
            || node.of::<beskid_analysis::syntax::StructLiteralExpression>().is_some()
            || node.of::<beskid_analysis::syntax::EnumConstructorExpression>().is_some()
            || node.of::<beskid_analysis::syntax::LambdaExpression>().is_some()
        {
            return Some(Ok(ManagedReferenceKind::GcManaged));
        }
        // Module constants are source-restricted to integer literals. Resolve through the same
        // current-unit/import-closure authority as lowering rather than treating their pointer-
        // shaped path syntax as an ambiguous managed reference.
        if node.of::<beskid_analysis::syntax::PathExpression>().is_some()
            && matches!(constant_integer(db, key), Ok(Some(_)))
        {
            return Some(Ok(ManagedReferenceKind::NativeOrScalar));
        }
        if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>()
            && let [segment] = path.path.node.segments.as_slice()
            && segment.node.type_args.is_empty()
            && matches!(constant_integer(db, key), Ok(Some(_)))
        {
            return Some(Ok(ManagedReferenceKind::NativeOrScalar));
        }
        if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>()
            && let [segment] = path.path.node.segments.as_slice()
            && segment.node.type_args.is_empty()
            && let Some(declaration) =
                resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())
            && let Some(binding) = pattern_binding_fact(db, index, key, declaration)
        {
            return Some(binding.map(|binding| binding.managed_reference));
        }
        if node.of::<beskid_analysis::syntax::CallExpression>().is_some()
            && let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(db, key)
            && let Some(kind) = managed_reference_kind_for_callable_result(db, key, declaration)
        {
            return Some(kind);
        }
        if node.of::<beskid_analysis::syntax::CallExpression>().is_some()
            && let Ok(Some(CallLowering::CorelibService(service))) = call_lowering(db, key)
        {
            return Some(match corelib_service_abi_signature(service).map(|signature| signature.result) {
                Some(SemanticTypeId::STRING) => Ok(ManagedReferenceKind::GcManaged),
                Some(SemanticTypeId::POINTER) | None => Err(SemanticError::unavailable("managed_reference_kind")),
                Some(_) => Ok(ManagedReferenceKind::NativeOrScalar),
            });
        }
        if node.of::<beskid_analysis::syntax::PathExpression>().is_some()
            && let Ok(Some(access)) = aggregate_field_access(db, key)
        {
            return Some(managed_reference_kind_for_aggregate_field(db, &access));
        }
        // Source identity must win over the pointer-shaped ABI fallback. In particular, an
        // explicitly declared `pointer` local is a native pointer, while arrays, functions, and
        // nominal values deliberately share the same target ABI representation.
        if let Ok(identity) = generic_source_expression_identity(db, key) {
            return Some(Ok(identity.managed_reference_kind()));
        }
        if let Ok(Some(semantic)) = node_type(db, key) {
            return Some(match semantic {
                SemanticTypeId::STRING => Ok(ManagedReferenceKind::GcManaged),
                SemanticTypeId::POINTER => Err(SemanticError::unavailable("managed_reference_kind")),
                _ => Ok(ManagedReferenceKind::NativeOrScalar),
            });
        }
        Some(Err(SemanticError::unavailable("managed_reference_kind")))
    })?
    .transpose()
}

fn managed_reference_kind_for_aggregate_field(
    db: &dyn Db,
    access: &AggregateFieldAccess,
) -> Result<ManagedReferenceKind, SemanticError> {
    let syntax = db
        .syntax_unit(access.declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, access.declaration))
        .ok_or_else(|| SemanticError::unavailable("managed_reference_kind"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), access.declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        .ok_or_else(|| SemanticError::unavailable("managed_reference_kind"))?;
    let field = definition
        .fields
        .get(usize::try_from(access.index).map_err(|_| SemanticError::unavailable("managed_reference_kind"))?)
        .ok_or_else(|| SemanticError::unavailable("managed_reference_kind"))?;
    if generic_parameter_reference_name(&field.node.ty.node).is_some() {
        return match access.layout.fields.get(usize::try_from(access.index).unwrap_or(usize::MAX)) {
            Some((_, AggregateFieldShape::Nominal(_)))
            | Some((_, AggregateFieldShape::Scalar(SemanticTypeId::STRING))) => Ok(ManagedReferenceKind::GcManaged),
            Some((_, AggregateFieldShape::Scalar(SemanticTypeId::POINTER))) => {
                Err(SemanticError::unavailable("managed_reference_kind"))
            }
            Some((_, AggregateFieldShape::Scalar(_))) => Ok(ManagedReferenceKind::NativeOrScalar),
            None => Err(SemanticError::unavailable("managed_reference_kind")),
        };
    }
    managed_reference_kind_for_syntax_type(&field.node.ty.node)
}

pub(in crate::semantic_contract) fn managed_reference_kind_for_syntax_type(
    syntax_type: &beskid_analysis::syntax::Type,
) -> Result<ManagedReferenceKind, SemanticError> {
    Ok(match syntax_type {
        beskid_analysis::syntax::Type::Primitive(primitive)
            if primitive.node == beskid_analysis::syntax::PrimitiveType::String =>
        {
            ManagedReferenceKind::GcManaged
        }
        beskid_analysis::syntax::Type::Primitive(_) => ManagedReferenceKind::NativeOrScalar,
        beskid_analysis::syntax::Type::Array(_) | beskid_analysis::syntax::Type::Function { .. } => {
            ManagedReferenceKind::GcManaged
        }
        beskid_analysis::syntax::Type::Complex(_) => ManagedReferenceKind::GcManaged,
        beskid_analysis::syntax::Type::Associated { .. } => {
            return Err(SemanticError::unavailable("managed_reference_kind"));
        }
    })
}

fn managed_reference_kind_for_callable_result(
    db: &dyn Db,
    use_key: AstNodeKey,
    declaration: AstNodeKey,
) -> Option<Result<ManagedReferenceKind, SemanticError>> {
    let syntax = db.syntax_unit(declaration.unit)?;
    let node = syntax.syntax_index(db).node_at(syntax.expanded_program(db), declaration.node)?;
    let return_type = node
        .of::<beskid_analysis::syntax::FunctionDefinition>()
        .and_then(|function| function.return_type.as_ref())
        .or_else(|| {
            node.of::<beskid_analysis::syntax::MethodDefinition>().and_then(|method| method.return_type.as_ref())
        });
    let Some(return_type) = return_type else {
        return Some(Ok(ManagedReferenceKind::NativeOrScalar));
    };
    Some(match &return_type.node {
        beskid_analysis::syntax::Type::Primitive(_)
        | beskid_analysis::syntax::Type::Array(_)
        | beskid_analysis::syntax::Type::Function { .. } => managed_reference_kind_for_syntax_type(&return_type.node),
        beskid_analysis::syntax::Type::Complex(_) => {
            if let Some(parameter) = generic_parameter_reference_name(&return_type.node)
                && let Ok(instance) = generic_specialization_instance_for_call(db, use_key)
                && let Some(binding) =
                    instance.substitutions.iter().find(|binding| binding.parameter.as_ref() == parameter)
            {
                Ok(binding.source_identity().managed_reference_kind())
            } else {
                Ok(ManagedReferenceKind::GcManaged)
            }
        }
        beskid_analysis::syntax::Type::Associated { .. } => Err(SemanticError::unavailable("managed_reference_kind")),
    })
}

#[salsa::tracked(persist)]
pub(super) fn node_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |program, index, node| {
        if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryExpression>() {
            return Some(abi_type_for_binary_expression(db, program, index, key, binary));
        }
        if node.of::<beskid_analysis::syntax::CallExpression>().is_some() {
            match primitive_numeric_conversion(db, key) {
                Ok(Some(conversion)) => return Some(Ok(conversion.to)),
                Ok(None) => (),
                Err(error) => return Some(Err(error)),
            }
            match call_lowering(db, key) {
                Ok(Some(CallLowering::Direct(_) | CallLowering::Runtime(_))) => (),
                Ok(Some(_) | None) => return Some(Err(SemanticError::unavailable("node_type"))),
                Err(error) => return Some(Err(error)),
            };
            return Some(
                call_abi_signature(db, key)
                    .and_then(|signature| signature.ok_or_else(|| SemanticError::unavailable("node_type")))
                    .map(|signature| signature.result),
            );
        }
        if let Some(binding_type) = pattern_binding_semantic_type(db, program, index, key, node) {
            return Some(binding_type);
        }
        if node.of::<beskid_analysis::syntax::PathExpression>().is_some()
            && matches!(constant_integer(db, key), Ok(Some(_)))
        {
            return Some(Ok(SemanticTypeId::I32));
        }
        if node.of::<beskid_analysis::syntax::MatchExpression>().is_some() && matches!(enum_match(db, key), Ok(Some(_)))
        {
            return Some(enum_match_result_semantic_type(db, key));
        }
        if let Some(beskid_analysis::syntax::Expression::Call(call)) = node.of::<beskid_analysis::syntax::Expression>()
        {
            let call = index
                .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(call))
                .map(|node| AstNodeKey { node, ..key })
                .ok_or_else(|| SemanticError::unavailable("node_type"));
            return Some(call.and_then(|call| {
                call_abi_signature(db, call)?
                    .map(|signature| signature.result)
                    .ok_or_else(|| SemanticError::unavailable("node_type"))
            }));
        }
        semantic_type_for_node(program, index, key.node, node)
    })?
    .transpose()
}

pub(super) fn enum_match_result_semantic_type(db: &dyn Db, key: AstNodeKey) -> Result<SemanticTypeId, SemanticError> {
    let fact = enum_match(db, key)?.ok_or_else(|| SemanticError::unavailable("node_type"))?;
    let mut result = None;
    for arm in fact.arms.iter() {
        let contextual = match contextual_integer_literal_abi_type(db, arm.body) {
            Ok(contextual) => contextual,
            Err(error) if error.is_unavailable() => None,
            Err(error) => return Err(error),
        };
        let arm_type = match contextual {
            Some(contextual) => contextual,
            None => node_type(db, arm.body)?.ok_or_else(|| SemanticError::unavailable("node_type"))?,
        };
        result = join_match_arm_type(result, arm_type)?;
    }
    result.ok_or_else(|| SemanticError::unavailable("node_type"))
}

fn join_match_arm_type(
    current: Option<SemanticTypeId>,
    candidate: SemanticTypeId,
) -> Result<Option<SemanticTypeId>, SemanticError> {
    match current {
        None => Ok(Some(candidate)),
        Some(previous) if previous == candidate => Ok(Some(previous)),
        Some(SemanticTypeId::NEVER) => Ok(Some(candidate)),
        Some(previous) if candidate == SemanticTypeId::NEVER => Ok(Some(previous)),
        Some(_) => Err(SemanticError::unavailable("node_type")),
    }
}

pub(super) fn pattern_binding_semantic_type(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
    let [segment] = path.path.node.segments.as_slice() else {
        return None;
    };
    if !segment.node.type_args.is_empty() {
        return None;
    }
    let declaration = resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())?;
    pattern_binding_abi_type(db, index, key, declaration)
}

/// Derive the ABI representation of an enum-pattern binding from its already-resolved match
/// layout. Both ordinary expression typing and generic-call specialization consume this fact.
pub(crate) fn pattern_binding_abi_type(
    db: &dyn Db,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    let binding = match pattern_binding_fact(db, index, key, declaration)? {
        Ok(binding) => binding,
        Err(error) => return Some(Err(error)),
    };
    Some(Ok(match binding.payload {
        AggregateFieldShape::Scalar(semantic) => semantic,
        AggregateFieldShape::Nominal(_) => SemanticTypeId::POINTER,
    }))
}

pub(super) fn pattern_binding_fact(
    db: &dyn Db,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<Result<EnumMatchBindingFact, SemanticError>> {
    if index.kind(parent_node(index, declaration)?)? != beskid_analysis::syntax_query::NodeKind::Pattern {
        return None;
    }
    let arm = nearest_ancestor(index, declaration, |kind| kind == beskid_analysis::syntax_query::NodeKind::MatchArm)?;
    let outer_match =
        nearest_ancestor(index, arm, |kind| kind == beskid_analysis::syntax_query::NodeKind::MatchExpression)?;
    if outer_match == key.node {
        return None;
    }
    let outer_match = AstNodeKey { node: outer_match, ..key };
    let fact = match enum_match(db, outer_match) {
        Ok(Some(fact)) => fact,
        Ok(None) | Err(_) => return Some(Err(SemanticError::unavailable("pattern_binding"))),
    };
    fact.arms
        .iter()
        .filter_map(|arm| enum_match_pattern_binding(&arm.pattern, declaration))
        .find(|binding| binding.declaration.node == declaration)
        .map(Ok)
        .or_else(|| Some(Err(SemanticError::unavailable("pattern_binding"))))
}

fn enum_match_pattern_binding(
    pattern: &EnumMatchPatternFact,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<EnumMatchBindingFact> {
    match pattern {
        EnumMatchPatternFact::Binding(binding) if binding.declaration.node == declaration => Some(*binding),
        EnumMatchPatternFact::Enum(pattern) => {
            pattern.items.iter().find_map(|item| enum_match_pattern_binding(item, declaration))
        }
        EnumMatchPatternFact::Wildcard
        | EnumMatchPatternFact::Binding(_)
        | EnumMatchPatternFact::UnitLiteral { .. }
        | EnumMatchPatternFact::ScalarLiteral(_) => None,
    }
}

pub(super) fn semantic_type_for_node(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    if let Some(literal) = node.of::<beskid_analysis::syntax::Literal>() {
        return Some(Ok(semantic_type_for_literal(literal)));
    }
    if let Some(literal) = node.of::<beskid_analysis::syntax::LiteralExpression>() {
        return Some(Ok(semantic_type_for_literal(&literal.literal.node)));
    }
    if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>() {
        return Some(semantic_type_for_local_path(program, index, reference, &path.path.node));
    }
    if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryExpression>() {
        return Some(semantic_type_for_binary_operands(
            program,
            index,
            reference,
            &binary.left.node,
            binary.op.node,
            &binary.right.node,
        ));
    }
    if let Some(unary) = node.of::<beskid_analysis::syntax::UnaryExpression>() {
        return Some(semantic_type_for_unary_operand(program, index, reference, unary));
    }
    if let Some(match_expression) = node.of::<beskid_analysis::syntax::MatchExpression>() {
        let mut result = None;
        for arm in &match_expression.arms {
            let arm_type = match semantic_type_for_expression(program, index, reference, &arm.node.value.node) {
                Ok(arm_type) => arm_type,
                Err(error) => return Some(Err(error)),
            };
            result = match join_match_arm_type(result, arm_type) {
                Ok(result) => result,
                Err(error) => return Some(Err(error)),
            };
        }
        return result.map(Ok).or_else(|| Some(Err(SemanticError::unavailable("node_type"))));
    }
    if let Some(expression) = node.of::<beskid_analysis::syntax::Expression>() {
        return Some(semantic_type_for_expression(program, index, reference, expression));
    }
    if let Some(syntax_type) = node.of::<beskid_analysis::syntax::Type>() {
        return Some(semantic_type_from_syntax(syntax_type));
    }
    if node.of::<beskid_analysis::syntax::Identifier>().is_some() {
        return local_declaration_type(program, index, reference);
    }
    expression_fact_target(node.node_kind()).then(|| Err(SemanticError::unavailable("node_type")))
}

pub(super) fn semantic_type_for_expression(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    expression: &beskid_analysis::syntax::Expression,
) -> Result<SemanticTypeId, SemanticError> {
    match expression {
        beskid_analysis::syntax::Expression::Literal(literal) => {
            Ok(semantic_type_for_literal(&literal.node.literal.node))
        }
        beskid_analysis::syntax::Expression::Path(path) => {
            semantic_type_for_local_path(program, index, reference, &path.node.path.node)
        }
        beskid_analysis::syntax::Expression::Grouped(grouped) => {
            semantic_type_for_expression(program, index, reference, &grouped.node.expr.node)
        }
        beskid_analysis::syntax::Expression::Binary(binary) => semantic_type_for_binary_operands(
            program,
            index,
            reference,
            &binary.node.left.node,
            binary.node.op.node,
            &binary.node.right.node,
        ),
        beskid_analysis::syntax::Expression::Unary(unary) => {
            semantic_type_for_unary_operand(program, index, reference, &unary.node)
        }
        _ => Err(SemanticError::unavailable("node_type")),
    }
}

fn semantic_type_for_unary_operand(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    unary: &beskid_analysis::syntax::UnaryExpression,
) -> Result<SemanticTypeId, SemanticError> {
    let operand = semantic_type_for_expression(program, index, reference, &unary.expr.node)?;
    match unary.op.node {
        beskid_analysis::syntax::UnaryOp::Neg if primitive_numeric(operand) => Ok(operand),
        beskid_analysis::syntax::UnaryOp::Not if operand == SemanticTypeId::BOOL => Ok(operand),
        _ => Err(SemanticError::unavailable("node_type")),
    }
}

pub(super) fn semantic_type_for_binary_operands(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    left: &beskid_analysis::syntax::Expression,
    op: beskid_analysis::syntax::BinaryOp,
    right: &beskid_analysis::syntax::Expression,
) -> Result<SemanticTypeId, SemanticError> {
    let left = semantic_type_for_expression(program, index, reference, left)?;
    let right = semantic_type_for_expression(program, index, reference, right)?;
    use beskid_analysis::syntax::BinaryOp;
    match op {
        BinaryOp::Or | BinaryOp::And if left == SemanticTypeId::BOOL && right == SemanticTypeId::BOOL => {
            Ok(SemanticTypeId::BOOL)
        }
        BinaryOp::IdentityEq
        | BinaryOp::IdentityNotEq
        | BinaryOp::Eq
        | BinaryOp::NotEq
        | BinaryOp::Lt
        | BinaryOp::Lte
        | BinaryOp::Gt
        | BinaryOp::Gte
            if left == right =>
        {
            Ok(SemanticTypeId::BOOL)
        }
        BinaryOp::Add if left == SemanticTypeId::STRING || right == SemanticTypeId::STRING => {
            Ok(SemanticTypeId::STRING)
        }
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod
            if left == right && primitive_numeric(left) =>
        {
            Ok(left)
        }
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::Shl | BinaryOp::Shr
            if left == right && primitive_integer(left) =>
        {
            Ok(left)
        }
        _ => Err(SemanticError::unavailable("node_type")),
    }
}

pub(super) fn semantic_type_for_local_path(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    path: &beskid_analysis::syntax::Path,
) -> Result<SemanticTypeId, SemanticError> {
    let [segment] = path.segments.as_slice() else {
        return Err(SemanticError::unavailable("node_type"));
    };
    if !segment.node.type_args.is_empty() {
        return Err(SemanticError::unavailable("node_type"));
    }
    let declaration = resolve_lexical_declaration(program, index, reference, segment.node.name.node.name.as_str())
        .ok_or_else(|| SemanticError::unavailable("node_type"))?;
    local_declaration_type(program, index, declaration).unwrap_or_else(|| Err(SemanticError::unavailable("node_type")))
}

pub(super) fn local_declaration_type(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    let parent = parent_node(index, declaration)?;
    match index.kind(parent)? {
        beskid_analysis::syntax_query::NodeKind::Parameter => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::Parameter>()
            .map(|parameter| semantic_type_from_syntax(&parameter.ty.node)),
        beskid_analysis::syntax_query::NodeKind::LambdaParameter => {
            index.node_at(program, parent)?.of::<beskid_analysis::syntax::LambdaParameter>().map(|parameter| {
                parameter.ty.as_ref().map_or_else(
                    || Err(SemanticError::unavailable("node_type")),
                    |syntax_type| semantic_type_from_syntax(&syntax_type.node),
                )
            })
        }
        beskid_analysis::syntax_query::NodeKind::LetStatement => {
            index.node_at(program, parent)?.of::<beskid_analysis::syntax::LetStatement>().map(|statement| {
                statement.type_annotation.as_ref().map_or_else(
                    || semantic_type_for_expression(program, index, parent, &statement.value.node),
                    |syntax_type| semantic_type_from_syntax(&syntax_type.node),
                )
            })
        }
        beskid_analysis::syntax_query::NodeKind::ForStatement => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::ForStatement>()
            .map(|statement| element_type_for_for_iterable(program, index, parent, &statement.iterable.node)),
        _ => None,
    }
}

pub(super) fn element_type_for_for_iterable(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    for_statement: beskid_analysis::syntax::AstNodeId,
    iterable: &beskid_analysis::syntax::Expression,
) -> Result<SemanticTypeId, SemanticError> {
    let beskid_analysis::syntax::Expression::Call(call) = iterable else {
        return Err(SemanticError::unavailable("for_iterator_element_type"));
    };
    let beskid_analysis::syntax::Expression::Path(path) = &call.node.callee.node else {
        return Err(SemanticError::unavailable("for_iterator_element_type"));
    };
    let [segment] = path.node.path.node.segments.as_slice() else {
        return Err(SemanticError::unavailable("for_iterator_element_type"));
    };
    if segment.node.name.node.name != "range" || !segment.node.type_args.is_empty() {
        return Err(SemanticError::unavailable("for_iterator_element_type"));
    }
    let [start, _end] = call.node.args.as_slice() else {
        return Err(SemanticError::unavailable("for_iterator_element_type"));
    };
    semantic_type_for_expression(program, index, for_statement, &start.node)
}

pub(super) fn semantic_type_for_literal(literal: &beskid_analysis::syntax::Literal) -> SemanticTypeId {
    match literal {
        beskid_analysis::syntax::Literal::Integer(value) if value.ends_with("_i32") => SemanticTypeId::I32,
        beskid_analysis::syntax::Literal::Integer(value) if value.ends_with("_i64") => SemanticTypeId::I64,
        beskid_analysis::syntax::Literal::Integer(value) if value.ends_with("_u32") => SemanticTypeId::U32,
        beskid_analysis::syntax::Literal::Integer(value) if value.ends_with("_u8") => SemanticTypeId::U8,
        beskid_analysis::syntax::Literal::Integer(value)
            if value.starts_with("0x") && integer_literal_u64(value).is_some_and(|number| number > i64::MAX as u64) =>
        {
            SemanticTypeId::WORD
        }
        beskid_analysis::syntax::Literal::Integer(_) => SemanticTypeId::I32,
        beskid_analysis::syntax::Literal::Float(_) => SemanticTypeId::F64,
        beskid_analysis::syntax::Literal::String(_) => SemanticTypeId::STRING,
        beskid_analysis::syntax::Literal::Char(_) => SemanticTypeId::CHAR,
        beskid_analysis::syntax::Literal::Bool(_) => SemanticTypeId::BOOL,
        beskid_analysis::syntax::Literal::Unit => SemanticTypeId::UNIT,
    }
}
