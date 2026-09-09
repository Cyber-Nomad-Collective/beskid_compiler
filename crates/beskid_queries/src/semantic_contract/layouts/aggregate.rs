//! Canonical semantic layout implementation.

use super::super::*;

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn aggregate_layout_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AggregateLayoutFact> {
    with_node(db, syntax, key, |program, index, node| {
        let definition = node.of::<beskid_analysis::syntax::TypeDefinition>()?;
        Some(aggregate_layout_from_definition(db, program, index, key, definition, None))
    })?
    .transpose()
}

pub(in crate::semantic_contract) fn aggregate_layout_from_definition(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: AstNodeKey,
    definition: &beskid_analysis::syntax::TypeDefinition,
    substitutions: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<AggregateLayoutFact, SemanticError> {
    if definition.generics.is_empty() && substitutions.is_some() {
        return Err(SemanticError::unavailable("aggregate_layout"));
    }
    definition
        .fields
        .iter()
        // Events are dispatch metadata, not ABI-v5 aggregate storage. Keeping them
        // out of this value-field layout preserves the exact physical indices used by
        // struct literals and direct value projections; a projection of an event itself
        // still has no aggregate-field fact and therefore fails closed.
        .filter(|field| field.node.kind == beskid_analysis::syntax::FieldKind::Value)
        .map(|field| {
            let substituted = substitutions.and_then(|substitutions| {
                generic_parameter_reference_name(&field.node.ty.node)
                    .and_then(|parameter| substitutions.get(parameter))
                    .copied()
            });
            substituted
                .map(|shape| (Arc::from(field.node.name.node.name.as_str()), shape))
                .map(Ok)
                .unwrap_or_else(|| aggregate_field_layout(db, program, index, declaration, field))
        })
        .collect::<Result<Vec<_>, SemanticError>>()
        .map(|fields| AggregateLayoutFact { fields: fields.into() })
}

pub(in crate::semantic_contract) fn instantiated_aggregate_layout_for_path(
    db: &dyn Db,
    use_key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
    ambient: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<(AstNodeKey, AggregateLayoutFact), SemanticError> {
    let declaration =
        resolve_type_declaration(db, use_key, path).ok_or_else(|| SemanticError::unavailable("aggregate_layout"))?;
    let syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .ok_or_else(|| SemanticError::unavailable("aggregate_layout"))?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let definition = index
        .node_at(program, declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        .ok_or_else(|| SemanticError::unavailable("aggregate_layout"))?;
    if definition.generics.is_empty() {
        return aggregate_layout_from_definition(db, program, index, declaration, definition, None)
            .map(|layout| (declaration, layout));
    }
    let terminal = path.segments.last().ok_or_else(|| SemanticError::unavailable("aggregate_layout"))?;
    if path.segments[..path.segments.len() - 1].iter().any(|segment| !segment.node.type_args.is_empty())
        || terminal.node.type_args.len() != definition.generics.len()
    {
        return Err(SemanticError::unavailable("aggregate_layout"));
    }
    // Materialize only substitutions that can change this declaration's physical field layout.
    // A source-proven enclosing generic remains part of the nominal application identity even
    // when the declaration is phantom over it, but it must not make an otherwise concrete layout
    // unavailable. Generic parameters used directly as fields still require an exact shape.
    let layout_parameters = definition
        .fields
        .iter()
        .filter_map(|field| generic_parameter_reference_name(&field.node.ty.node))
        .collect::<HashSet<_>>();
    let substitutions = definition
        .generics
        .iter()
        .zip(terminal.node.type_args.iter())
        .filter_map(|(generic, argument)| {
            let parameter = generic.node.name.as_str();
            match applied_aggregate_shape(db, use_key, &argument.node, ambient) {
                Ok(shape) => Some(Ok((generic.node.name.clone(), shape))),
                Err(_)
                    if !layout_parameters.contains(parameter)
                        && type_syntax_is_enclosing_generic_parameter_reference(db, use_key, &argument.node) =>
                {
                    None
                }
                Err(error) => Some(Err(error)),
            }
        })
        .collect::<Result<HashMap<_, _>, SemanticError>>()?;
    aggregate_layout_from_definition(db, program, index, declaration, definition, Some(&substitutions))
        .map(|layout| (declaration, layout))
}

pub(in crate::semantic_contract) fn applied_aggregate_shape(
    db: &dyn Db,
    use_key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
    ambient: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<AggregateFieldShape, SemanticError> {
    if let Some(parameter) = generic_parameter_reference_name(syntax_type)
        && let Some(shape) = ambient.and_then(|substitutions| substitutions.get(parameter))
    {
        return Ok(*shape);
    }
    aggregate_shape_from_applied_type(db, use_key, syntax_type)
        .map_err(|_| SemanticError::unavailable("aggregate_layout"))
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn aggregate_literal_layout_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AggregateLayoutFact> {
    with_node(db, syntax, key, |_, _, node| {
        let literal = node.of::<beskid_analysis::syntax::StructLiteralExpression>()?;
        Some(instantiated_aggregate_layout_for_path(db, key, &literal.path.node, None).map(|(_, layout)| layout))
    })?
    .transpose()
}

pub fn aggregate_literal_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<AggregateLayoutFact> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return Ok(None);
    };
    with_node(db, syntax, key, |_, _, node| {
        let literal = node.of::<beskid_analysis::syntax::StructLiteralExpression>()?;
        let ambient = enclosing
            .iter()
            .map(|binding| (binding.parameter.to_string(), AggregateFieldShape::Scalar(binding.argument)))
            .collect::<HashMap<_, _>>();
        Some(
            instantiated_aggregate_layout_for_path(db, key, &literal.path.node, Some(&ambient))
                .map(|(_, layout)| layout),
        )
    })?
    .transpose()
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn aggregate_literal_declaration_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AstNodeKey> {
    with_node(db, syntax, key, |program, index, node| {
        node.of::<beskid_analysis::syntax::StructLiteralExpression>()
            .and_then(|literal| resolve_nominal_layout_declaration(db, program, index, key, &literal.path.node))
    })
}

/// Derive the element ABI of an empty array literal only from its direct nominal aggregate-field
/// context.  An empty literal carries no element expression from which to infer a representation,
/// so standalone, local-inferred, nested, and mismatched-field uses remain unavailable.  The
/// enclosing aggregate declaration and its exact declared `T[]` field are the sole authority.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn empty_array_literal_element_abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |program, index, node| {
        let array = node.of::<beskid_analysis::syntax::ArrayLiteralExpression>()?;
        if !array.elements.is_empty() {
            return None;
        }

        // The AST preserves the direct `StructLiteralField -> Expression -> []` ownership chain.
        // Do not walk arbitrary ancestors: that would turn contextual syntax into inference.
        let expression = parent_node(index, key.node)?;
        if index.kind(expression) != Some(beskid_analysis::syntax_query::NodeKind::Expression) {
            return None;
        }
        let field_node = parent_node(index, expression)?;
        let literal_node = parent_node(index, field_node)?;
        let field = index.node_at(program, field_node)?.of::<beskid_analysis::syntax::StructLiteralField>()?;
        if index.kind(literal_node) != Some(beskid_analysis::syntax_query::NodeKind::StructLiteralExpression) {
            return None;
        }

        let literal = AstNodeKey { node: literal_node, ..key };
        let declaration = aggregate_literal_declaration(db, literal).ok().flatten()?;
        let declaration_syntax =
            db.syntax_unit(declaration.unit).filter(|unit| unit.generation(db) == declaration.generation)?;
        let definition = declaration_syntax
            .syntax_index(db)
            .node_at(declaration_syntax.expanded_program(db), declaration.node)?
            .of::<beskid_analysis::syntax::TypeDefinition>()?;
        let declared_field = definition.fields.iter().find(|candidate| {
            candidate.node.kind == beskid_analysis::syntax::FieldKind::Value
                && candidate.node.name.node.name == field.name.node.name
        })?;
        let beskid_analysis::syntax::Type::Array(element) = &declared_field.node.ty.node else {
            return Some(Err(SemanticError::unavailable("empty_array_literal_element_abi_type")));
        };
        Some(abi_type_from_syntax(db, declaration, &element.node))
    })?
    .transpose()
}

/// Resolve an empty array literal that is the direct return value of a specialized generic item.
///
/// The declared `T[]` return type supplies the parameter name and the immutable enclosing
/// specialization supplies its concrete ABI. Other empty-literal contexts remain unavailable.
pub fn empty_array_literal_element_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<SemanticTypeId> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return Ok(None);
    };
    with_node(db, syntax, key, |program, index, node| {
        let array = node.of::<beskid_analysis::syntax::ArrayLiteralExpression>()?;
        if !array.elements.is_empty() {
            return None;
        }
        let expression = parent_node(index, key.node)?;
        if index.kind(expression) != Some(beskid_analysis::syntax_query::NodeKind::Expression) {
            return None;
        }
        let return_node = parent_node(index, expression)?;
        let statement = index.node_at(program, return_node)?.of::<beskid_analysis::syntax::ReturnStatement>()?;
        let returned = statement.value.as_ref().and_then(|value| {
            index.direct_child_id(program, return_node, beskid_analysis::syntax_query::DynNodeRef::from(value))
        })?;
        if returned != expression {
            return None;
        }
        let item_node = nearest_ancestor(index, return_node, |kind| {
            matches!(
                kind,
                beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                    | beskid_analysis::syntax_query::NodeKind::MethodDefinition
            )
        })?;
        let item = index.node_at(program, item_node)?;
        let return_type = item
            .of::<beskid_analysis::syntax::FunctionDefinition>()
            .and_then(|function| function.return_type.as_ref())
            .or_else(|| {
                item.of::<beskid_analysis::syntax::MethodDefinition>().and_then(|method| method.return_type.as_ref())
            })?;
        let beskid_analysis::syntax::Type::Array(element) = &return_type.node else { return None };
        if let Some(parameter) = generic_parameter_reference_name(&element.node) {
            return Some(
                enclosing
                    .iter()
                    .find(|binding| binding.parameter.as_ref() == parameter)
                    .map(|binding| binding.argument)
                    .ok_or_else(|| SemanticError::unavailable("empty_array_literal_element_specialization")),
            );
        }
        Some(abi_type_from_syntax(db, AstNodeKey { node: item_node, ..key }, &element.node))
    })?
    .transpose()
}

/// Return the element ABI for an indexed, explicitly declared local array.
///
/// Array literals own allocation metadata, but an index operation may address an array supplied
/// by a parameter or constructed by a runtime intrinsic. In that case the declaration's `T[]`
/// syntax is the only authority for the element representation. Inferred, non-local, generic,
/// string, and stale targets deliberately remain unavailable.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn array_index_element_abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    Ok(match array_index_element_template_tracked(db, syntax, key)? {
        Some(ArrayIndexElementTemplate::Concrete(element)) => Some(element),
        Some(ArrayIndexElementTemplate::EnclosingParameter(_)) => {
            return Err(SemanticError::unavailable("array_index_element_abi_type"));
        }
        None => None,
    })
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn array_index_element_template_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ArrayIndexElementTemplate> {
    with_node(db, syntax, key, |program, index, node| {
        let (index_node, indexed) = if let Some(indexed) = node.of::<beskid_analysis::syntax::IndexExpression>() {
            (key.node, indexed)
        } else {
            let assignment = node.of::<beskid_analysis::syntax::AssignExpression>()?;
            if assignment.op.node != beskid_analysis::syntax::AssignOp::Assign {
                return Some(Err(SemanticError::unavailable("array_index_element_abi_type")));
            }
            let target = index
                .direct_child_id(
                    program,
                    key.node,
                    beskid_analysis::syntax_query::DynNodeRef::from(assignment.target.as_ref()),
                )
                .map(|target| normalized_expression_node(index, target))?;
            (target, index.node_at(program, target)?.of::<beskid_analysis::syntax::IndexExpression>()?)
        };
        let target = index
            .direct_child_id(
                program,
                index_node,
                beskid_analysis::syntax_query::DynNodeRef::from(indexed.target.as_ref()),
            )
            .map(|target| normalized_expression_node(index, target))?;
        let target = index.node_at(program, target)?.of::<beskid_analysis::syntax::PathExpression>()?;
        let [segment] = target.path.node.segments.as_slice() else {
            return Some(Err(SemanticError::unavailable("array_index_element_abi_type")));
        };
        if !segment.node.type_args.is_empty() {
            return Some(Err(SemanticError::unavailable("array_index_element_abi_type")));
        }
        let declaration =
            resolve_lexical_declaration(program, index, index_node, segment.node.name.node.name.as_str())?;
        let parent = parent_node(index, declaration)?;
        let array_type = match index.kind(parent)? {
            beskid_analysis::syntax_query::NodeKind::Parameter => index
                .node_at(program, parent)?
                .of::<beskid_analysis::syntax::Parameter>()
                .map(|parameter| &parameter.ty.node),
            beskid_analysis::syntax_query::NodeKind::LetStatement => index
                .node_at(program, parent)?
                .of::<beskid_analysis::syntax::LetStatement>()
                .and_then(|statement| statement.type_annotation.as_ref().map(|annotation| &annotation.node)),
            _ => None,
        }
        .ok_or_else(|| SemanticError::unavailable("array_index_element_abi_type"));
        Some(array_type.and_then(|array_type| {
            let beskid_analysis::syntax::Type::Array(element) = array_type else {
                return Err(SemanticError::unavailable("array_index_element_abi_type"));
            };
            if let Some(parameter) = generic_parameter_reference_name(&element.node) {
                return Ok(ArrayIndexElementTemplate::EnclosingParameter(Arc::from(parameter)));
            }
            abi_type_from_syntax(db, AstNodeKey { node: declaration, ..key }, &element.node)
                .map(ArrayIndexElementTemplate::Concrete)
        }))
    })?
    .transpose()
}

/// Resolve an indexed array element through one exact enclosing generic specialization.
pub fn array_index_element_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<SemanticTypeId> {
    let Some(syntax) = db.syntax_unit(key.unit) else { return Ok(None) };
    if !syntax.accepts_key(db, key) {
        return Ok(None);
    }
    let Some(template) = array_index_element_template_tracked(db, syntax, key)? else { return Ok(None) };
    match template {
        ArrayIndexElementTemplate::Concrete(element) => Ok(Some(element)),
        ArrayIndexElementTemplate::EnclosingParameter(parameter) => Ok(enclosing
            .iter()
            .find(|binding| binding.parameter.as_ref() == parameter.as_ref())
            .map(|binding| binding.argument)),
    }
}
