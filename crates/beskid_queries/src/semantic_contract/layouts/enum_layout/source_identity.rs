//! Enum layouts instantiated from a source environment or source identity.

use super::super::super::*;
use super::super::explicit_local_declaration_type;
use super::*;
use crate::semantic_contract::typing::{managed_reference_kind_for_syntax_type, pattern_binding_fact_in_environment};

pub(super) fn instantiated_enum_layout_for_path_in_environment(
    db: &dyn Db,
    use_key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
    enclosing: &[GenericSubstitution],
) -> Result<EnumLayoutFact, SemanticError> {
    let declaration = resolve_type_declaration(db, use_key, path)
        .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    let syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let definition = index
        .node_at(program, declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    if definition.generics.is_empty() {
        return enum_layout_from_definition(db, program, index, declaration, definition, None);
    }
    let terminal = path.segments.last().ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    if terminal.node.type_args.len() != definition.generics.len() {
        return Err(SemanticError::unavailable("enum_match_specialization"));
    }
    let substitutions = definition
        .generics
        .iter()
        .zip(terminal.node.type_args.iter())
        .map(|(parameter, argument)| {
            let shape = aggregate_shape_from_applied_type(db, use_key, &argument.node).or_else(|error| {
                let name = generic_parameter_reference_name(&argument.node).ok_or(error)?;
                enclosing
                    .iter()
                    .find(|binding| binding.parameter.as_ref() == name)
                    .map(|binding| AggregateFieldShape::Scalar(binding.argument))
                    .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))
            })?;
            Ok((parameter.node.name.clone(), shape))
        })
        .collect::<Result<HashMap<_, _>, SemanticError>>()?;
    enum_layout_from_definition(db, program, index, declaration, definition, Some(&substitutions))
}

/// Preserve nominal enum provenance for a direct call used as a `match` scrutinee.
///
/// The concrete call specialization remains the authority for generic method-owner bindings;
/// this only projects those bindings through the declaration's return type into the existing
/// enum layout representation.
pub(super) fn enum_layout_for_direct_call_result(
    db: &dyn Db,
    call: AstNodeKey,
) -> Result<(AstNodeKey, EnumLayoutFact), SemanticError> {
    let identity = generic_source_expression_identity(db, call)?;
    enum_layout_for_source_identity(db, call, &identity)
}

pub(in crate::semantic_contract) fn enum_layout_for_source_identity(
    db: &dyn Db,
    call: AstNodeKey,
    identity: &GenericSourceTypeIdentity,
) -> Result<(AstNodeKey, EnumLayoutFact), SemanticError> {
    let GenericSourceTypeIdentity::Nominal { arguments, .. } = &identity else {
        return Err(SemanticError::unavailable("enum_match"));
    };
    let declaration = enum_match_source_nominal_declaration(db, call, identity)?;
    let enum_syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    let definition = enum_syntax
        .syntax_index(db)
        .node_at(enum_syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    if definition.generics.is_empty() {
        return enum_layout_from_definition(
            db,
            enum_syntax.expanded_program(db),
            enum_syntax.syntax_index(db),
            declaration,
            definition,
            None,
        )
        .map(|layout| (declaration, layout));
    }
    if arguments.len() != definition.generics.len() {
        return Err(SemanticError::unavailable("enum_match"));
    }
    let substitutions = definition
        .generics
        .iter()
        .zip(arguments.iter())
        .map(|(generic, argument)| {
            let shape = match argument {
                GenericSourceTypeIdentity::Nominal { .. } => {
                    AggregateFieldShape::Nominal(enum_match_source_nominal_declaration(db, call, argument)?)
                }
                _ => AggregateFieldShape::Scalar(argument.abi_type()),
            };
            Ok((generic.node.name.clone(), shape))
        })
        .collect::<Result<HashMap<_, _>, SemanticError>>()?;
    enum_layout_from_definition(
        db,
        enum_syntax.expanded_program(db),
        enum_syntax.syntax_index(db),
        declaration,
        definition,
        Some(&substitutions),
    )
    .map(|layout| (declaration, layout))
}

/// Recover a proven nominal's declaration using its full identity and current generation.
/// The terminal spelling only selects candidates; exact identity remains mandatory.
fn enum_match_source_nominal_declaration(
    db: &dyn Db,
    key: AstNodeKey,
    identity: &GenericSourceTypeIdentity,
) -> Result<AstNodeKey, SemanticError> {
    let GenericSourceTypeIdentity::Nominal { qualified_name, arguments } = identity else {
        return Err(SemanticError::unavailable("enum_match"));
    };
    let name = qualified_name.rsplit("::").next().ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    let mut units = {
        let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
        registry
            .modules
            .iter()
            .filter(|((generation, module), _)| {
                *generation == key.generation && qualified_name.starts_with(&format!("{}::", module.join("::")))
            })
            .flat_map(|(_, units)| units.iter().copied())
            .collect::<HashSet<_>>()
    };
    units.insert(key.unit);
    let mut candidates = units.into_iter().filter_map(|unit| {
        let declaration = super::super::common::unique_type_in_unit(db, unit, key.generation, name, arguments.len())?;
        (stable_declaration_identity(db, declaration).as_ref() == Some(qualified_name)).then_some(declaration)
    });
    let declaration = candidates.next().ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    if candidates.next().is_some() {
        return Err(SemanticError::unavailable("enum_match"));
    }
    Ok(declaration)
}
