//! Classification of the exact source-issued Process resource declarations.
//! A descriptive name or copied source path cannot grant a native capability.
use crate::{AstNodeKey, Db};
use beskid_abi::runtime_source::CANONICAL_FOUNDATION_PROCESS_SOURCE_PATH;
use beskid_analysis::syntax::TypeDefinition;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanonicalProcessResourceKind {
    Child,
    Reader,
    Writer,
}

pub fn canonical_process_resource_kind(db: &dyn Db, declaration: AstNodeKey) -> Option<CanonicalProcessResourceKind> {
    let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
    let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    if registry.corelib_source_paths.get(&(declaration.unit, declaration.generation))?.as_str()
        != CANONICAL_FOUNDATION_PROCESS_SOURCE_PATH
    {
        return None;
    }
    drop(registry);
    let definition =
        syntax.syntax_index(db).node_at(syntax.expanded_program(db), declaration.node)?.of::<TypeDefinition>()?;
    match definition.name.node.name.as_str() {
        "ChildProcess" => Some(CanonicalProcessResourceKind::Child),
        "ProcessReader" => Some(CanonicalProcessResourceKind::Reader),
        "ProcessWriter" => Some(CanonicalProcessResourceKind::Writer),
        _ => None,
    }
}
