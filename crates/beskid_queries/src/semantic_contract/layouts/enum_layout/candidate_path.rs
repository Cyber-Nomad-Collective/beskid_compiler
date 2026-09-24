//! Contextual enum constructor type paths, instantiated layouts, and field layouts.

use super::super::super::*;
use super::super::explicit_local_declaration_type;
use super::*;
use crate::semantic_contract::typing::{managed_reference_kind_for_syntax_type, pattern_binding_fact_in_environment};

/// Return an explicitly applied enum type from a genericless constructor's proven value context.
///
/// The walk follows only transparent expression/match wrappers plus the final expression statement
/// of a block expression. It intentionally declines inferred values, non-final block statements,
/// local initializers, lambdas, and other nested control-flow contexts.
pub(in crate::semantic_contract) fn contextual_enum_constructor_type_path(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    constructor: &beskid_analysis::syntax::EnumConstructorExpression,
) -> Option<beskid_analysis::syntax::Path> {
    let constructor_path = &constructor.path.node.type_path.node;
    let terminal = constructor_path.segments.last()?;
    if !terminal.node.type_args.is_empty() {
        return None;
    }
    let mut current = parent_node(index, key.node)?;
    let mut value_child = key.node;
    let mut call_argument_root = key.node;
    loop {
        use beskid_analysis::syntax_query::NodeKind;

        match index.kind(current)? {
            NodeKind::Expression | NodeKind::Statement => {
                value_child = current;
                current = parent_node(index, current)?;
            }
            NodeKind::MatchArm => {
                let arm = index.node_at(program, current)?.of::<beskid_analysis::syntax::MatchArm>()?;
                let body = index.direct_child_id(
                    program,
                    current,
                    beskid_analysis::syntax_query::DynNodeRef::from(&arm.value),
                )?;
                (body == value_child).then_some(())?;
                value_child = current;
                current = parent_node(index, current)?;
            }
            NodeKind::MatchExpression => {
                (index.kind(value_child)? == NodeKind::MatchArm).then_some(())?;
                value_child = current;
                current = parent_node(index, current)?;
            }
            NodeKind::ExpressionStatement => {
                let statement_wrapper = parent_node(index, current)?;
                (index.kind(statement_wrapper)? == NodeKind::Statement).then_some(())?;
                let block = parent_node(index, statement_wrapper)?;
                (index.kind(block)? == NodeKind::Block).then_some(())?;
                let statements = block_statement_nodes(db, AstNodeKey { node: block, ..key }).ok()??;
                (statements.last().is_some_and(|statement| statement.node == current)).then_some(())?;
                let block_expression = parent_node(index, block)?;
                (index.kind(block_expression)? == NodeKind::BlockExpression).then_some(())?;
                call_argument_root = block_expression;
                value_child = block_expression;
                current = parent_node(index, block_expression)?;
            }
            _ => break,
        }
    }

    let expected = match index.kind(current)? {
        beskid_analysis::syntax_query::NodeKind::LetStatement => index
            .node_at(program, current)?
            .of::<beskid_analysis::syntax::LetStatement>()?
            .type_annotation
            .as_ref()
            .map(|annotation| &annotation.node),
        beskid_analysis::syntax_query::NodeKind::ReturnStatement => {
            let mut item = parent_node(index, current)?;
            while !matches!(
                index.kind(item)?,
                beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                    | beskid_analysis::syntax_query::NodeKind::MethodDefinition
            ) {
                item = parent_node(index, item)?;
            }
            let item = index.node_at(program, item)?;
            item.of::<beskid_analysis::syntax::FunctionDefinition>()
                .and_then(|function| function.return_type.as_ref().map(|annotation| &annotation.node))
                .or_else(|| {
                    item.of::<beskid_analysis::syntax::MethodDefinition>()
                        .and_then(|method| method.return_type.as_ref().map(|annotation| &annotation.node))
                })
        }
        beskid_analysis::syntax_query::NodeKind::AssignExpression => {
            let assignment = mutable_local_assignment(db, AstNodeKey { node: current, ..key }).ok().flatten()?;
            explicit_local_declaration_type(program, index, assignment.declaration.node)
        }
        beskid_analysis::syntax_query::NodeKind::CallExpression => {
            return expected_explicit_call_argument_type(
                db,
                program,
                index,
                AstNodeKey { node: call_argument_root, ..key },
            )
            .and_then(|expected| {
                let beskid_analysis::syntax::Type::Complex(path) = expected else { return None };
                contextual_enum_candidate_type_path(db, key, constructor_path, &path.node)
            });
        }
        _ => None,
    }?;
    let beskid_analysis::syntax::Type::Complex(path) = expected else {
        return None;
    };
    let expected_path = &path.node;
    contextual_enum_candidate_type_path(db, key, constructor_path, expected_path)
}

/// Admit a contextual generic enum application only when it resolves to the same nominal
/// declaration as the constructor. Matching the terminal spelling alone would let
/// `A.Result::Ok` inherit `B.Result<T, E>` arguments.
///
/// One unqualified constructor whose own name resolves to no declaration in this unit is the
/// single exception: there is no other declaration it could name, so the proven value context
/// is its only possible application. The spelling must still match the candidate's terminal
/// name, and any constructor that does resolve keeps the exact declaration comparison.
fn contextual_enum_candidate_type_path(
    db: &dyn Db,
    key: AstNodeKey,
    constructor_path: &beskid_analysis::syntax::Path,
    candidate_path: &beskid_analysis::syntax::Path,
) -> Option<beskid_analysis::syntax::Path> {
    let candidate_terminal = candidate_path.segments.last()?;
    (!candidate_terminal.node.type_args.is_empty()).then_some(())?;
    // Constructors written without type arguments cannot resolve a generic declaration by arity.
    // Apply the candidate's explicit arguments only to resolve the constructor's own qualified
    // path, then compare declarations before retaining the candidate application.
    let mut constructor_application = constructor_path.clone();
    constructor_application.segments.last_mut()?.node.type_args = candidate_terminal.node.type_args.clone();
    let candidate = resolve_type_declaration(db, key, candidate_path)?;
    let Some(constructor) = resolve_type_declaration(db, key, &constructor_application) else {
        let [segment] = constructor_path.segments.as_slice() else {
            return None;
        };
        return (segment.node.name.node.name == candidate_terminal.node.name.node.name).then(|| candidate_path.clone());
    };
    (constructor == candidate).then_some(candidate_path.clone())
}

pub(in crate::semantic_contract) fn instantiated_enum_layout_for_path(
    db: &dyn Db,
    use_key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Result<EnumLayoutFact, SemanticError> {
    let declaration =
        resolve_type_declaration(db, use_key, path).ok_or_else(|| SemanticError::unavailable("enum_layout"))?;
    let syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.generation(db) == declaration.generation)
        .ok_or_else(|| SemanticError::unavailable("enum_layout"))?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let definition = index
        .node_at(program, declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .ok_or_else(|| SemanticError::unavailable("enum_layout"))?;
    if definition.generics.is_empty() {
        return enum_layout_from_definition(db, program, index, declaration, definition, None);
    }
    let substitutions = enum_layout_substitutions(db, use_key, definition, path)?;
    enum_layout_from_definition(db, program, index, declaration, definition, Some(&substitutions))
}

pub(in crate::semantic_contract) fn enum_layout_substitutions(
    db: &dyn Db,
    use_key: AstNodeKey,
    definition: &beskid_analysis::syntax::EnumDefinition,
    path: &beskid_analysis::syntax::Path,
) -> Result<HashMap<String, AggregateFieldShape>, SemanticError> {
    let (terminal, module_path) =
        path.segments.split_last().ok_or_else(|| SemanticError::unavailable("enum_layout"))?;
    if module_path.iter().any(|segment| !segment.node.type_args.is_empty())
        || terminal.node.type_args.len() != definition.generics.len()
        || definition.generics.is_empty()
    {
        return Err(SemanticError::unavailable("enum_layout"));
    }
    definition
        .generics
        .iter()
        .zip(terminal.node.type_args.iter())
        .map(|(generic, argument)| {
            aggregate_shape_from_applied_type(db, use_key, &argument.node)
                .or_else(|error| {
                    if type_syntax_is_generic_parameter_reference(&argument.node, generic.node.name.as_str()) {
                        return Ok(AggregateFieldShape::Scalar(SemanticTypeId::POINTER));
                    }
                    Err(error)
                })
                .map(|shape| (generic.node.name.clone(), shape))
        })
        .collect()
}

pub(in crate::semantic_contract) fn aggregate_shape_from_applied_type(
    db: &dyn Db,
    use_key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
) -> Result<AggregateFieldShape, SemanticError> {
    match syntax_type {
        beskid_analysis::syntax::Type::Primitive(_) => {
            Ok(AggregateFieldShape::Scalar(semantic_type_from_syntax(syntax_type)?))
        }
        beskid_analysis::syntax::Type::Complex(path) => resolve_type_declaration(db, use_key, &path.node)
            .map(AggregateFieldShape::Nominal)
            .ok_or_else(|| SemanticError::unavailable("enum_layout")),
        beskid_analysis::syntax::Type::Associated { .. } => Err(SemanticError::unavailable("enum_layout")),
        beskid_analysis::syntax::Type::This => Err(SemanticError::unavailable("enum_layout")),
        beskid_analysis::syntax::Type::Array(_) => Ok(AggregateFieldShape::Scalar(SemanticTypeId::POINTER)),
        beskid_analysis::syntax::Type::Function { .. } => Err(SemanticError::unavailable("enum_layout")),
    }
}

pub(in crate::semantic_contract) fn enum_layout_from_definition(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: AstNodeKey,
    definition: &beskid_analysis::syntax::EnumDefinition,
    substitutions: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<EnumLayoutFact, SemanticError> {
    if definition.generics.is_empty() != substitutions.is_none() {
        return Err(SemanticError::unavailable("enum_layout"));
    }
    definition
        .variants
        .iter()
        .map(|variant| {
            variant
                .node
                .fields
                .iter()
                .map(|field| enum_field_layout(db, program, index, declaration, field, substitutions))
                .collect::<Result<Vec<_>, SemanticError>>()
                .map(|fields| EnumVariantLayoutFact {
                    name: Arc::from(variant.node.name.node.name.as_str()),
                    fields: fields.into(),
                })
        })
        .collect::<Result<Vec<_>, SemanticError>>()
        .map(|variants| EnumLayoutFact { variants: variants.into() })
}

pub(in crate::semantic_contract) fn enum_field_layout(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: AstNodeKey,
    field: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Field>,
    substitutions: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<(Arc<str>, AggregateFieldShape), SemanticError> {
    if field.node.kind != beskid_analysis::syntax::FieldKind::Value {
        return Err(SemanticError::unavailable("enum_layout"));
    }
    let substituted = match (&field.node.ty.node, substitutions) {
        (beskid_analysis::syntax::Type::Complex(path), Some(substitutions)) => {
            let [segment] = path.node.segments.as_slice() else {
                return Err(SemanticError::unavailable("enum_layout"));
            };
            segment
                .node
                .type_args
                .is_empty()
                .then(|| substitutions.get(segment.node.name.node.name.as_str()).copied())
                .flatten()
        }
        _ => None,
    };
    substituted
        .map(|shape| (Arc::from(field.node.name.node.name.as_str()), shape))
        .map(Ok)
        .unwrap_or_else(|| aggregate_field_layout(db, program, index, declaration, field))
}
