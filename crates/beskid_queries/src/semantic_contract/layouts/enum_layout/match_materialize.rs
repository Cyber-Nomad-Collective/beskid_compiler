//! Enum match facts and match-pattern materialization.

use super::super::super::*;
use super::super::explicit_local_declaration_type;
use super::*;
use crate::semantic_contract::typing::{managed_reference_kind_for_syntax_type, pattern_binding_fact_in_environment};

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn enum_match_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<EnumMatchFact> {
    with_node(db, syntax, key, |program, index, node| {
        let expression = node.of::<beskid_analysis::syntax::MatchExpression>()?;
        let (declaration, layout) = match enum_match_scrutinee_layout(db, program, index, key, expression) {
            Some(Ok(fact)) => fact,
            Some(Err(error)) => return Some(Err(error)),
            None => return Some(Err(SemanticError::unavailable("enum_match"))),
        };
        let environment = match enum_match_source_environment(db, program, index, key, expression, declaration, None) {
            Ok(environment) => environment,
            Err(error) => return Some(Err(error)),
        };
        let context = EnumMatchMaterializer { db, program, index, key, environment: &environment };
        Some(materialize_enum_match(&context, expression, declaration, layout))
    })?
    .transpose()
}

/// Materialize a generic match through the immutable specialization of its enclosing item.
///
/// The ordinary `enum_match` fact remains target-neutral and unavailable when a generic payload's
/// ownership cannot be proven. This explicit path applies the call-derived substitutions before
/// constructing layouts or bindings, so pointer-shaped nominal/native identities are never guessed.
pub fn enum_match_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<EnumMatchFact> {
    let syntax = db
        .syntax_unit(key.unit)
        .filter(|syntax| syntax.accepts_key(db, key))
        .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let expression = index
        .node_at(program, key.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::MatchExpression>())
        .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    let (declaration, layout) =
        enum_match_scrutinee_layout_in_environment(db, program, index, key, expression, &enclosing)?
            .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    let environment =
        enum_match_source_environment(db, program, index, key, expression, declaration, Some(&enclosing))?;
    let context = EnumMatchMaterializer { db, program, index, key, environment: &environment };
    materialize_enum_match(&context, expression, declaration, layout).map(Some)
}

struct EnumMatchMaterializer<'a> {
    db: &'a dyn Db,
    program: &'a beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &'a beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    environment: &'a HashMap<String, GenericSourceTypeIdentity>,
}

fn materialize_enum_match(
    context: &EnumMatchMaterializer<'_>,
    expression: &beskid_analysis::syntax::MatchExpression,
    declaration: AstNodeKey,
    layout: EnumLayoutFact,
) -> Result<EnumMatchFact, SemanticError> {
    let mut arms = Vec::with_capacity(expression.arms.len());
    for arm in &expression.arms {
        if arm.node.guard.is_some() {
            return Err(SemanticError::unavailable("enum_match"));
        }
        let arm_node = context
            .index
            .direct_child_id(context.program, context.key.node, beskid_analysis::syntax_query::DynNodeRef::from(arm))
            .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
        let body = context
            .index
            .direct_child_id(
                context.program,
                arm_node,
                beskid_analysis::syntax_query::DynNodeRef::from(&arm.node.value),
            )
            .map(|body| AstNodeKey { node: normalized_expression_node(context.index, body), ..context.key })
            .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
        let pattern_node = context
            .index
            .direct_child_id(
                context.program,
                arm_node,
                beskid_analysis::syntax_query::DynNodeRef::from(&arm.node.pattern),
            )
            .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
        let pattern = materialize_match_pattern(
            context,
            pattern_node,
            &arm.node.pattern,
            MatchPatternExpectation::NominalEnum { declaration, layout: layout.clone() },
            None,
        )?;
        arms.push(EnumMatchArmFact { pattern, body });
    }
    Ok(EnumMatchFact { declaration, layout, arms: arms.into() })
}

#[derive(Clone)]
enum MatchPatternExpectation {
    Scalar { semantic_type: SemanticTypeId, managed_reference: ManagedReferenceKind },
    Nominal(AstNodeKey),
    NominalEnum { declaration: AstNodeKey, layout: EnumLayoutFact },
}

impl MatchPatternExpectation {
    fn binding_shape(&self) -> AggregateFieldShape {
        match self {
            Self::Scalar { semantic_type, .. } => AggregateFieldShape::Scalar(*semantic_type),
            Self::Nominal(declaration) | Self::NominalEnum { declaration, .. } => {
                AggregateFieldShape::Nominal(*declaration)
            }
        }
    }

    fn managed_reference(&self) -> ManagedReferenceKind {
        match self {
            Self::Scalar { managed_reference, .. } => *managed_reference,
            Self::Nominal(_) | Self::NominalEnum { .. } => ManagedReferenceKind::GcManaged,
        }
    }
}

fn materialize_match_pattern(
    context: &EnumMatchMaterializer<'_>,
    pattern_node: beskid_analysis::syntax::AstNodeId,
    pattern: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Pattern>,
    expected: MatchPatternExpectation,
    source_identity: Option<GenericSourceTypeIdentity>,
) -> Result<EnumMatchPatternFact, SemanticError> {
    match &pattern.node {
        beskid_analysis::syntax::Pattern::Wildcard => Ok(EnumMatchPatternFact::Wildcard),
        beskid_analysis::syntax::Pattern::Identifier(identifier) => {
            if matches!(expected, MatchPatternExpectation::Scalar { semantic_type: SemanticTypeId::UNIT, .. }) {
                return Err(SemanticError::unavailable("enum_match"));
            }
            let declaration = context
                .index
                .direct_child_id(
                    context.program,
                    pattern_node,
                    beskid_analysis::syntax_query::DynNodeRef::from(identifier),
                )
                .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
            Ok(EnumMatchPatternFact::Binding(EnumMatchBindingFact {
                declaration: AstNodeKey { node: declaration, ..context.key },
                payload: expected.binding_shape(),
                managed_reference: expected.managed_reference(),
                source_identity,
            }))
        }
        beskid_analysis::syntax::Pattern::Literal(literal) => {
            let literal_node = context
                .index
                .direct_child_id(
                    context.program,
                    pattern_node,
                    beskid_analysis::syntax_query::DynNodeRef::from(literal),
                )
                .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
            let literal_key = AstNodeKey { node: literal_node, ..context.key };
            match &literal.node {
                beskid_analysis::syntax::Literal::Unit
                    if matches!(
                        expected,
                        MatchPatternExpectation::Scalar { semantic_type: SemanticTypeId::UNIT, .. }
                    ) =>
                {
                    Ok(EnumMatchPatternFact::UnitLiteral { literal: literal_key })
                }
                beskid_analysis::syntax::Literal::Integer(value) => materialize_scalar_literal(
                    literal_key,
                    &expected,
                    semantic_type_for_literal(&literal.node),
                    LiteralFact::Integer(Arc::from(value.as_str())),
                ),
                beskid_analysis::syntax::Literal::Bool(value) => {
                    materialize_scalar_literal(literal_key, &expected, SemanticTypeId::BOOL, LiteralFact::Bool(*value))
                }
                beskid_analysis::syntax::Literal::Char(value) => materialize_scalar_literal(
                    literal_key,
                    &expected,
                    SemanticTypeId::CHAR,
                    LiteralFact::Char(Arc::from(value.as_str())),
                ),
                beskid_analysis::syntax::Literal::Float(_)
                | beskid_analysis::syntax::Literal::String(_)
                | beskid_analysis::syntax::Literal::Unit => Err(SemanticError::unavailable("enum_match")),
            }
        }
        beskid_analysis::syntax::Pattern::Enum(enum_pattern) => {
            let (declaration, layout) = match expected {
                MatchPatternExpectation::NominalEnum { declaration, layout } => (declaration, layout),
                MatchPatternExpectation::Nominal(declaration) => {
                    let layout = enum_layout(context.db, declaration)?
                        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
                    (declaration, layout)
                }
                MatchPatternExpectation::Scalar { .. } => return Err(SemanticError::unavailable("enum_match")),
            };
            if let Some(GenericSourceTypeIdentity::Nominal { arguments, .. }) = source_identity {
                let syntax =
                    context.db.syntax_unit(declaration.unit).ok_or_else(|| SemanticError::unavailable("enum_match"))?;
                let definition = syntax
                    .syntax_index(context.db)
                    .node_at(syntax.expanded_program(context.db), declaration.node)
                    .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
                    .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
                if definition.generics.len() != arguments.len() {
                    return Err(SemanticError::unavailable("enum_match"));
                }
                let environment = definition
                    .generics
                    .iter()
                    .zip(arguments.iter().cloned())
                    .map(|(parameter, argument)| (parameter.node.name.clone(), argument))
                    .collect();
                let nested = EnumMatchMaterializer { environment: &environment, ..*context };
                materialize_enum_pattern(&nested, pattern_node, enum_pattern, (declaration, layout))
            } else {
                materialize_enum_pattern(context, pattern_node, enum_pattern, (declaration, layout))
            }
        }
    }
}

fn materialize_scalar_literal(
    literal: AstNodeKey,
    expected: &MatchPatternExpectation,
    semantic_type: SemanticTypeId,
    value: LiteralFact,
) -> Result<EnumMatchPatternFact, SemanticError> {
    if !matches!(expected, MatchPatternExpectation::Scalar { semantic_type: expected, .. } if *expected == semantic_type)
    {
        return Err(SemanticError::unavailable("enum_match"));
    }
    Ok(EnumMatchPatternFact::ScalarLiteral(EnumMatchScalarLiteralFact { literal, semantic_type, value }))
}

fn materialize_enum_pattern(
    context: &EnumMatchMaterializer<'_>,
    pattern_node: beskid_analysis::syntax::AstNodeId,
    pattern: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::EnumPattern>,
    applied_enum: (AstNodeKey, EnumLayoutFact),
) -> Result<EnumMatchPatternFact, SemanticError> {
    let (declaration, layout) = applied_enum;
    if !enum_pattern_targets_declaration(context.db, declaration, &pattern.node.path.node.type_path.node) {
        return Err(SemanticError::unavailable("enum_match"));
    }
    let name = pattern.node.path.node.variant.node.name.as_str();
    let (variant_index, variant) = layout
        .variants
        .iter()
        .enumerate()
        .find(|(_, variant)| variant.name.as_ref() == name)
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    if variant.fields.len() != pattern.node.items.len() {
        return Err(SemanticError::unavailable("enum_match"));
    }
    let enum_pattern_node = context
        .index
        .direct_child_id(context.program, pattern_node, beskid_analysis::syntax_query::DynNodeRef::from(pattern))
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    let items = pattern
        .node
        .items
        .iter()
        .zip(variant.fields.iter())
        .enumerate()
        .map(|(field_index, (item, (_, shape)))| {
            let item_node = context
                .index
                .direct_child_id(
                    context.program,
                    enum_pattern_node,
                    beskid_analysis::syntax_query::DynNodeRef::from(item),
                )
                .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
            let expected = match shape {
                AggregateFieldShape::Scalar(semantic_type) => MatchPatternExpectation::Scalar {
                    semantic_type: *semantic_type,
                    managed_reference: enum_variant_field_managed_reference(
                        context.db,
                        declaration,
                        variant_index,
                        field_index,
                        *shape,
                        context.environment,
                    )?,
                },
                AggregateFieldShape::Nominal(declaration) => MatchPatternExpectation::Nominal(*declaration),
            };
            let syntax =
                context.db.syntax_unit(declaration.unit).ok_or_else(|| SemanticError::unavailable("enum_match"))?;
            let definition = syntax
                .syntax_index(context.db)
                .node_at(syntax.expanded_program(context.db), declaration.node)
                .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
                .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
            let field = definition
                .variants
                .get(variant_index)
                .and_then(|variant| variant.node.fields.get(field_index))
                .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
            let substitutions = context.environment.iter().map(|(name, identity)| (name.as_str(), identity)).collect();
            let identity = generic_source_type_identity_with_substitutions(
                context.db,
                declaration,
                &field.node.ty.node,
                &substitutions,
            )
            .ok();
            materialize_match_pattern(context, item_node, item, expected, identity)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EnumMatchPatternFact::Enum(EnumMatchVariantPatternFact {
        declaration,
        layout,
        variant_index: u32::try_from(variant_index).map_err(|_| SemanticError::unavailable("enum_match"))?,
        items: items.into(),
    }))
}

fn enum_variant_field_managed_reference(
    db: &dyn Db,
    declaration: AstNodeKey,
    variant_index: usize,
    field_index: usize,
    applied_shape: AggregateFieldShape,
    environment: &HashMap<String, GenericSourceTypeIdentity>,
) -> Result<ManagedReferenceKind, SemanticError> {
    let syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    let field = definition
        .variants
        .get(variant_index)
        .and_then(|variant| variant.node.fields.get(field_index))
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;

    if let Some(parameter) = generic_parameter_reference_name(&field.node.ty.node) {
        if let Some(managed_reference) = environment.get(parameter) {
            return Ok(managed_reference.managed_reference_kind());
        }
        return match applied_shape {
            AggregateFieldShape::Nominal(_) | AggregateFieldShape::Scalar(SemanticTypeId::STRING) => {
                Ok(ManagedReferenceKind::GcManaged)
            }
            AggregateFieldShape::Scalar(SemanticTypeId::POINTER) => Err(SemanticError::unavailable("enum_match")),
            AggregateFieldShape::Scalar(_) => Ok(ManagedReferenceKind::NativeOrScalar),
        };
    }
    managed_reference_kind_for_syntax_type(&field.node.ty.node)
}
