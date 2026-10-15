//! Source identity projection within the canonical Salsa semantic authority.
use super::*;
use beskid_analysis::syntax::{EnumDefinition, FieldKind, Type, TypeDefinition};

pub(crate) use super::model::GenericSourceTypeIdentity as ShapeIdentity;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ShapeProjection {
    pub name: String,
    pub body: ShapeBodyProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum ShapeBodyProjection {
    Record(Vec<(AstNodeKey, String, ShapeIdentity)>),
    Enum(Vec<ShapeVariantProjection>),
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ShapeVariantProjection {
    pub key: AstNodeKey,
    pub name: String,
    pub ordinal: u32,
    pub fields: Vec<(AstNodeKey, String, ShapeIdentity)>,
}

fn bound_source_identity(ty: &Type, depth: usize, remaining: &mut usize) -> Result<(), SemanticError> {
    if depth > 128 || *remaining == 0 {
        return Err(SemanticError::unavailable("mod_shape.identity_limit"));
    }
    *remaining -= 1;
    match ty {
        Type::Primitive(_) => Ok(()),
        Type::Array(element) => bound_source_identity(&element.node, depth + 1, remaining),
        Type::Complex(path) => {
            for argument in path.node.segments.iter().flat_map(|segment| segment.node.type_args.iter()) {
                bound_source_identity(&argument.node, depth + 1, remaining)?;
            }
            Ok(())
        }
        Type::Function { .. } | Type::Associated { .. } | Type::This => {
            Err(SemanticError::unavailable("mod_shape.unsupported_identity"))
        }
    }
}

#[salsa::tracked(persist)]
pub(crate) fn mod_shape_projection(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
    arguments: Arc<[ShapeIdentity]>,
) -> Result<ShapeProjection, SemanticError> {
    if !syntax.accepts_key(db, key) {
        return Err(SemanticError::unavailable("mod_shape.generation"));
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let declaration = index.node_at(program, key.node).ok_or_else(|| SemanticError::unavailable("mod_shape.type"))?;
    let record = declaration.of::<TypeDefinition>();
    let enumeration = declaration.of::<EnumDefinition>();
    let (name, generics) = if let Some(definition) = record {
        (&definition.name, &definition.generics)
    } else if let Some(definition) = enumeration {
        (&definition.name, &definition.generics)
    } else {
        return Err(SemanticError::unavailable("mod_shape.type"));
    };
    if generics.len() != arguments.len() {
        return Err(SemanticError::unavailable("mod_shape.generic_arity"));
    }
    let environment = generics
        .iter()
        .zip(arguments.iter())
        .map(|(name, argument)| (name.node.name.as_str(), argument))
        .collect::<HashMap<_, _>>();
    let mut remaining = 65536;
    let mut project_fields =
        |owner: AstNodeKey, source_fields: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Field>]| {
            let mut fields = Vec::new();
            let mut names = std::collections::HashSet::new();
            for field in source_fields {
                if field.node.kind != FieldKind::Value || !names.insert(field.node.name.node.name.as_str()) {
                    return Err(SemanticError::unavailable("mod_shape.field_kind_or_duplicate"));
                }
                bound_source_identity(&field.node.ty.node, 0, &mut remaining)?;
                let node = index
                    .direct_child_id(program, owner.node, beskid_analysis::syntax_query::DynNodeRef::from(field))
                    .ok_or_else(|| SemanticError::unavailable("mod_shape.field_identity"))?;
                let identity =
                    generic_source_type_identity_with_substitutions(db, key, &field.node.ty.node, &environment)?;
                fields.push((AstNodeKey { node, ..key }, field.node.name.node.name.clone(), identity));
            }
            Ok(fields)
        };
    let body = if let Some(definition) = record {
        ShapeBodyProjection::Record(project_fields(key, &definition.fields)?)
    } else {
        let definition = enumeration.expect("validated enum declaration");
        if definition.variants.len() > 65536 {
            return Err(SemanticError::unavailable("mod_shape.variant_limit"));
        }
        let mut variants = Vec::new();
        let mut names = std::collections::HashSet::new();
        for (ordinal, variant) in definition.variants.iter().enumerate() {
            if !names.insert(variant.node.name.node.name.as_str()) {
                return Err(SemanticError::unavailable("mod_shape.duplicate_variant"));
            }
            let node = index
                .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(variant))
                .ok_or_else(|| SemanticError::unavailable("mod_shape.variant_identity"))?;
            let variant_key = AstNodeKey { node, ..key };
            variants.push(ShapeVariantProjection {
                key: variant_key,
                name: variant.node.name.node.name.clone(),
                ordinal: u32::try_from(ordinal).map_err(|_| SemanticError::unavailable("mod_shape.variant_limit"))?,
                fields: project_fields(variant_key, &variant.node.fields)?,
            });
        }
        ShapeBodyProjection::Enum(variants)
    };
    Ok(ShapeProjection { name: name.node.name.clone(), body })
}

pub(crate) fn mod_shape_stable_identity(db: &dyn Db, key: AstNodeKey) -> Option<Arc<str>> {
    stable_declaration_identity(db, key)
}

pub(crate) fn mod_shape_is_managed(identity: &ShapeIdentity) -> bool {
    identity.managed_reference_kind() == ManagedReferenceKind::GcManaged
}

/// Lexical declaration ancestry independent of dependency binding/module aliases.
pub(crate) fn mod_shape_lexical_path(db: &dyn Db, key: AstNodeKey) -> Option<Vec<String>> {
    let syntax = db.syntax_unit(key.unit)?;
    if !syntax.accepts_key(db, key) {
        return None;
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let mut path = Vec::new();
    let mut current = Some(key.node);
    while let Some(id) = current {
        let node = index.node_at(program, id)?;
        if let Some(definition) = node.of::<beskid_analysis::syntax::TypeDefinition>() {
            path.push(definition.name.node.name.clone());
        } else if let Some(definition) = node.of::<beskid_analysis::syntax::EnumDefinition>() {
            path.push(definition.name.node.name.clone());
        } else if let Some(module) = node.of::<beskid_analysis::syntax::InlineModule>() {
            path.push(module.name.node.name.clone());
        }
        current = parent_node(index, id);
    }
    path.reverse();
    (!path.is_empty()).then_some(path)
}

/// Resolve an identifier route from an exact caller key through the canonical item resolver
/// used for ordinary source calls. The route carries no spans or type arguments; it is never
/// a fallback name search.
pub(crate) fn mod_resolve_function_route(db: &dyn Db, caller: AstNodeKey, route: &[String]) -> Option<AstNodeKey> {
    use beskid_analysis::syntax::{Identifier, Path, PathSegment, SpanInfo, Spanned};
    let syntax = db.syntax_unit(caller.unit).filter(|syntax| syntax.accepts_key(db, caller))?;
    let span = SpanInfo::default();
    let path = Path {
        segments: route
            .iter()
            .map(|name| {
                Spanned::new(
                    PathSegment { name: Spanned::new(Identifier { name: name.clone() }, span), type_args: Vec::new() },
                    span,
                )
            })
            .collect(),
    };
    let declaration =
        resolve_item_declaration(db, syntax.expanded_program(db), syntax.syntax_index(db), caller, &path)?;
    let owner = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
    (owner.syntax_index(db).kind(declaration.node) == Some(beskid_analysis::syntax_query::NodeKind::FunctionDefinition))
        .then_some(declaration)
}

/// Caller-relative visibility of a resolved function. Canonical resolution admits some
/// same-unit names without consulting visibility, so a Mod route also requires the function
/// and every enclosing inline module outside the caller's own lexical scope chain to be
/// visible from its parent scope. A declaration in another unit or package has no shared
/// scope: the function and all its inline modules must be `pub`.
pub(crate) fn mod_function_accessible(db: &dyn Db, caller: AstNodeKey, declaration: AstNodeKey) -> bool {
    use beskid_analysis::syntax::{FunctionDefinition, InlineModule, Visibility};
    let Some(syntax) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)) else {
        return false;
    };
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let Some(function) = index.node_at(program, declaration.node).and_then(|node| node.of::<FunctionDefinition>())
    else {
        return false;
    };
    let mut caller_scopes = std::collections::HashSet::new();
    if caller.unit == declaration.unit && caller.generation == declaration.generation {
        let mut scope = module_scope(index, caller.node);
        while let Some(current) = scope {
            caller_scopes.insert(current);
            scope = outer_module_scope(index, current);
        }
    }
    let Some(mut scope) = module_scope(index, declaration.node) else {
        return false;
    };
    if !caller_scopes.contains(&scope) && function.visibility.node != Visibility::Public {
        return false;
    }
    while !caller_scopes.contains(&scope) {
        let Some(module) = index.node_at(program, scope).and_then(|node| node.of::<InlineModule>()) else {
            // The declaring unit's root: reached only through an import route that already
            // required a public export.
            return true;
        };
        let Some(parent) = outer_module_scope(index, scope) else {
            return false;
        };
        if !caller_scopes.contains(&parent) && module.visibility.node != Visibility::Public {
            return false;
        }
        scope = parent;
    }
    true
}

/// Declared signature of a resolved function in the canonical source identity model.
pub(crate) struct FunctionSignatureProjection {
    pub generic_count: u32,
    pub parameters: Vec<ShapeIdentity>,
    pub result: ShapeIdentity,
}

fn mentions_generic(ty: &Type, generics: &[&str]) -> bool {
    match ty {
        Type::Primitive(_) => false,
        Type::Array(element) => mentions_generic(&element.node, generics),
        Type::Complex(path) => {
            matches!(path.node.segments.as_slice(), [segment]
                if segment.node.type_args.is_empty() && generics.contains(&segment.node.name.node.name.as_str()))
                || path
                    .node
                    .segments
                    .iter()
                    .flat_map(|segment| segment.node.type_args.iter())
                    .any(|argument| mentions_generic(&argument.node, generics))
        }
        // Rejected earlier by the bounded identity walk.
        Type::Function { .. } | Type::Associated { .. } | Type::This => true,
    }
}

/// Parameter and result identities of a function, resolved from the function's own scope.
/// A signature that names the function's own unspecialized generic parameters has no
/// concrete identity and is unavailable rather than guessed.
pub(crate) fn mod_function_signature_projection(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<FunctionSignatureProjection, SemanticError> {
    use beskid_analysis::syntax::FunctionDefinition;
    let syntax = db
        .syntax_unit(key.unit)
        .filter(|syntax| syntax.accepts_key(db, key))
        .ok_or_else(|| SemanticError::unavailable("mod_function.generation"))?;
    let function = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), key.node)
        .and_then(|node| node.of::<FunctionDefinition>())
        .ok_or_else(|| SemanticError::unavailable("mod_function.declaration"))?;
    let generics = function.generics.iter().map(|name| name.node.name.as_str()).collect::<Vec<_>>();
    let generic_count =
        u32::try_from(generics.len()).map_err(|_| SemanticError::unavailable("mod_function.generic_limit"))?;
    if function.parameters.len() > 65536 {
        return Err(SemanticError::unavailable("mod_function.parameter_limit"));
    }
    let mut remaining = 65536;
    let mut project = |ty: &Type| -> Result<ShapeIdentity, SemanticError> {
        bound_source_identity(ty, 0, &mut remaining)?;
        if mentions_generic(ty, &generics) {
            return Err(SemanticError::unavailable("mod_function.unspecialized_generic"));
        }
        generic_source_type_identity_with_substitutions(db, key, ty, &HashMap::new())
    };
    let parameters =
        function.parameters.iter().map(|parameter| project(&parameter.node.ty.node)).collect::<Result<Vec<_>, _>>()?;
    let result = match &function.return_type {
        Some(ty) => project(&ty.node)?,
        None => ShapeIdentity::Abi(SemanticTypeId::UNIT),
    };
    Ok(FunctionSignatureProjection { generic_count, parameters, result })
}
