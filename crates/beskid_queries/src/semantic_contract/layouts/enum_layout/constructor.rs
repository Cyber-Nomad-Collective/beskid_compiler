//! Enum constructor facts and specializations.

use super::super::super::*;
use super::*;

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn enum_constructor_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<EnumConstructorFact> {
    with_node(db, syntax, key, |program, index, node| {
        let constructor = node.of::<beskid_analysis::syntax::EnumConstructorExpression>()?;
        let declaration = applied_enum_constructor_identity(db, key, constructor, None).and_then(|identity| {
            crate::semantic_contract::contracts::concrete_type_or_enum_declaration(db, key, &identity)
                .ok_or_else(|| SemanticError::unavailable("enum_constructor"))
        });
        let declaration = match declaration {
            Ok(declaration) => declaration,
            Err(error) => return Some(Err(error)),
        };
        let layout = match enum_layout(db, key) {
            Ok(Some(layout)) => layout,
            Ok(None) | Err(_) => return Some(Err(SemanticError::unavailable("enum_constructor"))),
        };
        Some(constructor_fact(program, index, key, constructor, declaration, &layout, || {
            SemanticError::unavailable("enum_constructor")
        }))
    })?
    .transpose()
}

/// The constructor's declaration, variant and payload keys against a proven layout.
fn constructor_fact(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    constructor: &beskid_analysis::syntax::EnumConstructorExpression,
    declaration: AstNodeKey,
    layout: &EnumLayoutFact,
    unavailable: impl Fn() -> SemanticError,
) -> Result<EnumConstructorFact, SemanticError> {
    let variant_name = constructor.path.node.variant.node.name.as_str();
    let variant_index = layout
        .variants
        .iter()
        .position(|variant| variant.name.as_ref() == variant_name)
        .ok_or_else(&unavailable)?;
    if layout.variants[variant_index].fields.len() != constructor.args.len() {
        return Err(unavailable());
    }
    let payloads = constructor
        .args
        .iter()
        .map(|argument| {
            index
                .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(argument))
                .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })
                .ok_or_else(&unavailable)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let variant_index = u32::try_from(variant_index).map_err(|_| unavailable())?;
    Ok(EnumConstructorFact { declaration, variant_index, payloads: payloads.into() })
}

/// The constructor and applied layout of an enum constructor inside one immutable specialization
/// of its enclosing generic item.
///
/// The constructor's applied enum is read in the instance's environment: its own written type
/// arguments (`Option<T>::Some(value)`) through the enclosing substitutions, or, written without
/// arguments, the applied enum its position expects there (`return Result::Ok(merge(lower,
/// value))` from a lambda whose expected result is `Result<R, ParallelError>` with `R` bound).
/// `Ok(None)` for a node that is not an enum constructor.
pub fn enum_constructor_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: &GenericSpecializationInstance,
) -> SemanticQueryResult<EnumConstructorSpecialization> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return Ok(None);
    };
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let Some(constructor) =
        index.node_at(program, key.node).and_then(|node| node.of::<beskid_analysis::syntax::EnumConstructorExpression>())
    else {
        return Ok(None);
    };
    let identity = applied_enum_constructor_identity(db, key, constructor, Some(enclosing))
        .map_err(|_| SemanticError::unavailable("enum_constructor_specialization"))?;
    let (declaration, layout) = enum_layout_for_source_identity(db, key, &identity)
        .map_err(|_| SemanticError::unavailable("enum_constructor_specialization"))?;
    let constructor =
        constructor_fact(program, index, key, constructor, declaration, &layout, || {
            SemanticError::unavailable("enum_constructor_specialization")
        })?;
    Ok(Some(EnumConstructorSpecialization { constructor, layout }))
}
