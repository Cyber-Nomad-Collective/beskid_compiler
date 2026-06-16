//! Per-unit exported type shapes and merged dependency views for entry checking.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::hir::{
    HirContractNode, HirFieldKind, HirFunctionDefinition, HirItem, HirMethodDefinition,
    HirPath, HirPrimitiveType, HirProgram, HirType, HirTypeDefinition,
};
use crate::paths;
use crate::resolve::{ItemId, ItemKind, Resolution, ResolvedType};
use crate::syntax::{SpanInfo, Spanned};
use crate::types::result::FunctionSignature;
use crate::types::{TypeId, TypeInfo, TypeTable};

/// Exported type metadata for one compilation unit (keyed by [`ItemId`]).
#[derive(Debug, Default, Clone)]
pub struct UnitTypeSurface {
    pub types: TypeTable,
    pub function_signatures: HashMap<ItemId, FunctionSignature>,
    pub method_function_signatures: HashMap<ItemId, FunctionSignature>,
    pub struct_fields_ordered: HashMap<ItemId, Vec<(String, TypeId)>>,
    pub enum_variants_ordered: HashMap<ItemId, Vec<(String, Vec<TypeId>)>>,
    pub generic_items: HashMap<ItemId, Vec<String>>,
    pub struct_event_fields: HashMap<ItemId, HashMap<String, Option<usize>>>,
    pub contract_signatures: HashMap<(ItemId, String), FunctionSignature>,
    pub contract_method_order: HashMap<ItemId, Vec<String>>,
    pub methods_by_receiver: HashMap<(ItemId, String), ItemId>,
    pub named_type_names: HashMap<ItemId, String>,
}

/// Merged dependency + entry surfaces for body checking and codegen metadata lookup.
#[derive(Debug, Default, Clone)]
pub struct MergedTypeEnv {
    pub function_signatures: HashMap<ItemId, FunctionSignature>,
    pub method_function_signatures: HashMap<ItemId, FunctionSignature>,
    pub struct_fields_ordered: HashMap<ItemId, Vec<(String, TypeId)>>,
    pub enum_variants_ordered: HashMap<ItemId, Vec<(String, Vec<TypeId>)>>,
    pub generic_items: HashMap<ItemId, Vec<String>>,
    pub struct_event_fields: HashMap<ItemId, HashMap<String, Option<usize>>>,
    pub contract_signatures: HashMap<(ItemId, String), FunctionSignature>,
    pub contract_method_order: HashMap<ItemId, Vec<String>>,
    pub methods_by_receiver: HashMap<(ItemId, String), ItemId>,
    pub named_type_names: HashMap<ItemId, String>,
}

impl MergedTypeEnv {
    /// Build a seed [`UnitTypeSurface`] for [`TypeChecker`](crate::types::TypeChecker) from merged metadata.
    pub fn to_unit_surface(&self, types: TypeTable) -> UnitTypeSurface {
        UnitTypeSurface {
            types,
            function_signatures: self.function_signatures.clone(),
            method_function_signatures: self.method_function_signatures.clone(),
            struct_fields_ordered: self.struct_fields_ordered.clone(),
            enum_variants_ordered: self.enum_variants_ordered.clone(),
            generic_items: self.generic_items.clone(),
            struct_event_fields: self.struct_event_fields.clone(),
            contract_signatures: self.contract_signatures.clone(),
            contract_method_order: self.contract_method_order.clone(),
            methods_by_receiver: self.methods_by_receiver.clone(),
            named_type_names: self.named_type_names.clone(),
        }
    }
}

/// Build the exported type surface for one unit without walking function bodies.
pub fn build_unit_type_surface(
    program: &Spanned<HirProgram>,
    resolution: &Resolution,
    source_path: &Path,
) -> UnitTypeSurface {
    let mut builder = TypeSurfaceBuilder::new(resolution, source_path);
    builder.walk_program(program);
    builder.finish()
}

/// Merge dependency unit surfaces; `entry_surface` wins on key conflicts.
pub fn merge_unit_surfaces(
    dependency_surfaces: impl Iterator<Item = (PathBuf, Arc<UnitTypeSurface>)>,
    entry_surface: Arc<UnitTypeSurface>,
) -> MergedTypeEnv {
    merge_unit_surfaces_with_types(dependency_surfaces, entry_surface).1
}

/// Merge unit surfaces and their type tables into one canonical [`TypeTable`].
pub fn merge_unit_surfaces_with_types(
    dependency_surfaces: impl Iterator<Item = (PathBuf, Arc<UnitTypeSurface>)>,
    entry_surface: Arc<UnitTypeSurface>,
) -> (TypeTable, MergedTypeEnv) {
    let mut types = TypeTable::new();
    let mut merged = MergedTypeEnv::default();
    for (_, surface) in dependency_surfaces {
        let remap = types.import_from(&surface.types);
        merge_surface_into_remapped(&mut merged, surface.as_ref(), &remap);
    }
    let remap = types.import_from(&entry_surface.types);
    merge_surface_into_remapped(&mut merged, &entry_surface, &remap);
    (types, merged)
}

fn merge_surface_into_remapped(
    target: &mut MergedTypeEnv,
    surface: &UnitTypeSurface,
    remap: &HashMap<TypeId, TypeId>,
) {
    for (item_id, signature) in &surface.function_signatures {
        target
            .function_signatures
            .insert(*item_id, remap_signature(remap, signature));
    }
    for (item_id, signature) in &surface.method_function_signatures {
        target
            .method_function_signatures
            .insert(*item_id, remap_signature(remap, signature));
    }
    for (item_id, fields) in &surface.struct_fields_ordered {
        target.struct_fields_ordered.insert(
            *item_id,
            fields
                .iter()
                .map(|(name, type_id)| (name.clone(), remap_type_id(remap, *type_id)))
                .collect(),
        );
    }
    for (item_id, variants) in &surface.enum_variants_ordered {
        target.enum_variants_ordered.insert(
            *item_id,
            variants
                .iter()
                .map(|(name, fields)| {
                    (
                        name.clone(),
                        fields
                            .iter()
                            .map(|type_id| remap_type_id(remap, *type_id))
                            .collect(),
                    )
                })
                .collect(),
        );
    }
    target.generic_items.extend(surface.generic_items.clone());
    target
        .struct_event_fields
        .extend(surface.struct_event_fields.clone());
    for (key, signature) in &surface.contract_signatures {
        target
            .contract_signatures
            .insert(key.clone(), remap_signature(remap, signature));
    }
    target
        .contract_method_order
        .extend(surface.contract_method_order.clone());
    target
        .methods_by_receiver
        .extend(surface.methods_by_receiver.clone());
    target
        .named_type_names
        .extend(surface.named_type_names.clone());
}

fn remap_type_id(remap: &HashMap<TypeId, TypeId>, type_id: TypeId) -> TypeId {
    remap.get(&type_id).copied().unwrap_or(type_id)
}

fn remap_signature(remap: &HashMap<TypeId, TypeId>, signature: &FunctionSignature) -> FunctionSignature {
    FunctionSignature {
        params: signature
            .params
            .iter()
            .map(|param| remap_type_id(remap, *param))
            .collect(),
        return_type: remap_type_id(remap, signature.return_type),
    }
}

fn merge_surface_into(target: &mut MergedTypeEnv, surface: &UnitTypeSurface) {
    target
        .function_signatures
        .extend(surface.function_signatures.clone());
    target
        .method_function_signatures
        .extend(surface.method_function_signatures.clone());
    target
        .struct_fields_ordered
        .extend(surface.struct_fields_ordered.clone());
    target
        .enum_variants_ordered
        .extend(surface.enum_variants_ordered.clone());
    target.generic_items.extend(surface.generic_items.clone());
    target
        .struct_event_fields
        .extend(surface.struct_event_fields.clone());
    target
        .contract_signatures
        .extend(surface.contract_signatures.clone());
    target
        .contract_method_order
        .extend(surface.contract_method_order.clone());
    target
        .methods_by_receiver
        .extend(surface.methods_by_receiver.clone());
    target
        .named_type_names
        .extend(surface.named_type_names.clone());
}

struct TypeSurfaceBuilder<'a> {
    resolution: &'a Resolution,
    source_path: PathBuf,
    types: TypeTable,
    primitive_types: HashMap<HirPrimitiveType, TypeId>,
    named_types: HashMap<ItemId, TypeId>,
    generic_params: HashMap<String, TypeId>,
    surface: UnitTypeSurface,
}

impl<'a> TypeSurfaceBuilder<'a> {
    fn new(resolution: &'a Resolution, source_path: &Path) -> Self {
        let mut builder = Self {
            resolution,
            source_path: paths::unit_path_key(source_path),
            types: TypeTable::new(),
            primitive_types: HashMap::new(),
            named_types: HashMap::new(),
            generic_params: HashMap::new(),
            surface: UnitTypeSurface::default(),
        };
        builder.seed_primitives();
        builder.seed_named_types();
        builder
    }

    fn finish(mut self) -> UnitTypeSurface {
        self.surface.types = self.types;
        self.surface
    }

    fn walk_program(&mut self, program: &Spanned<HirProgram>) {
        for item in &program.node.items {
            self.walk_item(item);
        }
        self.seed_contract_signatures(program);
        for item in &program.node.items {
            match &item.node {
                HirItem::MethodDefinition(def) => self.seed_method_receiver(item.span, def),
                HirItem::ExtendTypeDefinition(def) => {
                    for method in &def.node.methods {
                        self.seed_method_receiver(method.span, method);
                    }
                }
                HirItem::TypeDefinition(def) => {
                    for method in &def.node.methods {
                        self.seed_method_receiver(method.span, method);
                    }
                }
                _ => {}
            }
        }
    }

    fn walk_item(&mut self, item: &Spanned<HirItem>) {
        match &item.node {
            HirItem::FunctionDefinition(def) => {
                self.seed_generic_item(item.span, &def.node.generics);
                self.register_foreign_function(item.span, &def.node);
            }
            HirItem::TypeDefinition(def) => {
                self.seed_generic_item(item.span, &def.node.generics);
                self.register_struct_definition(item.span, &def.node, true);
                for method in &def.node.methods {
                    self.register_foreign_method(method.span, method);
                }
            }
            HirItem::EnumDefinition(def) => {
                self.seed_generic_item(item.span, &def.node.generics);
                self.register_enum_definition(item.span, &def.node);
            }
            HirItem::ExtendTypeDefinition(def) => {
                for method in &def.node.methods {
                    self.register_foreign_method(method.span, method);
                }
            }
            HirItem::InlineModule(module) => {
                for nested in &module.node.items {
                    self.walk_item(nested);
                }
            }
            _ => {}
        }
    }

    fn seed_primitives(&mut self) {
        for primitive in [
            HirPrimitiveType::Bool,
            HirPrimitiveType::I32,
            HirPrimitiveType::I64,
            HirPrimitiveType::U8,
            HirPrimitiveType::F64,
            HirPrimitiveType::Char,
            HirPrimitiveType::String,
            HirPrimitiveType::Unit,
            HirPrimitiveType::Never,
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
                    self.surface
                        .named_type_names
                        .insert(item.id, item.name.clone());
                }
                _ => {}
            }
        }
    }

    fn seed_generic_item(
        &mut self,
        item_span: SpanInfo,
        generics: &[Spanned<crate::hir::HirIdentifier>],
    ) {
        let Some(item_id) = self.item_id_for_span(item_span) else {
            return;
        };
        let names = generics
            .iter()
            .map(|generic| generic.node.name.clone())
            .collect::<Vec<_>>();
        if !names.is_empty() {
            self.surface.generic_items.insert(item_id, names);
        }
    }

    fn register_struct_definition(
        &mut self,
        item_span: SpanInfo,
        def: &HirTypeDefinition,
        in_generic_scope: bool,
    ) {
        let mut ordered = Vec::new();
        let mut event_fields = HashMap::new();
        for field in &def.fields {
            if field.node.kind == HirFieldKind::Injected {
                continue;
            }
            if field.node.kind == HirFieldKind::Event {
                event_fields.insert(field.node.name.node.name.clone(), field.node.event_capacity);
            }
            let type_id = if in_generic_scope {
                self.type_id_for_type_in_generic_scope(&field.node.ty)
            } else {
                self.type_id_for_type(&field.node.ty)
            };
            if let Some(type_id) = type_id {
                ordered.push((field.node.name.node.name.clone(), type_id));
            }
        }
        let type_name = def.name.node.name.as_str();
        let item_id = self
            .item_id_for_name(type_name, ItemKind::Type)
            .or_else(|| self.item_id_for_span(item_span));
        if let Some(item_id) = item_id {
            self.surface.struct_fields_ordered.insert(item_id, ordered);
            if !event_fields.is_empty() {
                self.surface.struct_event_fields.insert(item_id, event_fields);
            }
        }
    }

    fn register_enum_definition(
        &mut self,
        item_span: SpanInfo,
        def: &crate::hir::HirEnumDefinition,
    ) {
        let mut inserted = Vec::new();
        for generic in &def.generics {
            let name = generic.node.name.clone();
            let type_id = self
                .types
                .intern(TypeInfo::GenericParam(name.clone()));
            self.generic_params.insert(name.clone(), type_id);
            inserted.push(name);
        }
        let mut ordered = Vec::new();
        for variant in &def.variants {
            let mut fields = Vec::new();
            for field in &variant.node.fields {
                if let Some(type_id) = self.type_id_for_type_in_generic_scope(&field.node.ty) {
                    fields.push(type_id);
                }
            }
            ordered.push((variant.node.name.node.name.clone(), fields));
        }
        let enum_name = def.name.node.name.as_str();
        let item_id = self
            .item_id_for_name(enum_name, ItemKind::Enum)
            .or_else(|| self.item_id_for_span(item_span));
        if let Some(item_id) = item_id {
            self.surface.enum_variants_ordered.insert(item_id, ordered);
        }
        for name in inserted {
            self.generic_params.remove(&name);
        }
    }

    fn register_foreign_function(&mut self, item_span: SpanInfo, def: &HirFunctionDefinition) {
        let mut inserted = Vec::new();
        for generic in &def.generics {
            let name = generic.node.name.clone();
            let type_id = self
                .types
                .intern(TypeInfo::GenericParam(name.clone()));
            self.generic_params.insert(name.clone(), type_id);
            inserted.push(name);
        }
        let return_type = def
            .return_type
            .as_ref()
            .and_then(|ty| self.resolve_foreign_return_type(ty))
            .or_else(|| self.primitive_type_id(HirPrimitiveType::Unit));
        let placeholder_param = self.primitive_type_id(HirPrimitiveType::I64);
        let mut params = Vec::new();
        for param in &def.parameters {
            let type_id = self
                .type_id_for_type_in_generic_scope(&param.node.ty)
                .or(placeholder_param);
            if let Some(type_id) = type_id {
                params.push(type_id);
            }
        }
        self.record_signature(item_span, params.clone(), return_type);
        self.register_self_parameter_method(item_span, def, &params, return_type);
        for name in inserted {
            self.generic_params.remove(&name);
        }
    }

    fn register_foreign_method(&mut self, item_span: SpanInfo, def: &Spanned<HirMethodDefinition>) {
        let return_type = def
            .node
            .return_type
            .as_ref()
            .and_then(|ty| self.type_id_for_type_in_generic_scope(ty))
            .or_else(|| self.primitive_type_id(HirPrimitiveType::Unit));
        let placeholder_param = self.primitive_type_id(HirPrimitiveType::I64);
        let mut params = Vec::new();
        for param in &def.node.parameters {
            let type_id = self
                .type_id_for_type_in_generic_scope(&param.node.ty)
                .or(placeholder_param);
            if let Some(type_id) = type_id {
                params.push(type_id);
            }
        }
        self.record_signature(item_span, params.clone(), return_type);
        if let (Some(method_item_id), Some(return_type)) =
            (self.canonical_item_id_for_span(item_span), return_type)
        {
            self.surface.method_function_signatures.insert(
                method_item_id,
                FunctionSignature {
                    params,
                    return_type,
                },
            );
        }
    }

    fn register_self_parameter_method(
        &mut self,
        item_span: SpanInfo,
        def: &HirFunctionDefinition,
        params: &[TypeId],
        return_type: Option<TypeId>,
    ) {
        let Some(first) = def.parameters.first() else {
            return;
        };
        if first.node.name.node.name != "self" {
            return;
        }
        let Some(return_type) = return_type else {
            return;
        };
        let Some(method_item_id) = self.canonical_item_id_for_span(item_span) else {
            return;
        };
        let Some(receiver_type_id) = self.type_id_for_type_in_generic_scope(&first.node.ty) else {
            return;
        };
        let Some(receiver_item) = self.named_item_id(receiver_type_id) else {
            return;
        };
        self.surface.methods_by_receiver.insert(
            (receiver_item, def.name.node.name.clone()),
            method_item_id,
        );
        self.surface.method_function_signatures.insert(
            method_item_id,
            FunctionSignature {
                params: params.iter().skip(1).copied().collect(),
                return_type,
            },
        );
    }

    fn seed_method_receiver(&mut self, method_span: SpanInfo, def: &Spanned<HirMethodDefinition>) {
        let Some(method_item_id) = self.item_id_for_span(method_span) else {
            return;
        };
        let Some(ResolvedType::Item(receiver_item_id)) =
            self.resolved_type_at(def.node.receiver_type.span)
        else {
            return;
        };
        self.surface.methods_by_receiver.insert(
            (receiver_item_id, def.node.name.node.name.clone()),
            method_item_id,
        );
    }

    fn seed_contract_signatures(&mut self, program: &Spanned<HirProgram>) {
        let definitions: HashMap<String, &Spanned<crate::hir::HirContractDefinition>> = program
            .node
            .items
            .iter()
            .filter_map(|item| match &item.node {
                HirItem::ContractDefinition(def) => Some((def.node.name.node.name.clone(), def)),
                _ => None,
            })
            .collect();
        let mut cache: HashMap<String, Vec<(String, FunctionSignature)>> = HashMap::new();
        let contract_names = definitions.keys().cloned().collect::<Vec<_>>();

        for contract_name in contract_names {
            let signatures = self.collect_contract_signatures_recursive(
                contract_name.as_str(),
                &definitions,
                &mut cache,
                &mut HashSet::new(),
            );
            let Some(contract_item_id) =
                self.item_id_for_name(&contract_name, ItemKind::Contract)
            else {
                continue;
            };
            self.surface.contract_method_order.insert(
                contract_item_id,
                signatures.iter().map(|(name, _)| name.clone()).collect(),
            );
            for (method_name, signature) in signatures {
                self.surface
                    .contract_signatures
                    .insert((contract_item_id, method_name), signature);
            }
        }
    }

    fn collect_contract_signatures_recursive(
        &mut self,
        contract_name: &str,
        definitions: &HashMap<String, &Spanned<crate::hir::HirContractDefinition>>,
        cache: &mut HashMap<String, Vec<(String, FunctionSignature)>>,
        active: &mut HashSet<String>,
    ) -> Vec<(String, FunctionSignature)> {
        if let Some(cached) = cache.get(contract_name) {
            return cached.clone();
        }
        if !active.insert(contract_name.to_string()) {
            return Vec::new();
        }

        let mut methods = Vec::new();
        let Some(definition) = definitions.get(contract_name) else {
            active.remove(contract_name);
            return methods;
        };

        for node in &definition.node.items {
            match &node.node {
                HirContractNode::MethodSignature(signature) => {
                    if methods
                        .iter()
                        .any(|(name, _)| name == &signature.node.name.node.name)
                    {
                        continue;
                    }
                    let mut params = Vec::new();
                    let mut valid = true;
                    for param in &signature.node.parameters {
                        let Some(type_id) = self.type_id_for_type(&param.node.ty) else {
                            valid = false;
                            break;
                        };
                        params.push(type_id);
                    }
                    if !valid {
                        continue;
                    }
                    let return_type = signature
                        .node
                        .return_type
                        .as_ref()
                        .and_then(|ty| self.type_id_for_type(ty))
                        .or_else(|| self.primitive_type_id(HirPrimitiveType::Unit));
                    let Some(return_type) = return_type else {
                        continue;
                    };
                    methods.push((
                        signature.node.name.node.name.clone(),
                        FunctionSignature {
                            params,
                            return_type,
                        },
                    ));
                }
                HirContractNode::Embedding(embedding) => {
                    let embedded = self.collect_contract_signatures_recursive(
                        embedding.node.name.node.name.as_str(),
                        definitions,
                        cache,
                        active,
                    );
                    for (method_name, signature) in embedded {
                        if methods.iter().any(|(name, _)| name == &method_name) {
                            continue;
                        }
                        methods.push((method_name, signature));
                    }
                }
            }
        }

        active.remove(contract_name);
        cache.insert(contract_name.to_string(), methods.clone());
        methods
    }

    fn record_signature(
        &mut self,
        item_span: SpanInfo,
        params: Vec<TypeId>,
        return_type: Option<TypeId>,
    ) {
        let Some(item_id) = self.canonical_item_id_for_span(item_span) else {
            return;
        };
        let Some(return_type) = return_type else {
            return;
        };
        self.surface.function_signatures.insert(
            item_id,
            FunctionSignature {
                params,
                return_type,
            },
        );
    }

    fn resolve_foreign_return_type(&mut self, ty: &Spanned<HirType>) -> Option<TypeId> {
        if let Some(type_id) = self.type_id_for_type_in_generic_scope(ty) {
            return Some(type_id);
        }
        if let Some(type_id) = self.type_id_for_type(ty) {
            return Some(type_id);
        }
        let HirType::Complex(path) = &ty.node else {
            return None;
        };
        self.type_id_for_path_with_args(path)
    }

    fn type_id_for_type_in_generic_scope(&mut self, ty: &Spanned<HirType>) -> Option<TypeId> {
        if let HirType::Complex(path) = &ty.node
            && path.node.segments.len() == 1
            && path.node.segments[0].node.type_args.is_empty()
            && let Some(type_id) = self
                .generic_params
                .get(&path.node.segments[0].node.name.node.name)
        {
            return Some(*type_id);
        }
        self.type_id_for_type(ty)
    }

    fn type_id_for_type(&mut self, ty: &Spanned<HirType>) -> Option<TypeId> {
        match &ty.node {
            HirType::Primitive(primitive) => self.primitive_type_id(primitive.node),
            HirType::Complex(path) => self.type_id_for_path_with_args(path),
            HirType::Array(inner) => {
                let inner_id = self.type_id_for_type(inner)?;
                Some(
                    self.types
                        .find_array_of(inner_id)
                        .unwrap_or_else(|| self.types.intern(TypeInfo::Array(inner_id))),
                )
            }
            HirType::Function {
                return_type,
                parameters,
            } => {
                let return_type = self.type_id_for_type(return_type)?;
                let mut params = Vec::with_capacity(parameters.len());
                for parameter in parameters {
                    params.push(self.type_id_for_type(parameter)?);
                }
                Some(self.types.intern(TypeInfo::Function {
                    params,
                    return_type,
                }))
            }
        }
    }

    fn type_id_for_path_with_args(&mut self, path: &Spanned<HirPath>) -> Option<TypeId> {
        let item_id = self.item_id_for_type_path(path)?;
        let base = self.named_type_id(item_id)?;
        let last = path.node.segments.last()?;
        if last.node.type_args.is_empty() {
            return Some(base);
        }
        let mut args = Vec::with_capacity(last.node.type_args.len());
        for arg in &last.node.type_args {
            args.push(self.type_id_for_type(arg)?);
        }
        Some(
            self.types
                .intern(TypeInfo::Applied {
                    base: item_id,
                    args,
                }),
        )
    }

    fn item_id_for_type_path(&self, path: &Spanned<HirPath>) -> Option<ItemId> {
        if let Some(ResolvedType::Item(item_id)) = self.resolved_type_at(path.span) {
            return Some(item_id);
        }
        let segments: Vec<String> = path
            .node
            .segments
            .iter()
            .map(|segment| segment.node.name.node.name.clone())
            .collect();
        if segments.len() >= 2 {
            let (module_path, tail) = segments.split_at(segments.len() - 1);
            if let Some(module_id) = self.resolution.module_graph.module_id(module_path)
                && let Some(module) = self.resolution.module_graph.module(module_id)
                && let Some(item_id) = module.scope.get(&tail[0])
            {
                return Some(*item_id);
            }
            if let Some(item_name) = segments.last()
                && let Some(module_id) = self.resolution.module_graph.module_id(&segments)
                && let Some(module) = self.resolution.module_graph.module(module_id)
                && let Some(item_id) = module.scope.get(item_name)
            {
                return Some(*item_id);
            }
        }
        if segments.len() == 1 {
            let name = &segments[0];
            return self
                .item_id_for_name(name, ItemKind::Enum)
                .or_else(|| self.item_id_for_name(name, ItemKind::Type));
        }
        None
    }

    fn primitive_type_id(&self, primitive: HirPrimitiveType) -> Option<TypeId> {
        self.primitive_types.get(&primitive).copied()
    }

    fn named_type_id(&self, item_id: ItemId) -> Option<TypeId> {
        self.named_types.get(&item_id).copied()
    }

    fn named_item_id(&self, type_id: TypeId) -> Option<ItemId> {
        match self.types.get(type_id) {
            Some(TypeInfo::Named(item_id)) => Some(*item_id),
            Some(TypeInfo::Applied { base, .. }) => Some(*base),
            _ => None,
        }
    }

    fn resolved_type_at(&self, span: SpanInfo) -> Option<ResolvedType> {
        self.resolution
            .tables
            .resolved_type_at(span, Some(&self.source_path))
    }

    fn item_id_for_span(&self, span: SpanInfo) -> Option<ItemId> {
        if let Some(info) = self.resolution.items.iter().find(|info| {
            info.span == span
                && info
                    .source_path
                    .as_ref()
                    .is_some_and(|source| paths::same_file(source, &self.source_path))
        }) {
            return Some(info.id);
        }
        let matches: Vec<_> = self
            .resolution
            .items
            .iter()
            .filter(|info| info.span == span)
            .collect();
        match matches.as_slice() {
            [] => None,
            [single] => Some(single.id),
            _ => None,
        }
    }

    fn item_id_for_name(&self, name: &str, kind: ItemKind) -> Option<ItemId> {
        let matches: Vec<_> = self
            .resolution
            .items
            .iter()
            .filter(|info| info.name == name && info.kind == kind)
            .collect();
        match matches.as_slice() {
            [] => None,
            [single] => Some(single.id),
            many => many
                .iter()
                .rev()
                .find(|info| {
                    info.source_path
                        .as_ref()
                        .is_some_and(|source| paths::same_file(source, &self.source_path))
                })
                .or_else(|| many.last())
                .map(|info| info.id),
        }
    }

    fn canonical_item_id_for_span(&self, span: SpanInfo) -> Option<ItemId> {
        let item_id = self.item_id_for_span(span)?;
        let symbol = self
            .resolution
            .items
            .get(item_id.0)
            .and_then(|info| info.symbol)?;
        self.resolution
            .by_symbol
            .get(&symbol)
            .copied()
            .or(Some(item_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_prefers_entry_surface_on_conflict() {
        let item = ItemId(1);
        let i32 = TypeId(0);
        let i64 = TypeId(1);

        let mut dep = UnitTypeSurface::default();
        dep.function_signatures.insert(
            item,
            FunctionSignature {
                params: vec![i32],
                return_type: i32,
            },
        );

        let mut entry = UnitTypeSurface::default();
        entry.function_signatures.insert(
            item,
            FunctionSignature {
                params: vec![i64],
                return_type: i64,
            },
        );

        let merged = merge_unit_surfaces(
            std::iter::once((PathBuf::from("dep.bd"), Arc::new(dep))),
            Arc::new(entry),
        );
        assert_eq!(
            merged.function_signatures.get(&item),
            Some(&FunctionSignature {
                params: vec![i64],
                return_type: i64,
            })
        );
    }
}
