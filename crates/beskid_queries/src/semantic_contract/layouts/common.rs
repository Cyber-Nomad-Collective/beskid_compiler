//! Canonical semantic layout implementation.

use super::super::*;

pub(in crate::semantic_contract) fn aggregate_field_layout(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    field: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Field>,
) -> Result<(Arc<str>, AggregateFieldShape), SemanticError> {
    if field.node.kind != beskid_analysis::syntax::FieldKind::Value {
        return Err(SemanticError::unavailable("aggregate_layout"));
    }
    let shape = match &field.node.ty.node {
        beskid_analysis::syntax::Type::Primitive(_) => {
            AggregateFieldShape::Scalar(semantic_type_from_syntax(&field.node.ty.node)?)
        }
        beskid_analysis::syntax::Type::Complex(path) => AggregateFieldShape::Nominal(
            resolve_nominal_layout_declaration(db, program, index, key, &path.node)
                .ok_or_else(|| SemanticError::unavailable("aggregate_layout"))?,
        ),
        // Arrays are heap-backed reference values in ABI v5, including empty literal payloads.
        beskid_analysis::syntax::Type::Array(_) => AggregateFieldShape::Scalar(SemanticTypeId::POINTER),
        _ => return Err(SemanticError::unavailable("aggregate_layout")),
    };
    Ok((Arc::from(field.node.name.node.name.as_str()), shape))
}

pub(in crate::semantic_contract) fn resolve_nominal_layout_declaration(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    if let Some(declaration) = resolve_type_declaration(db, key, path) {
        return Some(declaration);
    }
    let (name, module_path) = path.segments.split_last()?;
    if module_path.is_empty() || module_path.iter().any(|segment| !segment.node.type_args.is_empty()) {
        return None;
    }
    let mut module_path = module_path.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>();
    module_path.push(name.node.name.node.name.clone());
    let target = {
        let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
        let [target] = registry.modules.get(&(key.generation, module_path))?.as_slice() else {
            return None;
        };
        *target
    };
    if let Some(declaration) =
        unique_exported_type_in_unit(db, target, key.generation, &name.node.name.node.name, name.node.type_args.len())
    {
        return Some(declaration);
    }
    let [segment] = path.segments.as_slice() else {
        return None;
    };
    let name = segment.node.name.node.name.as_str();
    let candidates = index
        .metadata()
        .iter()
        .filter_map(|metadata| {
            let node = index.node_at(program, metadata.id)?;
            let matches = node
                .of::<beskid_analysis::syntax::TypeDefinition>()
                .is_some_and(|definition| definition.name.node.name == name)
                || node
                    .of::<beskid_analysis::syntax::EnumDefinition>()
                    .is_some_and(|definition| definition.name.node.name == name);
            matches.then_some(AstNodeKey { node: metadata.id, ..key })
        })
        .collect::<Vec<_>>();
    let [declaration] = candidates.as_slice() else {
        return None;
    };
    Some(*declaration)
}

pub(in crate::semantic_contract) fn nominal_aggregate_abi_type(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Result<SemanticTypeId, SemanticError> {
    resolve_type_declaration(db, key, path).ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    Ok(SemanticTypeId::POINTER)
}

pub(in crate::semantic_contract) fn abi_type_for_local_path(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Result<SemanticTypeId, SemanticError> {
    match path.segments.as_slice() {
        [segment] if segment.node.type_args.is_empty() => {
            let declaration =
                resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())
                    .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
            abi_local_declaration_type(db, program, index, key, declaration)
                .unwrap_or_else(|| Err(SemanticError::unavailable("abi_type")))
        }
        [receiver, field] if receiver.node.type_args.is_empty() && field.node.type_args.is_empty() => {
            abi_type_for_direct_aggregate_field_projection(
                db,
                program,
                index,
                key,
                receiver.node.name.node.name.as_str(),
                field.node.name.node.name.as_str(),
            )
        }
        _ => Err(SemanticError::unavailable("abi_type")),
    }
}

/// Resolve only the ABI of `local.field` when `local` has an explicit nominal annotation.
///
/// This is deliberately narrower than general member typing: inferred locals, chained paths,
/// generic receiver segments, ambiguous declarations, and field forms without an exact ABI stay
/// unavailable. It supplies generic call specialization with the ABI fact it needs without
/// reconstructing retired HIR receiver types.
pub(in crate::semantic_contract) fn abi_type_for_direct_aggregate_field_projection(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    receiver_name: &str,
    field_name: &str,
) -> Result<SemanticTypeId, SemanticError> {
    let local = resolve_lexical_declaration(program, index, key.node, receiver_name)
        .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let parent = parent_node(index, local).ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let annotation = match index.kind(parent) {
        Some(beskid_analysis::syntax_query::NodeKind::Parameter) => index
            .node_at(program, parent)
            .and_then(|node| node.of::<beskid_analysis::syntax::Parameter>())
            .map(|parameter| &parameter.ty.node),
        Some(beskid_analysis::syntax_query::NodeKind::LetStatement) => index
            .node_at(program, parent)
            .and_then(|node| node.of::<beskid_analysis::syntax::LetStatement>())
            .and_then(|statement| statement.type_annotation.as_ref())
            .map(|annotation| &annotation.node),
        _ => None,
    }
    .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let beskid_analysis::syntax::Type::Complex(receiver_path) = annotation else {
        return Err(SemanticError::unavailable("abi_type"));
    };
    let declaration =
        resolve_type_declaration(db, key, &receiver_path.node).ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.generation(db) == declaration.generation)
        .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let (terminal, module_path) =
        receiver_path.node.segments.split_last().ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    if module_path.iter().any(|segment| !segment.node.type_args.is_empty())
        || terminal.node.type_args.len() != definition.generics.len()
    {
        return Err(SemanticError::unavailable("abi_type"));
    }
    let field = definition
        .fields
        .iter()
        .find(|field| {
            field.node.kind == beskid_analysis::syntax::FieldKind::Value && field.node.name.node.name == field_name
        })
        .ok_or_else(|| SemanticError::unavailable("abi_type"))?;
    let applied_generic = match &field.node.ty.node {
        beskid_analysis::syntax::Type::Complex(path) => {
            let [segment] = path.node.segments.as_slice() else {
                return abi_type_from_syntax(db, key, &field.node.ty.node);
            };
            segment
                .node
                .type_args
                .is_empty()
                .then(|| {
                    definition.generics.iter().position(|generic| generic.node.name == segment.node.name.node.name)
                })
                .flatten()
        }
        _ => None,
    };
    match applied_generic {
        Some(index) => abi_type_from_syntax(db, key, &terminal.node.type_args[index].node),
        None => abi_type_from_syntax(db, key, &field.node.ty.node),
    }
}

pub(in crate::semantic_contract) fn abi_local_declaration_type(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    let parent = parent_node(index, declaration)?;
    match index.kind(parent)? {
        beskid_analysis::syntax_query::NodeKind::Parameter => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::Parameter>()
            .map(|parameter| abi_type_from_syntax(db, key, &parameter.ty.node)),
        beskid_analysis::syntax_query::NodeKind::LetStatement => {
            index.node_at(program, parent)?.of::<beskid_analysis::syntax::LetStatement>().map(|statement| {
                statement.type_annotation.as_ref().map_or_else(
                    || Err(SemanticError::unavailable("abi_type")),
                    |syntax_type| abi_type_from_syntax(db, key, &syntax_type.node),
                )
            })
        }
        _ => None,
    }
}

pub(in crate::semantic_contract) fn resolve_type_declaration(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    let (name, module_path) = path.segments.split_last()?;
    let generic_arity = name.node.type_args.len();
    let name = name.node.name.node.name.as_str();
    if module_path.is_empty() {
        let mut candidates = Vec::new();
        if let Some(local) = unique_type_in_unit(db, key.unit, key.generation, name, generic_arity) {
            candidates.push(local);
        }
        let import_targets = {
            let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
            registry
                .imports
                .get(&(key.unit, key.generation))
                .into_iter()
                .flatten()
                .map(|import| import.target)
                .collect::<Vec<_>>()
        };
        candidates.extend(
            import_targets
                .into_iter()
                .filter_map(|target| unique_exported_type_in_unit(db, target, key.generation, name, generic_arity)),
        );
        let [declaration] = candidates.as_slice() else {
            return None;
        };
        return Some(*declaration);
    }
    let module_path =
        module_path.iter().map(|segment| segment.node.name.node.name.as_str()).map(str::to_owned).collect::<Vec<_>>();
    if let Some(unit) = resolve_qualified_module_unit(db, key, &module_path)
        && let Some(declaration) = unique_exported_type_in_unit(db, unit, key.generation, name, generic_arity)
    {
        return Some(declaration);
    }
    // One-type-per-file modules export `Core.Syscall.SyscallError` as both the module path and
    // the type name. Ordinary lookup looks for `SyscallError` inside `Core.Syscall` and misses;
    // retry against the assembly module registry with the terminal segment appended so applied
    // generic type arguments still resolve without requiring a short-name `use`.
    let mut type_module = module_path;
    type_module.push(name.to_owned());
    let target = {
        let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
        let [target] = registry.modules.get(&(key.generation, type_module))?.as_slice() else {
            return None;
        };
        *target
    };
    unique_exported_type_in_unit(db, target, key.generation, name, generic_arity)
}

/// Resolve a public type member through its defining syntax unit or explicit public re-exports.
pub(in crate::semantic_contract) fn unique_exported_type_in_unit(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    name: &str,
    generic_arity: usize,
) -> Option<AstNodeKey> {
    let mut pending = vec![unit];
    let mut visited = std::collections::HashSet::new();
    let mut candidates = Vec::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        if let Some(candidate) = unique_public_type_in_unit(db, current, generation, name, generic_arity) {
            candidates.push(candidate);
        }
        pending.extend(public_reexport_units(db, current, generation));
    }
    let [candidate] = candidates.as_slice() else {
        return None;
    };
    Some(*candidate)
}

pub(in crate::semantic_contract) fn unique_type_in_unit(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    name: &str,
    generic_arity: usize,
) -> Option<AstNodeKey> {
    let syntax = db.syntax_unit(unit)?;
    if syntax.generation(db) != generation {
        return None;
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let matches = index
        .metadata()
        .iter()
        .map(|metadata| metadata.id)
        .filter(|candidate| {
            index.node_at(program, *candidate).is_some_and(|node| {
                node.of::<beskid_analysis::syntax::TypeDefinition>().is_some_and(|definition| {
                    definition.name.node.name == name && definition.generics.len() == generic_arity
                }) || node.of::<beskid_analysis::syntax::EnumDefinition>().is_some_and(|definition| {
                    definition.name.node.name == name && definition.generics.len() == generic_arity
                })
            })
        })
        .collect::<Vec<_>>();
    let [node] = matches.as_slice() else {
        return None;
    };
    Some(AstNodeKey { unit, generation, node: *node })
}

pub(in crate::semantic_contract) fn unique_public_type_in_unit(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    name: &str,
    generic_arity: usize,
) -> Option<AstNodeKey> {
    let syntax = db.syntax_unit(unit)?;
    if syntax.generation(db) != generation {
        return None;
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let matches = index
        .metadata()
        .iter()
        .map(|metadata| metadata.id)
        .filter(|candidate| {
            index.node_at(program, *candidate).is_some_and(|node| {
                node.of::<beskid_analysis::syntax::TypeDefinition>().is_some_and(|definition| {
                    definition.visibility.node == beskid_analysis::syntax::Visibility::Public
                        && definition.name.node.name == name
                        && definition.generics.len() == generic_arity
                }) || node.of::<beskid_analysis::syntax::EnumDefinition>().is_some_and(|definition| {
                    definition.visibility.node == beskid_analysis::syntax::Visibility::Public
                        && definition.name.node.name == name
                        && definition.generics.len() == generic_arity
                })
            })
        })
        .collect::<Vec<_>>();
    let [node] = matches.as_slice() else {
        return None;
    };
    Some(AstNodeKey { unit, generation, node: *node })
}

pub(in crate::semantic_contract) fn semantic_type_from_syntax(
    syntax_type: &beskid_analysis::syntax::Type,
) -> Result<SemanticTypeId, SemanticError> {
    use beskid_analysis::syntax::{PrimitiveType, Type};

    match syntax_type {
        Type::Primitive(primitive) => Ok(match primitive.node {
            PrimitiveType::Bool => SemanticTypeId::BOOL,
            PrimitiveType::I32 => SemanticTypeId::I32,
            PrimitiveType::I64 => SemanticTypeId::I64,
            PrimitiveType::U8 => SemanticTypeId::U8,
            PrimitiveType::Pointer => SemanticTypeId::POINTER,
            PrimitiveType::Word => SemanticTypeId::WORD,
            PrimitiveType::F64 => SemanticTypeId::F64,
            PrimitiveType::Char => SemanticTypeId::CHAR,
            PrimitiveType::String => SemanticTypeId::STRING,
            PrimitiveType::Unit => SemanticTypeId::UNIT,
            PrimitiveType::Never => SemanticTypeId::NEVER,
        }),
        // ABI-v5 passes every nominal aggregate and array by managed reference.  This source
        // signature fact records the representation, while layout queries retain nominal identity.
        Type::Complex(_) | Type::Array(_) => Ok(SemanticTypeId::POINTER),
        Type::Function { .. } => Err(SemanticError::unavailable("item_signature")),
    }
}
