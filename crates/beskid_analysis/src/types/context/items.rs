use crate::hir::{HirItem, HirPrimitiveType, HirProgram, HirTypeDefinition};
use crate::resolve::ItemId;
use crate::syntax::{SpanInfo, Spanned};
use crate::types::TypeId;

use super::context::{FunctionSignature, TypeContext};

impl<'a> TypeContext<'a> {
    /// Populate [`struct_fields_ordered`](super::context::TypeContext::struct_fields_ordered) from type items without typing bodies.
    pub(super) fn seed_struct_definitions(&mut self, program: &Spanned<HirProgram>) {
        for item in &program.node.items {
            self.seed_struct_definitions_item(item);
        }
    }

    fn seed_struct_definitions_item(&mut self, item: &Spanned<HirItem>) {
        match &item.node {
            HirItem::TypeDefinition(def) => {
                self.register_struct_definition_fields(item.span, &def.node, false);
            }
            HirItem::InlineModule(def) => {
                for nested in &def.node.items {
                    self.seed_struct_definitions_item(nested);
                }
            }
            _ => {}
        }
    }

    /// Register struct field layout for codegen (`struct_fields_ordered`).
    fn register_struct_definition_fields(
        &mut self,
        item_span: SpanInfo,
        def: &HirTypeDefinition,
        in_generic_scope: bool,
    ) {
        let mut fields = std::collections::HashMap::new();
        let mut ordered = Vec::new();
        let mut event_fields = std::collections::HashMap::new();
        for field in &def.fields {
            if field.node.kind == crate::hir::HirFieldKind::Injected {
                continue;
            }
            if field.node.kind == crate::hir::HirFieldKind::Event {
                if matches!(field.node.event_capacity, Some(0)) {
                    self.errors
                        .push(super::context::TypeError::InvalidEventCapacity {
                            span: field.span,
                        });
                }
                event_fields.insert(field.node.name.node.name.clone(), field.node.event_capacity);
            }
            let type_id = if in_generic_scope {
                self.type_id_for_type_in_generic_scope(&field.node.ty)
            } else {
                self.type_id_for_type(&field.node.ty)
            };
            if let Some(type_id) = type_id {
                fields.insert(field.node.name.node.name.clone(), type_id);
                ordered.push((field.node.name.node.name.clone(), type_id));
            }
        }
        let type_name = def.name.node.name.as_str();
        let item_id = self
            .item_id_for_name(type_name, crate::resolve::ItemKind::Type)
            .or_else(|| self.canonical_item_id_for_span(item_span));
        if let Some(item_id) = item_id {
            self.struct_fields.insert(item_id, fields);
            self.struct_fields_ordered.insert(item_id, ordered);
            self.struct_event_fields.insert(item_id, event_fields);
        }
    }

    /// Register callable signatures from dependency units without typing their bodies.
    /// Populate [`enum_variants`](super::context::TypeContext::enum_variants) from enum items without typing bodies.
    pub(super) fn seed_enum_definitions(&mut self, program: &Spanned<HirProgram>) {
        for item in &program.node.items {
            self.seed_enum_definitions_item(item);
        }
    }

    fn seed_enum_definitions_item(&mut self, item: &Spanned<HirItem>) {
        match &item.node {
            HirItem::EnumDefinition(def) => {
                let mut inserted = Vec::new();
                for generic in &def.node.generics {
                    let name = generic.node.name.clone();
                    let type_id = self
                        .type_table
                        .intern(crate::types::TypeInfo::GenericParam(name.clone()));
                    self.generic_params.insert(name.clone(), type_id);
                    inserted.push(name);
                }
                let mut variants = std::collections::HashMap::new();
                let mut ordered = Vec::new();
                for variant in &def.node.variants {
                    let mut fields = Vec::new();
                    for field in &variant.node.fields {
                        if let Some(type_id) = self.type_id_for_type_in_generic_scope(&field.node.ty) {
                            fields.push(type_id);
                        }
                    }
                    variants.insert(variant.node.name.node.name.clone(), fields.clone());
                    ordered.push((variant.node.name.node.name.clone(), fields));
                }
                let enum_name = def.node.name.node.name.as_str();
                let item_id = self
                    .item_id_for_name(enum_name, crate::resolve::ItemKind::Enum)
                    .or_else(|| self.item_id_for_span(item.span));
                if let Some(item_id) = item_id {
                    self.enum_variants.insert(item_id, variants);
                    self.enum_variants_ordered.insert(item_id, ordered);
                }
                for name in inserted {
                    self.generic_params.remove(&name);
                }
            }
            HirItem::InlineModule(def) => {
                for nested in &def.node.items {
                    self.seed_enum_definitions_item(nested);
                }
            }
            _ => {}
        }
    }

    pub(super) fn register_foreign_function_signatures(&mut self, program: &Spanned<HirProgram>) {
        for item in &program.node.items {
            self.register_foreign_function_signatures_item(item);
        }
    }

    fn register_foreign_function_signatures_item(&mut self, item: &Spanned<HirItem>) {
        match &item.node {
            HirItem::FunctionDefinition(def) => {
                let mut inserted = Vec::new();
                for generic in &def.node.generics {
                    let name = generic.node.name.clone();
                    let type_id = self
                        .type_table
                        .intern(crate::types::TypeInfo::GenericParam(name.clone()));
                    self.generic_params.insert(name.clone(), type_id);
                    inserted.push(name);
                }
                let return_type = def
                    .node
                    .return_type
                    .as_ref()
                    .and_then(|ty| self.type_id_for_type_in_generic_scope(ty))
                    .or_else(|| self.primitive_type_id(HirPrimitiveType::Unit));
                let mut params = Vec::new();
                for param in &def.node.parameters {
                    if let Some(type_id) = self.type_id_for_type_in_generic_scope(&param.node.ty) {
                        params.push(type_id);
                    }
                }
                self.record_signature(item.span, params, return_type);
                for name in inserted {
                    self.generic_params.remove(&name);
                }
            }
            HirItem::InlineModule(def) => {
                for nested in &def.node.items {
                    self.register_foreign_function_signatures_item(nested);
                }
            }
            _ => {}
        }
    }

    pub(super) fn type_dependency_function_items(&mut self, items: &[Spanned<HirItem>]) {
        for item in items {
            match &item.node {
                HirItem::FunctionDefinition(_) | HirItem::MethodDefinition(_) => {
                    let item_errors_before = self.errors.len();
                    let item_cast_intents_before = self.cast_intents.len();
                    self.type_item(item);
                    self.errors.truncate(item_errors_before);
                    self.cast_intents.truncate(item_cast_intents_before);
                    self.flush_scoped_type_maps_for_current_path();
                }
                HirItem::InlineModule(def) => {
                    self.type_dependency_function_items(&def.node.items);
                }
                _ => {}
            }
        }
    }

    pub(super) fn type_item(&mut self, item: &Spanned<HirItem>) {
        match &item.node {
            HirItem::HostDefinition(_) => {}
            HirItem::FunctionDefinition(def) => {
                let mut inserted = Vec::new();
                for generic in &def.node.generics {
                    let name = generic.node.name.clone();
                    let type_id = self
                        .type_table
                        .intern(crate::types::TypeInfo::GenericParam(name.clone()));
                    self.generic_params.insert(name.clone(), type_id);
                    inserted.push(name);
                }
                let return_type = def
                    .node
                    .return_type
                    .as_ref()
                    .and_then(|ty| self.type_id_for_type(ty))
                    .or_else(|| self.primitive_type_id(HirPrimitiveType::Unit));
                self.current_return_type = return_type;
                let mut params = Vec::new();
                for param in &def.node.parameters {
                    if let Some(type_id) = self.type_id_for_type(&param.node.ty) {
                        params.push(type_id);
                        self.insert_local_type(param.node.name.span, type_id);
                    }
                }
                self.record_signature(item.span, params, return_type);
                self.type_block(&def.node.body);
                for name in inserted {
                    self.generic_params.remove(&name);
                }
            }
            HirItem::MethodDefinition(def) => {
                self.type_method_definition(item.span, def);
            }
            HirItem::ExtendTypeDefinition(def) => {
                self.type_id_for_type(&def.node.target_type);
                for method in &def.node.methods {
                    self.type_method_definition(method.span, method);
                }
            }
            HirItem::TestDefinition(def) => {
                if let Some(meta) = &def.node.meta {
                    for entry in &meta.node.entries {
                        self.type_expression(&entry.node.value);
                    }
                }
                if let Some(skip) = &def.node.skip {
                    for entry in &skip.node.entries {
                        self.type_expression(&entry.node.value);
                    }
                }
                let return_type = self.primitive_type_id(HirPrimitiveType::Unit);
                self.current_return_type = return_type;
                self.record_signature(item.span, Vec::new(), return_type);
                self.type_block(&def.node.body);
            }
            HirItem::TypeDefinition(def) => {
                let mut inserted = Vec::new();
                for generic in &def.node.generics {
                    let name = generic.node.name.clone();
                    let type_id = self
                        .type_table
                        .intern(crate::types::TypeInfo::GenericParam(name.clone()));
                    self.generic_params.insert(name.clone(), type_id);
                    inserted.push(name);
                }
                self.register_struct_definition_fields(item.span, &def.node, true);
                for name in inserted {
                    self.generic_params.remove(&name);
                }
            }
            HirItem::EnumDefinition(def) => {
                let mut inserted = Vec::new();
                for generic in &def.node.generics {
                    let name = generic.node.name.clone();
                    let type_id = self
                        .type_table
                        .intern(crate::types::TypeInfo::GenericParam(name.clone()));
                    self.generic_params.insert(name.clone(), type_id);
                    inserted.push(name);
                }
                let mut variants = std::collections::HashMap::new();
                let mut ordered = Vec::new();
                for variant in &def.node.variants {
                    let mut fields = Vec::new();
                    for field in &variant.node.fields {
                        if let Some(type_id) = self.type_id_for_type_in_generic_scope(&field.node.ty) {
                            fields.push(type_id);
                        }
                    }
                    variants.insert(variant.node.name.node.name.clone(), fields.clone());
                    ordered.push((variant.node.name.node.name.clone(), fields));
                }
                let enum_name = def.node.name.node.name.as_str();
                let item_id = self
                    .item_id_for_name(enum_name, crate::resolve::ItemKind::Enum)
                    .or_else(|| self.item_id_for_span(item.span));
                if let Some(item_id) = item_id {
                    self.enum_variants.insert(item_id, variants);
                    self.enum_variants_ordered.insert(item_id, ordered);
                }
                for name in inserted {
                    self.generic_params.remove(&name);
                }
            }
            HirItem::ContractDefinition(_) => {}
            HirItem::AttributeDeclaration(_) => {}
            HirItem::InlineModule(def) => {
                for item in &def.node.items {
                    self.type_item(item);
                }
            }
            HirItem::ModuleDeclaration(_) | HirItem::UseDeclaration(_) => {}
            HirItem::MacroDefinition(_) => {}
        }
        self.current_return_type = None;
    }

    fn record_signature(
        &mut self,
        item_span: crate::syntax::SpanInfo,
        params: Vec<TypeId>,
        return_type: Option<TypeId>,
    ) {
        let Some(item_id) = self.canonical_item_id_for_span(item_span) else {
            return;
        };
        let Some(return_type) = return_type else {
            return;
        };
        self.function_signatures.insert(
            item_id,
            FunctionSignature {
                params,
                return_type,
            },
        );
    }

    fn canonical_item_id_for_span(&self, span: crate::syntax::SpanInfo) -> Option<ItemId> {
        let item_id = self.item_id_for_span(span)?;
        let symbol = self.resolution.items.get(item_id.0).and_then(|info| info.symbol)?;
        self.resolution
            .by_symbol
            .get(&symbol)
            .copied()
            .or(Some(item_id))
    }

    fn type_method_definition(
        &mut self,
        item_span: crate::syntax::SpanInfo,
        def: &Spanned<crate::hir::HirMethodDefinition>,
    ) {
        let receiver_type = self.type_id_for_type(&def.node.receiver_type);
        let previous_receiver = self.current_receiver_item_id;
        self.current_receiver_item_id =
            receiver_type.and_then(|type_id| self.named_item_id(type_id));
        let return_type = def
            .node
            .return_type
            .as_ref()
            .and_then(|ty| self.type_id_for_type(ty))
            .or_else(|| self.primitive_type_id(HirPrimitiveType::Unit));
        self.current_return_type = return_type;
        if let Some(receiver_type) = receiver_type {
            self.insert_local_type(def.node.receiver_type.span, receiver_type);
        }
        let mut params = Vec::new();
        for param in &def.node.parameters {
            if let Some(type_id) = self.type_id_for_type(&param.node.ty) {
                params.push(type_id);
                self.insert_local_type(param.node.name.span, type_id);
            }
        }
        self.record_signature(item_span, params, return_type);
        self.type_block(&def.node.body);
        self.current_receiver_item_id = previous_receiver;
    }
}
