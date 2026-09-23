use crate::resolve::ItemKind;
use crate::syntax::{FieldKind, FunctionDefinition, MethodDefinition, PrimitiveType, SpanInfo, Spanned};
use crate::types::result::FunctionSignature;
use crate::types::{TypeId, TypeInfo};

use super::state::TypeSurfaceBuilder;

impl<'a> TypeSurfaceBuilder<'a> {
    pub(super) fn seed_generic_item(&mut self, item_span: SpanInfo, generics: &[Spanned<crate::syntax::Identifier>]) {
        let Some(item_id) = self.item_id_for_span(item_span) else {
            return;
        };
        let names = generics.iter().map(|generic| generic.node.name.clone()).collect::<Vec<_>>();
        if !names.is_empty() {
            self.surface.generic_items.insert(item_id, names);
        }
    }

    pub(super) fn register_struct_definition(&mut self, item_span: SpanInfo, def: &crate::syntax::TypeDefinition) {
        if def
            .fields
            .iter()
            .filter(|field| field.node.kind != FieldKind::Injected)
            .any(|field| self.type_references_ambiguous_module_import(&field.node.ty))
        {
            return;
        }
        let mut ordered = Vec::new();
        let mut event_fields = std::collections::HashMap::new();
        for field in &def.fields {
            if field.node.kind == FieldKind::Injected {
                continue;
            }
            if field.node.kind == FieldKind::Event {
                event_fields.insert(field.node.name.node.name.clone(), field.node.event_capacity);
            }
            let type_id = self.type_id_for_type(&field.node.ty);
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

    pub(super) fn register_enum_definition(&mut self, item_span: SpanInfo, def: &crate::syntax::EnumDefinition) {
        if def
            .variants
            .iter()
            .flat_map(|variant| &variant.node.fields)
            .any(|field| self.type_references_ambiguous_module_import(&field.node.ty))
        {
            return;
        }
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
                if let Some(type_id) = self.type_id_for_type(&field.node.ty) {
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

    pub(super) fn register_foreign_function(&mut self, item_span: SpanInfo, def: &FunctionDefinition) {
        if def.parameters.iter().any(|param| self.type_references_ambiguous_module_import(&param.node.ty))
            || def.return_type.as_ref().is_some_and(|ty| self.type_references_ambiguous_module_import(ty))
        {
            return;
        }
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
            .and_then(|ty| self.type_id_for_type(ty))
            .or_else(|| self.primitive_type_id(PrimitiveType::Unit));
        let placeholder_param = self.primitive_type_id(PrimitiveType::I64);
        let mut params = Vec::new();
        for param in &def.parameters {
            let type_id = self.type_id_for_type(&param.node.ty).or(placeholder_param);
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

    pub(super) fn register_foreign_method(&mut self, item_span: SpanInfo, def: &Spanned<MethodDefinition>) {
        if def.node.parameters.iter().any(|param| self.type_references_ambiguous_module_import(&param.node.ty))
            || def.node.return_type.as_ref().is_some_and(|ty| self.type_references_ambiguous_module_import(ty))
        {
            return;
        }
        // `This` in a method signature is the method's receiver type (mirrors
        // `TypeChecker::type_method_definition`).
        let previous_this = self
            .type_id_for_type(&def.node.receiver_type)
            .map(|receiver_type| self.generic_params.insert("This".to_string(), receiver_type));
        let return_type = def
            .node
            .return_type
            .as_ref()
            .and_then(|ty| self.type_id_for_type(ty))
            .or_else(|| self.primitive_type_id(PrimitiveType::Unit));
        let placeholder_param = self.primitive_type_id(PrimitiveType::I64);
        let mut params = Vec::new();
        for param in &def.node.parameters {
            let type_id = self.type_id_for_type(&param.node.ty).or(placeholder_param);
            if let Some(type_id) = type_id {
                params.push(type_id);
            }
        }
        match previous_this {
            Some(Some(previous)) => {
                self.generic_params.insert("This".to_string(), previous);
            }
            Some(None) => {
                self.generic_params.remove("This");
            }
            None => {}
        }
        self.record_signature(item_span, params.clone(), return_type);
        if let (Some(method_item_id), Some(return_type)) = (self.canonical_item_id_for_span(item_span), return_type) {
            self.surface.method_function_signatures.insert(method_item_id, FunctionSignature { params, return_type });
        }
    }

    pub(super) fn register_self_parameter_method(
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
        let Some(receiver_type_id) = self.type_id_for_type(&first.node.ty) else {
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

    pub(super) fn record_signature(&mut self, item_span: SpanInfo, params: Vec<TypeId>, return_type: Option<TypeId>) {
        let Some(item_id) = self.canonical_item_id_for_span(item_span) else {
            return;
        };
        let Some(return_type) = return_type else {
            return;
        };
        self.surface.function_signatures.insert(item_id, FunctionSignature { params, return_type });
    }
}
