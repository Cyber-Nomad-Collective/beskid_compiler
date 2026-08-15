use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::syntax::{
    ContractNode, FieldKind, FunctionDefinition, Node, MethodDefinition, Path, PrimitiveType,
    Program, Type, TypeDefinition,
};
use crate::paths;
use crate::resolve::{ItemId, ItemKind, Resolution, ResolvedType};
use crate::syntax::{SpanInfo, Spanned};
use crate::types::result::FunctionSignature;
use crate::types::{TypeId, TypeInfo, TypeTable};

use super::model::UnitTypeSurface;

pub(super) struct TypeSurfaceBuilder<'a> {
    resolution: &'a Resolution,
    source_path: PathBuf,
    types: TypeTable,
    primitive_types: HashMap<PrimitiveType, TypeId>,
    named_types: HashMap<ItemId, TypeId>,
    generic_params: HashMap<String, TypeId>,
    surface: UnitTypeSurface,
}

impl<'a> TypeSurfaceBuilder<'a> {
    pub(super) fn new(resolution: &'a Resolution, source_path: &Path) -> Self {
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

    pub(super) fn finish(mut self) -> UnitTypeSurface {
        self.surface.types = self.types;
        self.surface
    }

    pub(super) fn walk_program(&mut self, program: &Spanned<Program>) {
        for item in &program.node.items {
            self.walk_item(item);
        }
        self.seed_contract_signatures(program);
        for item in &program.node.items {
            match &item.node {
                Node::Method(def) => self.seed_method_receiver(item.span, def),
                Node::ExtendTypeDefinition(def) => {
                    for method in &def.node.methods {
                        self.seed_method_receiver(method.span, method);
                    }
                }
                Node::TypeDefinition(def) => {
                    for method in &def.node.methods {
                        self.seed_method_receiver(method.span, method);
                    }
                }
                _ => {}
            }
        }
    }

    fn walk_item(&mut self, item: &Spanned<Node>) {
        match &item.node {
            Node::Function(def) => {
                self.seed_generic_item(item.span, &def.node.generics);
                self.register_foreign_function(item.span, &def.node);
            }
            Node::TypeDefinition(def) => {
                self.seed_generic_item(item.span, &def.node.generics);
                self.register_struct_definition(item.span, &def.node, true);
                for method in &def.node.methods {
                    self.register_foreign_method(method.span, method);
                }
            }
            Node::EnumDefinition(def) => {
                self.seed_generic_item(item.span, &def.node.generics);
                self.register_enum_definition(item.span, &def.node);
            }
            Node::ExtendTypeDefinition(def) => {
                for method in &def.node.methods {
                    self.register_foreign_method(method.span, method);
                }
            }
            Node::InlineModule(module) => {
                for nested in &module.node.items {
                    self.walk_item(nested);
                }
            }
            _ => {}
        }
    }

    fn seed_primitives(&mut self) {
        for primitive in [
            PrimitiveType::Bool,
            PrimitiveType::I32,
            PrimitiveType::I64,
            PrimitiveType::U8,
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

    fn seed_generic_item(&mut self, item_span: SpanInfo, generics: &[Spanned<crate::syntax::Identifier>]) {
        let Some(item_id) = self.item_id_for_span(item_span) else {
            return;
        };
        let names = generics.iter().map(|generic| generic.node.name.clone()).collect::<Vec<_>>();
        if !names.is_empty() {
            self.surface.generic_items.insert(item_id, names);
        }
    }

    fn register_struct_definition(&mut self, item_span: SpanInfo, def: &TypeDefinition, in_generic_scope: bool) {
        let mut ordered = Vec::new();
        let mut event_fields = HashMap::new();
        for field in &def.fields {
            if field.node.kind == FieldKind::Injected {
                continue;
            }
            if field.node.kind == FieldKind::Event {
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
        let item_id = self.item_id_for_name(type_name, ItemKind::Type).or_else(|| self.item_id_for_span(item_span));
        if let Some(item_id) = item_id {
            self.surface.struct_fields_ordered.insert(item_id, ordered);
            if !event_fields.is_empty() {
                self.surface.struct_event_fields.insert(item_id, event_fields);
            }
        }
    }

    fn register_enum_definition(&mut self, item_span: SpanInfo, def: &crate::syntax::EnumDefinition) {
        let mut inserted = Vec::new();
        for generic in &def.generics {
            let name = generic.node.name.clone();
            let type_id = self.types.intern(TypeInfo::GenericParam(name.clone()));
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
        let item_id = self.item_id_for_name(enum_name, ItemKind::Enum).or_else(|| self.item_id_for_span(item_span));
        if let Some(item_id) = item_id {
            self.surface.enum_variants_ordered.insert(item_id, ordered);
        }
        for name in inserted {
            self.generic_params.remove(&name);
        }
    }

    fn register_foreign_function(&mut self, item_span: SpanInfo, def: &FunctionDefinition) {
        let mut inserted = Vec::new();
        for generic in &def.generics {
            let name = generic.node.name.clone();
            let type_id = self.types.intern(TypeInfo::GenericParam(name.clone()));
            self.generic_params.insert(name.clone(), type_id);
            inserted.push(name);
        }
        let return_type = def
            .return_type
            .as_ref()
            .and_then(|ty| self.resolve_foreign_return_type(ty))
            .or_else(|| self.primitive_type_id(PrimitiveType::Unit));
        let placeholder_param = self.primitive_type_id(PrimitiveType::I64);
        let mut params = Vec::new();
        for param in &def.parameters {
            let type_id = self.type_id_for_type_in_generic_scope(&param.node.ty).or(placeholder_param);
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

    fn register_foreign_method(&mut self, item_span: SpanInfo, def: &Spanned<MethodDefinition>) {
        let return_type = def
            .node
            .return_type
            .as_ref()
            .and_then(|ty| self.type_id_for_type_in_generic_scope(ty))
            .or_else(|| self.primitive_type_id(PrimitiveType::Unit));
        let placeholder_param = self.primitive_type_id(PrimitiveType::I64);
        let mut params = Vec::new();
        for param in &def.node.parameters {
            let type_id = self.type_id_for_type_in_generic_scope(&param.node.ty).or(placeholder_param);
            if let Some(type_id) = type_id {
                params.push(type_id);
            }
        }
        self.record_signature(item_span, params.clone(), return_type);
        if let (Some(method_item_id), Some(return_type)) = (self.canonical_item_id_for_span(item_span), return_type) {
            self.surface.method_function_signatures.insert(method_item_id, FunctionSignature { params, return_type });
        }
    }

    fn register_self_parameter_method(
        &mut self,
        item_span: SpanInfo,
        def: &FunctionDefinition,
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
        self.surface.methods_by_receiver.insert((receiver_item, def.name.node.name.clone()), method_item_id);
        self.surface.method_function_signatures.insert(
            method_item_id,
            FunctionSignature { params: params.iter().skip(1).copied().collect(), return_type },
        );
    }

    fn seed_method_receiver(&mut self, method_span: SpanInfo, def: &Spanned<MethodDefinition>) {
        let Some(method_item_id) = self.item_id_for_span(method_span) else {
            return;
        };
        let Some(ResolvedType::Item(receiver_item_id)) = self.resolved_type_at(def.node.receiver_type.span) else {
            return;
        };
        self.surface.methods_by_receiver.insert((receiver_item_id, def.node.name.node.name.clone()), method_item_id);
    }

    fn seed_contract_signatures(&mut self, program: &Spanned<Program>) {
        let definitions: HashMap<String, &Spanned<crate::syntax::ContractDefinition>> = program
            .node
            .items
            .iter()
            .filter_map(|item| match &item.node {
                Node::ContractDefinition(def) => Some((def.node.name.node.name.clone(), def)),
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
            let Some(contract_item_id) = self.item_id_for_name(&contract_name, ItemKind::Contract) else {
                continue;
            };
            self.surface
                .contract_method_order
                .insert(contract_item_id, signatures.iter().map(|(name, _)| name.clone()).collect());
            for (method_name, signature) in signatures {
                self.surface.contract_signatures.insert((contract_item_id, method_name), signature);
            }
        }
    }

    fn collect_contract_signatures_recursive(
        &mut self,
        contract_name: &str,
        definitions: &HashMap<String, &Spanned<crate::syntax::ContractDefinition>>,
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
                ContractNode::MethodSignature(signature) => {
                    if methods.iter().any(|(name, _)| name == &signature.node.name.node.name) {
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
                        .or_else(|| self.primitive_type_id(PrimitiveType::Unit));
                    let Some(return_type) = return_type else {
                        continue;
                    };
                    methods.push((signature.node.name.node.name.clone(), FunctionSignature { params, return_type }));
                }
                ContractNode::Embedding(embedding) => {
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

    fn record_signature(&mut self, item_span: SpanInfo, params: Vec<TypeId>, return_type: Option<TypeId>) {
        let Some(item_id) = self.canonical_item_id_for_span(item_span) else {
            return;
        };
        let Some(return_type) = return_type else {
            return;
        };
        self.surface.function_signatures.insert(item_id, FunctionSignature { params, return_type });
    }

    fn resolve_foreign_return_type(&mut self, ty: &Spanned<Type>) -> Option<TypeId> {
        if let Some(type_id) = self.type_id_for_type_in_generic_scope(ty) {
            return Some(type_id);
        }
        if let Some(type_id) = self.type_id_for_type(ty) {
            return Some(type_id);
        }
        let Type::Complex(path) = &ty.node else {
            return None;
        };
        self.type_id_for_path_with_args(path)
    }

    fn type_id_for_type_in_generic_scope(&mut self, ty: &Spanned<Type>) -> Option<TypeId> {
        if let Type::Complex(path) = &ty.node
            && path.node.segments.len() == 1
            && path.node.segments[0].node.type_args.is_empty()
            && let Some(type_id) = self.generic_params.get(&path.node.segments[0].node.name.node.name)
        {
            return Some(*type_id);
        }
        self.type_id_for_type(ty)
    }

    fn type_id_for_type(&mut self, ty: &Spanned<Type>) -> Option<TypeId> {
        match &ty.node {
            Type::Primitive(primitive) => self.primitive_type_id(primitive.node),
            Type::Complex(path) => self.type_id_for_path_with_args(path),
            Type::Array(inner) => {
                let inner_id = self.type_id_for_type(inner)?;
                Some(self.types.find_array_of(inner_id).unwrap_or_else(|| self.types.intern(TypeInfo::Array(inner_id))))
            }
            Type::Function { return_type, parameters } => {
                let return_type = self.type_id_for_type(return_type)?;
                let mut params = Vec::with_capacity(parameters.len());
                for parameter in parameters {
                    params.push(self.type_id_for_type(parameter)?);
                }
                Some(self.types.intern(TypeInfo::Function { params, return_type }))
            }
        }
    }

    fn type_id_for_path_with_args(&mut self, path: &Spanned<Path>) -> Option<TypeId> {
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
        Some(self.types.intern(TypeInfo::Applied { base: item_id, args }))
    }

    fn item_id_for_type_path(&self, path: &Spanned<Path>) -> Option<ItemId> {
        if let Some(ResolvedType::Item(item_id)) = self.resolved_type_at(path.span) {
            return Some(item_id);
        }
        let segments: Vec<String> =
            path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect();
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
            return self.item_id_for_name(name, ItemKind::Enum).or_else(|| self.item_id_for_name(name, ItemKind::Type));
        }
        None
    }

    fn primitive_type_id(&self, primitive: PrimitiveType) -> Option<TypeId> {
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
        self.resolution.tables.resolved_type_at(span, Some(&self.source_path))
    }

    fn item_id_for_span(&self, span: SpanInfo) -> Option<ItemId> {
        if let Some(info) = self.resolution.items.iter().find(|info| {
            info.span == span
                && info.source_path.as_ref().is_some_and(|source| paths::same_file(source, &self.source_path))
        }) {
            return Some(info.id);
        }
        let matches: Vec<_> = self.resolution.items.iter().filter(|info| info.span == span).collect();
        match matches.as_slice() {
            [] => None,
            [single] => Some(single.id),
            _ => None,
        }
    }

    fn item_id_for_name(&self, name: &str, kind: ItemKind) -> Option<ItemId> {
        let matches: Vec<_> =
            self.resolution.items.iter().filter(|info| info.name == name && info.kind == kind).collect();
        match matches.as_slice() {
            [] => None,
            [single] => Some(single.id),
            many => many
                .iter()
                .rev()
                .find(|info| {
                    info.source_path.as_ref().is_some_and(|source| paths::same_file(source, &self.source_path))
                })
                .or_else(|| many.last())
                .map(|info| info.id),
        }
    }

    fn canonical_item_id_for_span(&self, span: SpanInfo) -> Option<ItemId> {
        let item_id = self.item_id_for_span(span)?;
        let symbol = self.resolution.items.get(item_id.0).and_then(|info| info.symbol)?;
        self.resolution.by_symbol.get(&symbol).copied().or(Some(item_id))
    }
}
