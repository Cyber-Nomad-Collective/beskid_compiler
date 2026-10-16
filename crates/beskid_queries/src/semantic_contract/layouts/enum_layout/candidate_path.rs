//! The applied type of an enum constructor, instantiated layouts, and field layouts.
//!
//! A constructor written without type arguments (`Result::Ok(value)`) names its application
//! only through the type its position is checked against. That requirement is read from the one
//! checking-direction authority, [`expected_type`] (target-neutral) or
//! [`expected_type_in_environment`] (inside the specialization of the enclosing generic item);
//! no constructor context is re-derived from the surrounding syntax here.

use super::super::super::*;
use super::*;

/// The applied enum a constructor names: its own written type arguments (interpreted through the
/// enclosing substitutions when there is a specialization), or, for a constructor written
/// without arguments, the applied enum its checking context expects
/// ([`expected_enum_constructor_identity`]). A non-generic enum has the identity of its
/// declaration. Unproven stays an error.
pub(in crate::semantic_contract) fn applied_enum_constructor_identity(
    db: &dyn Db,
    key: AstNodeKey,
    constructor: &beskid_analysis::syntax::EnumConstructorExpression,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let written = &constructor.path.node.type_path;
    let (terminal, module_path) =
        written.node.segments.split_last().ok_or_else(|| SemanticError::unavailable("enum_constructor"))?;
    if !terminal.node.type_args.is_empty() {
        let environment = enclosing
            .map(|enclosing| {
                enclosing
                    .substitutions
                    .iter()
                    .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        // An application is exact: no arguments on module segments, and every argument is a
        // layout-able type (a primitive, a nominal, an array) or an enclosing parameter the
        // specialization binds.
        if module_path.iter().any(|segment| !segment.node.type_args.is_empty())
            || terminal.node.type_args.iter().any(|argument| {
                aggregate_shape_from_applied_type(db, key, &argument.node).is_err()
                    && !generic_parameter_reference_name(&argument.node)
                        .is_some_and(|parameter| environment.contains_key(parameter))
            })
        {
            return Err(SemanticError::unavailable("enum_constructor"));
        }
        return generic_source_type_identity_with_substitutions(
            db,
            key,
            &beskid_analysis::syntax::Type::Complex(written.clone()),
            &environment,
        );
    }
    // A non-generic enum (`Color::Red`) is named by its own path alone; only a path that resolves
    // no declaration at arity zero (a generic enum) consults the expectation.
    if let Some(declaration) = resolve_type_declaration(db, key, &written.node) {
        let qualified_name = stable_declaration_identity(db, declaration)
            .ok_or_else(|| SemanticError::unavailable("enum_constructor"))?;
        return Ok(GenericSourceTypeIdentity::Nominal { qualified_name, arguments: Arc::from([]) });
    }
    expected_enum_constructor_identity(db, key, constructor, enclosing)
        .ok_or_else(|| SemanticError::unavailable("enum_constructor"))
}

/// The applied generic enum a constructor written without type arguments is checked against:
/// the identity of its expected type, when that identity is an applied enum whose declaration
/// the constructor's own path names.
///
/// Matching the terminal spelling alone would let `A.Result::Ok` inherit `B.Result<T, E>`
/// arguments, so the constructor path, resolved at the candidate's arity, must denote the
/// candidate's declaration. One unqualified constructor whose name resolves to no declaration
/// in this unit is the single exception: there is no other declaration it could name, so the
/// expected application is its only possible one; the spelling must still match. `None` when the
/// position has no such expectation.
pub(in crate::semantic_contract) fn expected_enum_constructor_identity(
    db: &dyn Db,
    key: AstNodeKey,
    constructor: &beskid_analysis::syntax::EnumConstructorExpression,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Option<GenericSourceTypeIdentity> {
    let constructor_path = &constructor.path.node.type_path.node;
    let terminal = constructor_path.segments.last()?;
    if !terminal.node.type_args.is_empty() {
        return None;
    }
    let expected = match enclosing {
        None => expected_type(db, key).ok()??,
        Some(enclosing) => expected_type_in_environment(db, key, enclosing).ok()??,
    };
    let identity = expected.identity();
    let GenericSourceTypeIdentity::Nominal { qualified_name, arguments } = identity else {
        return None;
    };
    if arguments.is_empty() || qualified_name.rsplit("::").next() != Some(terminal.node.name.node.name.as_str()) {
        return None;
    }
    let candidate = crate::semantic_contract::contracts::concrete_type_or_enum_declaration(db, key, identity)?;
    // Constructors written without type arguments cannot resolve a generic declaration by arity;
    // the candidate's arity resolves the constructor's own path, then the declarations compare.
    let mut application = constructor_path.clone();
    let placeholder = beskid_analysis::syntax::Spanned::new(
        beskid_analysis::syntax::Type::Primitive(beskid_analysis::syntax::Spanned::new(
            beskid_analysis::syntax::PrimitiveType::Unit,
            terminal.span,
        )),
        terminal.span,
    );
    application.segments.last_mut()?.node.type_args = vec![placeholder; arguments.len()];
    match resolve_type_declaration(db, key, &application) {
        Some(resolved) => (resolved == candidate).then(|| identity.clone()),
        None => {
            let [_] = constructor_path.segments.as_slice() else { return None };
            Some(identity.clone())
        }
    }
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
        beskid_analysis::syntax::Type::Array(_) => Ok(AggregateFieldShape::ManagedReference(SemanticTypeId::POINTER)),
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
