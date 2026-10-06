//! Read-only current Corelib service provenance. Runtime intrinsic corpus
//! authority and ordinary user source paths cannot substitute for this registry.
use crate::{AstNodeKey, Db};

pub fn canonical_corelib_source_path(db: &dyn Db, key: AstNodeKey) -> Option<String> {
    let syntax = db.syntax_unit(key.unit)?;
    if !syntax.accepts_key(db, key) { return None; }
    db.syntax_dependency_registry().lock().expect("syntax dependency registry")
        .corelib_source_paths.get(&(key.unit, key.generation)).cloned()
}

/// Declaration names are descriptive facts of current registered nodes. The
/// callable-only item_name query intentionally does not cover these constructs.
pub fn registered_declaration_name(db:&dyn Db,key:AstNodeKey)->Option<String> {
    let syntax=db.syntax_unit(key.unit).filter(|syntax|syntax.accepts_key(db,key))?;
    let node=syntax.syntax_index(db).node_at(syntax.expanded_program(db),key.node)?;
    node.of::<beskid_analysis::syntax::ContractDefinition>().map(|definition|definition.name.node.name.clone())
        .or_else(||node.of::<beskid_analysis::syntax::TypeDefinition>().map(|definition|definition.name.node.name.clone()))
        .or_else(||node.of::<beskid_analysis::syntax::EnumDefinition>().map(|definition|definition.name.node.name.clone()))
}
