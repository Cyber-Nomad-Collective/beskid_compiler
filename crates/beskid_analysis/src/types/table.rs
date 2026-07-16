//! Canonical [`TypeId`] assignment: structural equality deduplicates primitives, named types, arrays, and functions.

use std::collections::HashMap;

use crate::hir::HirPrimitiveType;
use crate::resolve::ItemId;

/// Dense index into [`TypeTable::types`]; stable for the duration of one check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub usize);

/// Structural description interned into a [`TypeId`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

/// Intern table for [`TypeInfo`] used by [`crate::types::TypeChecker`].
#[derive(Debug, Clone)]
pub struct TypeTable {
    types: Vec<TypeInfo>,
    intern_map: HashMap<TypeInfo, TypeId>,
    primitive_ids: HashMap<HirPrimitiveType, TypeId>,
    array_ids: HashMap<TypeId, TypeId>,
}

impl Default for TypeTable {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeTable {
    pub fn new() -> Self {
        Self {
            types: Vec::new(),
            intern_map: HashMap::new(),
            primitive_ids: HashMap::new(),
            array_ids: HashMap::new(),
        }
    }

    /// Return an existing id when `info` is already present (structural hash-consing).
    pub fn intern(&mut self, info: TypeInfo) -> TypeId {
        if let Some(&existing) = self.intern_map.get(&info) {
            return existing;
        }
        let id = TypeId(self.types.len());
        self.types.push(info.clone());
        self.intern_map.insert(info.clone(), id);
        match &info {
            TypeInfo::Primitive(primitive) => {
                self.primitive_ids.insert(*primitive, id);
            }
            TypeInfo::Array(element) => {
                self.array_ids.insert(*element, id);
            }
            _ => {}
        }
        id
    }

    pub fn get(&self, id: TypeId) -> Option<&TypeInfo> {
        self.types.get(id.0)
    }

    /// Number of interned types. Bounds linear scans over [`TypeId`] space so a
    /// lookup for an un-interned type returns `None` instead of scanning forever.
    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    /// Returns an existing `TypeInfo::Array` for `element`, if already interned.
    pub fn find_array_of(&self, element: TypeId) -> Option<TypeId> {
        self.array_ids.get(&element).copied()
    }

    /// Returns an existing primitive type id when already interned.
    pub fn find_primitive(&self, primitive: HirPrimitiveType) -> Option<TypeId> {
        self.primitive_ids.get(&primitive).copied()
    }

    /// Import all types from `other`, remapping ids. References between imported types are preserved.
    pub fn import_from(&mut self, other: &TypeTable) -> HashMap<TypeId, TypeId> {
        let mut remap = HashMap::new();
        for index in 0..other.types.len() {
            let old_id = TypeId(index);
            self.import_type_id(old_id, other, &mut remap);
        }
        remap
    }

    fn import_type_id(
        &mut self,
        old_id: TypeId,
        other: &TypeTable,
        remap: &mut HashMap<TypeId, TypeId>,
    ) -> TypeId {
        if let Some(new_id) = remap.get(&old_id) {
            return *new_id;
        }
        let Some(info) = other.get(old_id).cloned() else {
            return old_id;
        };
        let remapped = self.remap_type_info_for_import(&info, other, remap);
        let new_id = self.intern(remapped);
        remap.insert(old_id, new_id);
        new_id
    }

    fn remap_type_info_for_import(
        &mut self,
        info: &TypeInfo,
        other: &TypeTable,
        remap: &mut HashMap<TypeId, TypeId>,
    ) -> TypeInfo {
        match info {
            TypeInfo::Primitive(_) | TypeInfo::Named(_) | TypeInfo::GenericParam(_) => info.clone(),
            TypeInfo::Applied { base, args } => TypeInfo::Applied {
                base: *base,
                args: args
                    .iter()
                    .map(|arg| self.import_type_id(*arg, other, remap))
                    .collect(),
            },
            TypeInfo::Function {
                params,
                return_type,
            } => TypeInfo::Function {
                params: params
                    .iter()
                    .map(|param| self.import_type_id(*param, other, remap))
                    .collect(),
                return_type: self.import_type_id(*return_type, other, remap),
            },
            TypeInfo::Array(element) => {
                TypeInfo::Array(self.import_type_id(*element, other, remap))
            }
            TypeInfo::Fiber(payload) => {
                TypeInfo::Fiber(self.import_type_id(*payload, other, remap))
            }
        }
    }
}
