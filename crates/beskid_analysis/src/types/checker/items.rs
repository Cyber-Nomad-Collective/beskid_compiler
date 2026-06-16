use crate::hir::{HirItem, HirPrimitiveType, HirProgram, HirType, HirTypeDefinition};
use crate::resolve::ItemId;
use crate::syntax::{SpanInfo, Spanned};
use crate::types::result::{FunctionSignature, TypeError};
use crate::types::TypeId;

use super::TypeChecker;

impl<'a> TypeChecker<'a> {
    pub fn type_callable_items(&mut self, items: &[Spanned<HirItem>]) {
        self.type_dependency_function_items(items);
    }

    pub(super) fn seed_definitions_from_source_path(&mut self, path: &std::path::Path) {
        let Ok(source) = std::fs::read_to_string(path) else {
            return;
        };
        let logical_name = path.display().to_string();
        let Ok(program) = crate::services::parse_program_with_source_name(&logical_name, &source)
        else {
            return;
        };
        let ast: crate::syntax::Spanned<crate::hir::AstProgram> = program.into();
        let hir = crate::hir::lower_program(&ast);
        self.current_source_path = Some(crate::paths::unit_path_key(path));
        let errors_before = self.errors.len();
        self.seed_enum_definitions(&hir);
        self.seed_struct_definitions(&hir);
        self.register_foreign_function_signatures(&hir);
        self.errors.truncate(errors_before);
    }

    /// Populate struct field layout from type items without typing bodies.
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
                        .push(TypeError::InvalidEventCapacity { span: field.span });
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

    /// Populate enum variant layouts from enum items without typing bodies.
    pub(crate) fn seed_enum_definitions(&mut self, program: &Spanned<HirProgram>) {
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
                        if let Some(type_id) =
                            self.type_id_for_type_in_generic_scope(&field.node.ty)
                        {
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

    fn resolve_foreign_return_type(&mut self, ty: &Spanned<HirType>) -> Option<TypeId> {
        let errors_before = self.errors.len();
        if let Some(type_id) = self.type_id_for_type_in_generic_scope(ty) {
            return Some(type_id);
        }
        self.errors.truncate(errors_before);
        if let Some(type_id) = self.type_id_for_type(ty) {
            return Some(type_id);
        }
        self.errors.truncate(errors_before);
        let HirType::Complex(path) = &ty.node else {
            return None;
        };
        let applied = self
            .intern_foreign_applied_type(path)
            .or_else(|| {
                let errors_before = self.errors.len();
                let id = self.type_id_for_path_with_args(path);
                if id.is_none() {
                    self.errors.truncate(errors_before);
                }
                id
            });
        if applied.is_none() {
            self.errors.truncate(errors_before);
        }
        applied
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
                    .and_then(|ty| self.resolve_foreign_return_type(ty))
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
                self.record_signature(item.span, params.clone(), return_type);
                self.register_self_parameter_method(item.span, &def.node, &params, return_type);
                for name in inserted {
                    self.generic_params.remove(&name);
                }
            }
            HirItem::ExtendTypeDefinition(def) => {
                for method in &def.node.methods {
                    self.register_foreign_method_signature(method.span, method);
                }
            }
            HirItem::TypeDefinition(def) => {
                for method in &def.node.methods {
                    self.register_foreign_method_signature(method.span, method);
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
                HirItem::FunctionDefinition(_)
                | HirItem::MethodDefinition(_)
                | HirItem::ExtendTypeDefinition(_)
                | HirItem::TypeDefinition(_)
                | HirItem::TestDefinition(_) => {
                    let item_errors_before = self.errors.len();
                    self.type_item(item);
                    self.errors.truncate(item_errors_before);
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
                    .and_then(|ty| {
                        self.type_id_for_type(ty)
                            .or_else(|| self.resolve_foreign_return_type(ty))
                    })
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
                for method in &def.node.methods {
                    self.type_method_definition(method.span, method);
                }
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
                        if let Some(type_id) =
                            self.type_id_for_type_in_generic_scope(&field.node.ty)
                        {
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
        self.record_signature(item_span, params.clone(), return_type);
        if let (Some(method_item_id), Some(return_type)) =
            (self.canonical_item_id_for_span(item_span), return_type)
        {
            self.method_function_signatures.insert(
                method_item_id,
                FunctionSignature {
                    params,
                    return_type,
                },
            );
        }
        self.type_block(&def.node.body);
        self.current_receiver_item_id = previous_receiver;
    }

    pub(super) fn register_foreign_method_signature(
        &mut self,
        item_span: crate::syntax::SpanInfo,
        def: &Spanned<crate::hir::HirMethodDefinition>,
    ) {
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
        if let Some(method_item_id) = self.canonical_item_id_for_span(item_span) {
            self.method_function_signatures.insert(
                method_item_id,
                FunctionSignature {
                    params: params.clone(),
                    return_type: return_type.expect("method return type"),
                },
            );
        }
    }

    fn register_self_parameter_method(
        &mut self,
        item_span: crate::syntax::SpanInfo,
        def: &crate::hir::HirFunctionDefinition,
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
        self.methods_by_receiver
            .insert((receiver_item, def.name.node.name.clone()), method_item_id);
        self.method_function_signatures.insert(
            method_item_id,
            FunctionSignature {
                params: params.iter().skip(1).copied().collect(),
                return_type,
            },
        );
    }
}
