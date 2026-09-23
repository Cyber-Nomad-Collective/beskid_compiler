use std::collections::HashMap;
use std::path::PathBuf;

use crate::paths;
use crate::resolve::{ItemId, ItemKind, Resolution};
use crate::syntax::PrimitiveType;
use crate::types::{TypeId, TypeInfo, TypeTable};

use super::super::model::UnitTypeSurface;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ModuleImport {
    Unique(Vec<String>),
    Ambiguous,
}

pub(in crate::types::surface) struct TypeSurfaceBuilder<'a> {
    pub(super) resolution: &'a Resolution,
    pub(super) source_path: PathBuf,
    pub(super) types: TypeTable,
    pub(super) primitive_types: HashMap<PrimitiveType, TypeId>,
    pub(super) named_types: HashMap<ItemId, TypeId>,
    pub(super) generic_params: HashMap<String, TypeId>,
    pub(super) module_import_scopes: Vec<HashMap<String, ModuleImport>>,
    pub(super) surface: UnitTypeSurface,
}

impl<'a> TypeSurfaceBuilder<'a> {
    pub(in crate::types::surface) fn new(resolution: &'a Resolution, source_path: &std::path::Path) -> Self {
        let mut builder = Self {
            resolution,
            source_path: paths::unit_path_key(source_path),
            types: TypeTable::new(),
            primitive_types: HashMap::new(),
            named_types: HashMap::new(),
            generic_params: HashMap::new(),
            module_import_scopes: Vec::new(),
            surface: UnitTypeSurface::default(),
        };
        builder.seed_primitives();
        builder.seed_named_types();
        builder
    }

    pub(in crate::types::surface) fn finish(mut self) -> UnitTypeSurface {
        self.surface.types = self.types;
        self.surface
    }

    fn seed_primitives(&mut self) {
        for primitive in [
            PrimitiveType::Bool,
            PrimitiveType::I32,
            PrimitiveType::I64,
            PrimitiveType::U32,
            PrimitiveType::U8,
            PrimitiveType::Pointer,
            PrimitiveType::Word,
            PrimitiveType::F64,
            PrimitiveType::Char,
            PrimitiveType::String,
            PrimitiveType::Unit,
            PrimitiveType::Never,
        ] {
            let id = self.types.intern(TypeInfo::Primitive(primitive));
            self.primitive_types.insert(primitive, id);
        }
    }

    fn seed_named_types(&mut self) {
        for item in &self.resolution.items {
            match item.kind {
                ItemKind::Type | ItemKind::Enum | ItemKind::Contract => {
                    let id = self.types.intern(TypeInfo::Named(item.id));
                    self.named_types.insert(item.id, id);
                    self.surface.named_type_names.insert(item.id, item.name.clone());
                }
                _ => {}
            }
        }
    }
}
