use crate::hir::{
    HirEnumConstructorExpression, HirExpressionNode, HirMemberExpression, HirPathExpression,
    HirStructLiteralExpression,
};
use crate::resolve::ItemKind;
use crate::syntax::Spanned;
use crate::types::path_value::{first_field_segment_name, resolve_path_base_local};
use crate::types::result::{MethodReceiverSource, TypeError};
use crate::types::{TypeId, TypeInfo};

use super::super::TypeChecker;

impl<'a> TypeChecker<'a> {
    pub(super) fn type_struct_literal_expression(&mut self, literal: &Spanned<HirStructLiteralExpression>) -> Option<TypeId> {
        let mut type_id = self.type_id_for_path_with_args(&literal.node.path);
        if type_id.is_none()
            && let Some(segment) = literal.node.path.node.segments.last()
        {
            let fallback = self
                .item_id_for_name(&segment.node.name.node.name, ItemKind::Type)
                .and_then(|item_id| self.named_types.get(&item_id).copied());
            type_id = fallback;
        }
        let type_id = type_id?;
        let Some(item_id) = self.named_item_id(type_id) else {
            self.errors.push(TypeError::UnknownStructType { span: literal.span });
            return None;
        };
        let mapping = self.generic_mapping_for_type_id(type_id);
        let fields = self.struct_fields.get(&item_id).cloned().or_else(|| {
            self.resolution
                .items
                .iter()
                .find(|info| info.id == item_id)
                .and_then(|info| self.item_id_for_name(&info.name, ItemKind::Type))
                .and_then(|item_id| self.struct_fields.get(&item_id).cloned())
        });
        let Some(fields) = fields else {
            self.errors.push(TypeError::UnknownStructType { span: literal.span });
            return None;
        };

        let mut seen = std::collections::HashSet::new();
        for field in &literal.node.fields {
            let name = field.node.name.node.name.clone();
            seen.insert(name.clone());
            let Some(expected) = fields.get(&name) else {
                self.errors.push(TypeError::UnknownStructField { span: field.node.name.span, name });
                continue;
            };
            let expected = if mapping.is_empty() { *expected } else { self.substitute_type_id(*expected, &mapping) };
            if let Some(actual) = self.type_expression(&field.node.value) {
                self.require_same_type(field.node.value.span, expected, actual);
            }
        }

        for name in fields.keys() {
            if seen.contains(name) {
                continue;
            }
            if self.struct_event_fields.get(&item_id).and_then(|event_fields| event_fields.get(name)).is_some() {
                continue;
            }
            self.errors.push(TypeError::MissingStructField { span: literal.span, name: name.clone() });
        }

        Some(type_id)
    }

    pub(super) fn type_enum_constructor_expression(
        &mut self,
        constructor: &Spanned<HirEnumConstructorExpression>,
    ) -> Option<TypeId> {
        let mut type_id = self.type_id_for_enum_path(constructor.node.path.span, &constructor.node.path);
        if type_id.is_none() {
            let type_name = constructor
                .node
                .path
                .node
                .type_path
                .node
                .segments
                .last()
                .map(|segment| segment.node.name.node.name.as_str());
            let fallback = type_name
                .and_then(|name| self.item_id_for_name(name, ItemKind::Enum))
                .and_then(|item_id| self.named_types.get(&item_id).copied());
            type_id = fallback;
        }
        let type_id = type_id?;
        let Some(item_id) = self.named_item_id(type_id) else {
            self.errors.push(TypeError::UnknownEnumType { span: constructor.span });
            return None;
        };
        let mapping = self.generic_mapping_for_type_id(type_id);
        let mut applied_type_id = type_id;
        if mapping.is_empty()
            && let Some(expected) = self.contextual_expected_type
            && let Some(expected_item) = self.named_item_id(expected)
            && expected_item == item_id
        {
            applied_type_id = expected;
        } else if mapping.is_empty()
            && let Some(generic_names) = self.generic_items.get(&item_id)
            && !generic_names.is_empty()
            && let Some(arg_type) = constructor.node.args.first().and_then(|arg| self.type_expression(arg))
            && let Some(TypeInfo::Applied { base, .. }) = self.type_table.get(arg_type)
            && *base == item_id
        {
            applied_type_id = arg_type;
        }
        let mapping = self.generic_mapping_for_type_id(applied_type_id);
        let variants = self.enum_variants.get(&item_id).cloned().or_else(|| {
            self.resolution
                .items
                .iter()
                .find(|info| info.id == item_id)
                .and_then(|info| self.item_id_for_name(&info.name, ItemKind::Enum))
                .and_then(|item_id| self.enum_variants.get(&item_id).cloned())
        });
        let Some(variants) = variants else {
            self.errors.push(TypeError::UnknownEnumType { span: constructor.span });
            return None;
        };
        let variant_name = constructor.node.path.node.variant.node.name.clone();
        let Some(fields) = variants.get(&variant_name) else {
            self.errors.push(TypeError::UnknownEnumVariant {
                span: constructor.node.path.node.variant.span,
                name: variant_name,
            });
            return Some(type_id);
        };

        let fields: Vec<TypeId> = if mapping.is_empty() {
            fields.clone()
        } else {
            fields.iter().map(|field| self.substitute_type_id(*field, &mapping)).collect()
        };

        if constructor.node.args.len() != fields.len() {
            self.errors.push(TypeError::EnumConstructorMismatch {
                span: constructor.span,
                expected: fields.len(),
                actual: constructor.node.args.len(),
            });
            return Some(type_id);
        }

        for (arg, expected) in constructor.node.args.iter().zip(fields.iter()) {
            if let Some(actual) = self.type_expression(arg) {
                self.require_same_type(arg.span, *expected, actual);
            }
        }

        Some(applied_type_id)
    }

    pub(super) fn type_member_expression(&mut self, member: &Spanned<HirMemberExpression>) -> Option<TypeId> {
        let target_type = self.type_expression(&member.node.target)?;

        if self.method_item_for_receiver(target_type, member.node.member.node.name.as_str()).is_some() {
            self.errors.push(TypeError::UnknownValueType { span: member.span });
            return None;
        }

        let Some(item_id) = self.named_item_id(target_type) else {
            self.errors.push(TypeError::InvalidMemberTarget { span: member.span });
            return None;
        };
        let fields = self.struct_fields.get(&item_id).cloned().or_else(|| {
            self.resolution
                .items
                .iter()
                .find(|info| info.id == item_id)
                .and_then(|info| self.item_id_for_name(&info.name, ItemKind::Type))
                .and_then(|item_id| self.struct_fields.get(&item_id).cloned())
        });
        let Some(fields) = fields else {
            self.errors.push(TypeError::UnknownStructType { span: member.span });
            return None;
        };
        let mapping = self.generic_mapping_for_type_id(target_type);
        let name = member.node.member.node.name.clone();
        let Some(field_type) = fields.get(&name) else {
            self.errors.push(TypeError::UnknownStructField { span: member.node.member.span, name });
            return None;
        };
        let field_type = if mapping.is_empty() { *field_type } else { self.substitute_type_id(*field_type, &mapping) };
        Some(field_type)
    }

    pub(super) fn is_event_member_expression(&self, member: &Spanned<HirMemberExpression>) -> bool {
        let Some(target_type) = self.node_types.get(&member.node.target.id).copied() else {
            return false;
        };
        let Some(item_id) = self.named_item_id(target_type) else {
            return false;
        };
        self.struct_event_fields.get(&item_id).and_then(|fields| fields.get(&member.node.member.node.name)).is_some()
    }

    pub(super) fn is_event_path_expression(&self, path_expr: &Spanned<HirPathExpression>) -> bool {
        let segments = &path_expr.node.path.node.segments;
        let Some(field_name) = first_field_segment_name(segments) else {
            return false;
        };
        let Some(first_name) = segments.first().map(|segment| segment.node.name.node.name.as_str()) else {
            return false;
        };
        let Some(local_id) = resolve_path_base_local(
            self.resolution,
            path_expr.node.path.span,
            first_name,
            self.current_source_path.as_ref(),
        ) else {
            return false;
        };
        let Some(base_type) = self.local_types.get(&local_id).copied() else {
            return false;
        };
        let Some(item_id) = self.named_item_id(base_type) else {
            return false;
        };
        self.struct_event_fields.get(&item_id).and_then(|fields| fields.get(field_name)).is_some()
    }

    pub(super) fn resolve_event_call_target(
        &mut self,
        callee: &Spanned<HirExpressionNode>,
    ) -> Option<(MethodReceiverSource, TypeId, crate::resolve::ItemId, TypeId)> {
        match &callee.node {
            HirExpressionNode::MemberExpression(member) => {
                let receiver_type = self.type_expression(&member.node.target)?;
                let receiver_item_id = self.named_item_id(receiver_type)?;
                let field_name = member.node.member.node.name.as_str();
                let is_event =
                    self.struct_event_fields.get(&receiver_item_id).and_then(|fields| fields.get(field_name)).is_some();
                if !is_event {
                    return None;
                }
                let field_type =
                    self.struct_fields.get(&receiver_item_id).and_then(|fields| fields.get(field_name)).copied()?;
                Some((
                    MethodReceiverSource::Expression(member.node.target.span),
                    receiver_type,
                    receiver_item_id,
                    field_type,
                ))
            }
            HirExpressionNode::PathExpression(path_expr) => {
                let segments = &path_expr.node.path.node.segments;
                let field_name = first_field_segment_name(segments)?;
                let first_name = segments.first().map(|segment| segment.node.name.node.name.as_str())?;
                let local_id = resolve_path_base_local(
                    self.resolution,
                    path_expr.node.path.span,
                    first_name,
                    self.current_source_path.as_ref(),
                )?;
                let receiver_type = *self.local_types.get(&local_id)?;
                let receiver_item_id = self.named_item_id(receiver_type)?;
                let is_event =
                    self.struct_event_fields.get(&receiver_item_id).and_then(|fields| fields.get(field_name)).is_some();
                if !is_event {
                    return None;
                }
                let field_type =
                    self.struct_fields.get(&receiver_item_id).and_then(|fields| fields.get(field_name)).copied()?;
                Some((MethodReceiverSource::Local(local_id), receiver_type, receiver_item_id, field_type))
            }
            _ => None,
        }
    }
}
