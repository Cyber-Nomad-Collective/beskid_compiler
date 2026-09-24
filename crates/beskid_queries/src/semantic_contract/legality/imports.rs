//! Unresolved import legality fact (E1105).
//!
//! `build_typed_program` registers a unit's top-level `use` declarations as syntax imports only
//! when the imported path names exactly one module of the assembly; any other `use` is dropped
//! without a trace, and every later name that relies on it fails as an unrelated unresolved type
//! or callee (or, when nothing relies on it, is silently accepted). This fact reports such a `use`
//! against the same assembled module registry the import registration reads, so it can never
//! disagree with what resolution sees.
//!
//! The fact is keyed by a unit's root node. `check_items` judges only the units that own an item
//! it judges, so an assembly may still carry units whose imports reach outside it as long as
//! nothing is lowered from them.

use super::*;
use beskid_analysis::syntax::UseDeclaration;

/// One top-level `use` whose path names no module of the assembly.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct UnresolvedImport {
    /// The `use` declaration (the diagnostic site).
    pub site: AstNodeKey,
    /// The imported path, dot-separated.
    pub path: Arc<str>,
}

/// Report every top-level `use` of the unit rooted at `key` whose path names no assembled module,
/// in source order. `Ok(None)` when `key` is not a registered unit root.
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
    let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    let unresolved = declarations
        .into_iter()
        .filter(|(_, path)| !registry.modules.contains_key(&(key.generation, path.clone())))
        .map(|(node, path)| UnresolvedImport { site: AstNodeKey { node, ..key }, path: Arc::from(path.join(".")) })
        .collect::<Vec<_>>();
    Ok(Some(unresolved.into()))
}
