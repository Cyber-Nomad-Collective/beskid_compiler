//! Canonical [`TypeId`] assignment: structural equality deduplicates primitives, named types, arrays, and functions.

use crate::hir::HirPrimitiveType;
use crate::resolve::ItemId;

/// Dense index into [`TypeTable::types`]; stable for the duration of one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub usize);

/// Structural description interned into a [`TypeId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeInfo {
    Primitive(HirPrimitiveType),
    Named(ItemId),
    GenericParam(String),
    Applied {
        base: ItemId,
        args: Vec<TypeId>,
    },
    Function {
        params: Vec<TypeId>,
        return_type: TypeId,
    },
    /// Slice-like `T[]`: runtime value is a `BeskidArray` fat pointer (see `beskid_abi::BeskidArray`).
    Array(TypeId),
    /// Opaque cooperative fiber handle for spawn entry return type `T`.
    Fiber(TypeId),
}

/// Intern table for [`TypeInfo`] used by [`crate::types::context::TypeContext`].
#[derive(Debug, Default)]
pub struct TypeTable {
    types: Vec<TypeInfo>,
}

impl TypeTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return an existing id when `info` is already present (structural hash-consing).
    pub fn intern(&mut self, info: TypeInfo) -> TypeId {
        if let Some(existing) = self.types.iter().position(|entry| *entry == info) {
            return TypeId(existing);
        }
        let id = TypeId(self.types.len());
        self.types.push(info);
        id
    }

    pub fn get(&self, id: TypeId) -> Option<&TypeInfo> {
        self.types.get(id.0)
    }

    /// Returns an existing `TypeInfo::Array` for `element`, if already interned.
    pub fn find_array_of(&self, element: TypeId) -> Option<TypeId> {
        self.types.iter().enumerate().find_map(|(i, info)| {
            matches!(info, TypeInfo::Array(e) if *e == element).then_some(TypeId(i))
        })
    }
}
