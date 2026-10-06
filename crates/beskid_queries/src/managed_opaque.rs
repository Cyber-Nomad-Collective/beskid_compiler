//! Managed erasure authority requires exact canonical source and current package/corpus proof.
//! It is not derived from names, user attributes, paths or persisted markers.
use crate::{AstNodeKey, Db, IndexedNodeKind, node_kind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeManagedOpaqueKind {
    SerializationExtras,
    DynamicCellV1,
    DynamicPayloadV1,
}

pub fn runtime_managed_opaque_kind(db: &dyn Db, declaration: AstNodeKey) -> Option<RuntimeManagedOpaqueKind> {
    let input = db.syntax_unit(declaration.unit)?;
    if !input.accepts_key(db, declaration) {
        return None;
    }
    if crate::canonical_corelib_source_path(db,declaration).as_deref()
        == Some(beskid_abi::runtime_source::CANONICAL_SERIALIZATION_DESCRIPTORS_SOURCE_PATH)
        && node_kind(db, declaration).ok().flatten()? == IndexedNodeKind::TypeDefinition
        && crate::corelib_source_authority::registered_declaration_name(db, declaration)?.as_str() == "ExtrasBinding"
    {
        return Some(RuntimeManagedOpaqueKind::SerializationExtras);
    }
    if input.revision(db).runtime_source_authority.as_deref()?
        != beskid_abi::runtime_source::CANONICAL_PUBLIC_DYNAMIC_SOURCE_PATH
        || node_kind(db, declaration).ok().flatten()? != IndexedNodeKind::TypeDefinition
    {
        return None;
    }
    match crate::corelib_source_authority::registered_declaration_name(db, declaration)?.as_str() {
        "DynamicValueV1" => Some(RuntimeManagedOpaqueKind::DynamicCellV1),
        "DynamicPayloadV1" => Some(RuntimeManagedOpaqueKind::DynamicPayloadV1),
        _ => None,
    }
}
