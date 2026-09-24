//! Enum constructor facts, templates, and specializations.

use super::super::super::*;
use super::super::explicit_local_declaration_type;
use super::*;
use crate::semantic_contract::typing::{managed_reference_kind_for_syntax_type, pattern_binding_fact_in_environment};

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn enum_constructor_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<EnumConstructorFact> {
    with_node(db, syntax, key, |program, index, node| {
        let constructor = node.of::<beskid_analysis::syntax::EnumConstructorExpression>()?;
        let type_path = contextual_enum_constructor_type_path(db, program, index, key, constructor)
            .unwrap_or_else(|| constructor.path.node.type_path.node.clone());
        let declaration =
            resolve_type_declaration(db, key, &type_path).ok_or_else(|| SemanticError::unavailable("enum_constructor"));
        let declaration = match declaration {
            Ok(declaration) => declaration,
            Err(error) => return Some(Err(error)),
        };
        let layout = match enum_layout(db, key) {
            Ok(Some(layout)) => layout,
            Ok(None) | Err(_) => return Some(Err(SemanticError::unavailable("enum_constructor"))),
        };
        let variant_name = constructor.path.node.variant.node.name.as_str();
        let Some(variant_index) = layout.variants.iter().position(|variant| variant.name.as_ref() == variant_name)
        else {
            return Some(Err(SemanticError::unavailable("enum_constructor")));
        };
        let variant = &layout.variants[variant_index];
        if variant.fields.len() != constructor.args.len() {
            return Some(Err(SemanticError::unavailable("enum_constructor")));
        }
        let payloads = constructor
            .args
            .iter()
            .map(|argument| {
                index
                    .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(argument))
                    .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })
                    .ok_or_else(|| SemanticError::unavailable("enum_constructor"))
            })
            .collect::<Result<Vec<_>, _>>();
        let variant_index = match u32::try_from(variant_index) {
            Ok(variant_index) => variant_index,
            Err(_) => return Some(Err(SemanticError::unavailable("enum_constructor"))),
        };
        Some(payloads.map(|payloads| EnumConstructorFact { declaration, variant_index, payloads: payloads.into() }))
    })?
    .transpose()
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn enum_constructor_template_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<EnumConstructorTemplate> {
    with_node(db, syntax, key, |program, index, node| {
        let constructor = node.of::<beskid_analysis::syntax::EnumConstructorExpression>()?;
        let contextual = contextual_enum_constructor_type_path(db, program, index, key, constructor)?;
        let declaration = resolve_type_declaration(db, key, &contextual)?;
        let declaration_syntax = db.syntax_unit(declaration.unit)?;
        let definition = declaration_syntax
            .syntax_index(db)
            .node_at(declaration_syntax.expanded_program(db), declaration.node)?
            .of::<beskid_analysis::syntax::EnumDefinition>()?;
        let terminal = contextual.segments.last()?;
        if definition.generics.is_empty() || terminal.node.type_args.len() != definition.generics.len() {
            return Some(Err(SemanticError::unavailable("enum_constructor_template")));
        }

        let owner = nearest_ancestor(index, key.node, |kind| {
            matches!(
                kind,
                beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                    | beskid_analysis::syntax_query::NodeKind::MethodDefinition
            )
        })?;
        let owner_node = index.node_at(program, owner)?;
        let generic_names = if let Some(function) = owner_node.of::<beskid_analysis::syntax::FunctionDefinition>() {
            function.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>()
        } else {
            let type_owner = parent_node(index, owner)?;
            index
                .node_at(program, type_owner)?
                .of::<beskid_analysis::syntax::TypeDefinition>()?
                .generics
                .iter()
                .map(|generic| generic.node.name.as_str())
                .collect::<Vec<_>>()
        };
        let arguments = terminal
            .node
            .type_args
            .iter()
            .map(|argument| {
                aggregate_shape_from_applied_type(db, key, &argument.node)
                    .map(EnumLayoutTemplateArgument::Concrete)
                    .or_else(|error| {
                        let parameter = generic_parameter_reference_name(&argument.node)
                            .filter(|parameter| generic_names.contains(parameter))
                            .ok_or(error)?;
                        Ok(EnumLayoutTemplateArgument::EnclosingParameter(Arc::from(parameter)))
                    })
            })
            .collect::<Result<Vec<_>, SemanticError>>();
        let arguments = match arguments {
            Ok(arguments)
                if arguments
                    .iter()
                    .any(|argument| matches!(argument, EnumLayoutTemplateArgument::EnclosingParameter(_))) =>
            {
                arguments
            }
            Ok(_) => return None,
            Err(error) => return Some(Err(error)),
        };
        let variant_name = constructor.path.node.variant.node.name.as_str();
        let Some(variant_index) = definition
            .variants
            .iter()
            .position(|variant| variant.node.name.node.name == variant_name)
            .and_then(|index| u32::try_from(index).ok())
        else {
            return Some(Err(SemanticError::unavailable("enum_constructor_template")));
        };
        let variant = &definition.variants[usize::try_from(variant_index).ok()?];
        if variant.node.fields.len() != constructor.args.len() {
            return Some(Err(SemanticError::unavailable("enum_constructor_template")));
        }
        let payloads = constructor
            .args
            .iter()
            .map(|argument| {
                index
                    .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(argument))
                    .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })
                    .ok_or_else(|| SemanticError::unavailable("enum_constructor_template"))
            })
            .collect::<Result<Vec<_>, _>>();
        let payloads = match payloads {
            Ok(payloads) => payloads,
            Err(error) => return Some(Err(error)),
        };
        let parameters =
            definition.generics.iter().map(|generic| Arc::<str>::from(generic.node.name.as_str())).collect::<Vec<_>>();
        Some(Ok(EnumConstructorTemplate {
            constructor: EnumConstructorFact { declaration, variant_index, payloads: payloads.into() },
            parameters: parameters.into(),
            arguments: arguments.into(),
        }))
    })?
    .transpose()
}

pub fn enum_constructor_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<EnumConstructorSpecialization> {
    let Some(template) = enum_constructor_template(db, key)? else {
        return Ok(None);
    };
    let substitutions = template
        .parameters
        .iter()
        .zip(template.arguments.iter())
        .map(|(parameter, argument)| {
            let shape = match argument {
                EnumLayoutTemplateArgument::Concrete(shape) => *shape,
                EnumLayoutTemplateArgument::EnclosingParameter(name) => {
                    let semantic = enclosing
                        .iter()
                        .find(|binding| binding.parameter == *name)
                        .map(|binding| binding.argument)
                        .ok_or_else(|| SemanticError::unavailable("enum_constructor_specialization"))?;
                    AggregateFieldShape::Scalar(semantic)
                }
            };
            Ok((parameter.to_string(), shape))
        })
        .collect::<Result<HashMap<_, _>, SemanticError>>()?;
    let syntax = db
        .syntax_unit(template.constructor.declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, template.constructor.declaration))
        .ok_or_else(|| SemanticError::unavailable("enum_constructor_specialization"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), template.constructor.declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .ok_or_else(|| SemanticError::unavailable("enum_constructor_specialization"))?;
    let layout = enum_layout_from_definition(
        db,
        syntax.expanded_program(db),
        syntax.syntax_index(db),
        template.constructor.declaration,
        definition,
        Some(&substitutions),
    )?;
    Ok(Some(EnumConstructorSpecialization { constructor: template.constructor, layout }))
}
