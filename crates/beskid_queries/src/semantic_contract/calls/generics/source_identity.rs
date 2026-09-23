//! Source type identity for generic arguments, paths, substitutions, locals, and declarations.

use super::super::super::*;
use super::*;

/// Preserve the canonical source identity of one concrete generic argument independently of its
/// ABI representation. Nominal, array, and function types can all share the pointer ABI, so the
/// specialization key must retain their recursive source shape before ABI lowering.
pub(in crate::semantic_contract) fn generic_source_type_identity(
    db: &dyn Db,
    key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    use beskid_analysis::syntax::Type;

    Ok(match syntax_type {
        Type::Primitive(_) => GenericSourceTypeIdentity::Abi(abi_type_from_syntax(db, key, syntax_type)?),
        Type::Complex(path) => generic_source_path_identity(db, key, &path.node)?,
        Type::Array(element) => {
            GenericSourceTypeIdentity::Array(Box::new(generic_source_type_identity(db, key, &element.node)?))
        }
        Type::Function { return_type, parameters } => GenericSourceTypeIdentity::Function {
            parameters: parameters
                .iter()
                .map(|parameter| generic_source_type_identity(db, key, &parameter.node))
                .collect::<Result<Vec<_>, _>>()?
                .into(),
            result: Box::new(generic_source_type_identity(db, key, &return_type.node)?),
        },
        Type::Associated { .. } => return Err(SemanticError::unavailable("generic_source_type_identity")),
        // `This` in a method's own signature is its receiver type. `This` inside a contract's
        // own signature, used through generic-call specialization (as opposed to a direct
        // `impl`/`type` conformance site, which `beskid_analysis`'s typechecker already
        // substitutes), stays deferred, same as a bounded generic `This`.
        Type::This => {
            let receiver =
                method_this_type(db, key).ok_or_else(|| SemanticError::unavailable("generic_source_type_identity"))?;
            return generic_source_type_identity(db, key, &receiver);
        }
    })
}

pub(super) fn generic_source_path_identity(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let declaration = resolve_type_declaration(db, key, path)
        .ok_or_else(|| SemanticError::unavailable("generic_source_type_identity"))?;
    let qualified_name = stable_declaration_identity(db, declaration)
        .ok_or_else(|| SemanticError::unavailable("generic_source_type_identity"))?;
    let arguments = path
        .segments
        .iter()
        .flat_map(|segment| segment.node.type_args.iter())
        .map(|argument| generic_source_type_identity(db, key, &argument.node))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GenericSourceTypeIdentity::Nominal { qualified_name, arguments: arguments.into() })
}

pub(in crate::semantic_contract) fn generic_source_type_identity_with_substitutions(
    db: &dyn Db,
    key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
    substitutions: &HashMap<&str, &GenericSourceTypeIdentity>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    use beskid_analysis::syntax::Type;

    if let Some(parameter) = generic_parameter_reference_name(syntax_type)
        && let Some(identity) = substitutions.get(parameter)
    {
        return Ok((*identity).clone());
    }
    Ok(match syntax_type {
        Type::Primitive(_) => GenericSourceTypeIdentity::Abi(abi_type_from_syntax(db, key, syntax_type)?),
        Type::Complex(path) => {
            let declaration = resolve_type_declaration(db, key, &path.node)
                .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
            let qualified_name = stable_declaration_identity(db, declaration)
                .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
            let arguments = path
                .node
                .segments
                .iter()
                .flat_map(|segment| segment.node.type_args.iter())
                .map(|argument| generic_source_type_identity_with_substitutions(db, key, &argument.node, substitutions))
                .collect::<Result<Vec<_>, _>>()?;
            GenericSourceTypeIdentity::Nominal { qualified_name, arguments: arguments.into() }
        }
        Type::Array(element) => GenericSourceTypeIdentity::Array(Box::new(
            generic_source_type_identity_with_substitutions(db, key, &element.node, substitutions)?,
        )),
        Type::Function { return_type, parameters } => GenericSourceTypeIdentity::Function {
            parameters: parameters
                .iter()
                .map(|parameter| {
                    generic_source_type_identity_with_substitutions(db, key, &parameter.node, substitutions)
                })
                .collect::<Result<Vec<_>, _>>()?
                .into(),
            result: Box::new(generic_source_type_identity_with_substitutions(
                db,
                key,
                &return_type.node,
                substitutions,
            )?),
        },
        Type::Associated { .. } => return Err(SemanticError::unavailable("source_expression_type")),
        Type::This => {
            let receiver =
                method_this_type(db, key).ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
            return generic_source_type_identity_with_substitutions(db, key, &receiver, substitutions);
        }
    })
}

/// Source-proven type identity of a lexical local: its written parameter/let annotation, or,
/// for an unannotated `let`, the identity of its initializer. This is the single local-type
/// authority shared by expression typing and field projection; unknown stays unavailable.
pub(in crate::semantic_contract) fn generic_source_local_identity(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let parent = parent_node(index, declaration).ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    match index.kind(parent) {
        Some(beskid_analysis::syntax_query::NodeKind::Parameter) => index
            .node_at(program, parent)
            .and_then(|node| node.of::<beskid_analysis::syntax::Parameter>())
            .ok_or_else(|| SemanticError::unavailable("source_expression_type"))
            .and_then(|parameter| generic_source_type_identity(db, key, &parameter.ty.node)),
        Some(beskid_analysis::syntax_query::NodeKind::LetStatement) => {
            let statement = index
                .node_at(program, parent)
                .and_then(|node| node.of::<beskid_analysis::syntax::LetStatement>())
                .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
            if let Some(annotation) = statement.type_annotation.as_ref() {
                generic_source_type_identity(db, key, &annotation.node)
            } else {
                let initializer = index
                    .direct_child_id(program, parent, beskid_analysis::syntax_query::DynNodeRef::from(&statement.value))
                    .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
                generic_source_expression_identity(db, AstNodeKey { node: initializer, ..key })
            }
        }
        _ => Err(SemanticError::unavailable("source_expression_type")),
    }
}

/// Deterministic assembly-relative identity for a resolved nominal declaration.
///
/// The assembled module registry is the authority in production. Inline module names and the
/// declaration name complete the path without leaking an absolute checkout root into symbols.
pub(in crate::semantic_contract) fn stable_declaration_identity(
    db: &dyn Db,
    declaration: AstNodeKey,
) -> Option<Arc<str>> {
    let syntax = db.syntax_unit(declaration.unit)?;
    if !syntax.accepts_key(db, declaration) {
        return None;
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let declaration_node = index.node_at(program, declaration.node)?;
    let declaration_name = declaration_node
        .of::<beskid_analysis::syntax::TypeDefinition>()
        .map(|definition| definition.name.node.name.as_str())
        .or_else(|| {
            declaration_node
                .of::<beskid_analysis::syntax::EnumDefinition>()
                .map(|definition| definition.name.node.name.as_str())
        })
        .or_else(|| {
            declaration_node
                .of::<beskid_analysis::syntax::FunctionDefinition>()
                .map(|definition| definition.name.node.name.as_str())
        })
        .or_else(|| {
            declaration_node
                .of::<beskid_analysis::syntax::MethodDefinition>()
                .map(|definition| definition.name.node.name.as_str())
        })?;
    let mut path = db
        .syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .modules
        .iter()
        .filter(|((generation, _), units)| *generation == declaration.generation && units.contains(&declaration.unit))
        .map(|((_, module_path), _)| module_path.clone())
        .min()
        .unwrap_or_default();
    let mut inline = Vec::new();
    let mut parent = parent_node(index, declaration.node);
    while let Some(node) = parent {
        if let Some(parent) = index.node_at(program, node) {
            if let Some(module) = parent.of::<beskid_analysis::syntax::InlineModule>() {
                inline.push(module.name.node.name.clone());
            } else if let Some(owner) = parent.of::<beskid_analysis::syntax::TypeDefinition>() {
                inline.push(owner.name.node.name.clone());
            }
        }
        parent = parent_node(index, node);
    }
    inline.reverse();
    path.extend(inline);
    path.push(declaration_name.to_owned());
    Some(Arc::from(path.join("::")))
}
