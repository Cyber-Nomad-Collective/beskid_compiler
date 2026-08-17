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
        Some(
            definition
                .fields
                .iter()
                // Events are dispatch metadata, not ABI-v5 aggregate storage. Keeping them
                // out of this value-field layout preserves the exact physical indices used by
                // struct literals and direct value projections; a projection of an event itself
                // still has no aggregate-field fact and therefore fails closed.
                .filter(|field| field.node.kind == beskid_analysis::syntax::FieldKind::Value)
                .map(|field| aggregate_field_layout(db, program, index, key, field))
                .collect::<Result<Vec<_>, SemanticError>>()
                .map(|fields| AggregateLayoutFact { fields: fields.into() }),
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
            abi_type_from_syntax(db, AstNodeKey { node: declaration, ..key }, &element.node)
        }))
    })?
    .transpose()
}
