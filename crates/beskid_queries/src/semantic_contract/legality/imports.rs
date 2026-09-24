//! Unresolved import legality fact (E1105).
//!
//! `build_typed_program` registers a unit's top-level `use` declarations as syntax imports only
//! when the imported path names exactly one module of the assembly; any other `use` is dropped
//! without a trace, and every later name that relies on it fails as an unrelated unresolved type
//! or callee (or, when nothing relies on it, is silently accepted). This fact reports such a `use`
//! against the same assembled module registry the import registration reads, so it can never
//! disagree with what resolution sees.
//!
//! A `use` may also name one top-level item of an assembled module (`use Concurrency.Channel.SendOk;`),
//! as the assembly's own import closure resolves an import through its longest module prefix. Such
//! a path is resolved when its parent path names an assembled module one of whose units declares a
//! top-level function, type, enum, or contract with the final segment's name.
//!
//! The fact is keyed by a unit's root node. `check_items` judges only the units that own an item
//! it judges, so an assembly may still carry units whose imports reach outside it as long as
//! nothing is lowered from them.

use super::*;
use beskid_analysis::syntax::{Node, UseDeclaration};

/// One top-level `use` whose path names neither a module of the assembly nor one of its items.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct UnresolvedImport {
    /// The `use` declaration (the diagnostic site).
    pub site: AstNodeKey,
    /// The imported path, dot-separated.
    pub path: Arc<str>,
}

/// Report every top-level `use` of the unit rooted at `key` whose path names neither an assembled
/// module nor a top-level item of one, in source order. `Ok(None)` when `key` is not a registered unit root.
pub fn unresolved_imports(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[UnresolvedImport]>> {
    with_registered_syntax(db, key, unresolved_imports_tracked)
}

#[salsa::tracked(persist)]
fn unresolved_imports_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[UnresolvedImport]>> {
    let declarations = with_node(db, syntax, key, |program, index, _node| {
        (index.kind(key.node) == Some(NodeKind::Program)).then(|| {
            index
                .ids_of_kind(NodeKind::UseDeclaration)
                .filter(|node| nearest_ancestor(index, *node, |kind| kind == NodeKind::InlineModule).is_none())
                .filter_map(|node| {
                    let declaration = index.node_at(program, node)?.of::<UseDeclaration>()?;
                    let path = declaration
                        .path
                        .node
                        .segments
                        .iter()
                        .map(|segment| segment.node.name.node.name.clone())
                        .collect::<Vec<_>>();
                    Some((node, path))
                })
                .collect::<Vec<_>>()
        })
    })?;
    let Some(mut declarations) = declarations else { return Ok(None) };
    declarations.sort_unstable_by_key(|(node, _)| *node);
    let unresolved = declarations
        .into_iter()
        .filter(|(_, path)| !import_resolves(db, key.generation, path))
        .map(|(node, path)| UnresolvedImport { site: AstNodeKey { node, ..key }, path: Arc::from(path.join(".")) })
        .collect::<Vec<_>>();
    Ok(Some(unresolved.into()))
}

/// Whether `path` names an assembled module, or one top-level item of an assembled module.
fn import_resolves(db: &dyn Db, generation: SyntaxGenerationId, path: &[String]) -> bool {
    let (module, parent_units) = {
        let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
        let parent_units = path
            .split_last()
            .and_then(|(_, parent)| registry.modules.get(&(generation, parent.to_vec())).cloned())
            .unwrap_or_default();
        (registry.modules.contains_key(&(generation, path.to_vec())), parent_units)
    };
    if module {
        return true;
    }
    let Some(name) = path.last() else { return false };
    parent_units.into_iter().any(|unit| {
        let root = AstNodeKey { unit, generation, node: beskid_analysis::syntax::AstNodeId(0) };
        matches!(
            with_registered_syntax(db, root, top_level_item_names_tracked),
            Ok(Some(names)) if names.iter().any(|declared| declared.as_ref() == name)
        )
    })
}

/// The names of the top-level functions, types, enums, and contracts of the unit rooted at `key`.
#[salsa::tracked(persist)]
fn top_level_item_names_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[Arc<str>]>> {
    with_node(db, syntax, key, |program, index, _node| {
        (index.kind(key.node) == Some(NodeKind::Program)).then(|| {
            program
                .node
                .items
                .iter()
                .filter_map(|item| match &item.node {
                    Node::Function(definition) => Some(definition.node.name.node.name.as_str()),
                    Node::TypeDefinition(definition) => Some(definition.node.name.node.name.as_str()),
                    Node::EnumDefinition(definition) => Some(definition.node.name.node.name.as_str()),
                    Node::ContractDefinition(definition) => Some(definition.node.name.node.name.as_str()),
                    _ => None,
                })
                .map(Arc::from)
                .collect::<Vec<_>>()
                .into()
        })
    })
}
