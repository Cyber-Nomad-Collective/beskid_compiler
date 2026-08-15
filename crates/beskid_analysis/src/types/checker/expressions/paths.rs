use crate::syntax::Path;
use crate::resolve::{ItemKind, ResolvedType, ResolvedValue};
use crate::syntax::Spanned;
use crate::types::TypeId;
use crate::types::path_value::resolve_path_base_local;
use crate::types::result::TypeError;

use super::super::TypeChecker;

impl<'a> TypeChecker<'a> {
    pub(in crate::types::checker) fn type_id_for_path(
        &mut self,
        span: crate::syntax::SpanInfo,
        path: &Spanned<Path>,
    ) -> Option<TypeId> {
        if path.node.segments.len() == 1 {
            let field_name = path.node.segments[0].node.name.node.name.as_str();
            if let Some(ResolvedValue::Local(local_id)) = self.resolved_value_at(span)
                && let Some(receiver_item) = self.current_receiver_item_id
                && self.local_types.get(&local_id).and_then(|type_id| self.named_item_id(*type_id))
                    == Some(receiver_item)
                && let Some(field_type) =
                    self.struct_fields.get(&receiver_item).and_then(|fields| fields.get(field_name))
            {
                return Some(*field_type);
            }
        }
        if path.node.segments.len() > 1 {
            return self.type_struct_field_path(span, path);
        }
        match self.resolved_value_at(span) {
            Some(ResolvedValue::Local(local)) => self.local_types.get(&local).copied().or_else(|| {
                self.errors.push(TypeError::UnknownValueType { span });
                None
            }),
            Some(ResolvedValue::Item(_)) => {
                self.errors.push(TypeError::UnknownValueType { span });
                None
            }
            None => {
                self.errors.push(TypeError::UnknownValueType { span });
                None
            }
        }
    }

    fn type_struct_field_path(&mut self, span: crate::syntax::SpanInfo, path: &Spanned<Path>) -> Option<TypeId> {
        let segments = &path.node.segments;
        let source_path = self.current_source_path.as_ref();
        let first_name = segments.first()?.node.name.node.name.as_str();
        let Some(local_id) = resolve_path_base_local(self.resolution, span, first_name, source_path) else {
            self.errors.push(TypeError::UnknownValueType { span });
            return None;
        };
        let Some(mut current_type) = self.local_types.get(&local_id).copied() else {
            self.errors.push(TypeError::UnknownValueType { span });
            return None;
        };
        for segment in segments.iter().skip(1) {
            let field_name = segment.node.name.node.name.clone();
            let Some(item_id) = self.named_item_id(current_type) else {
                self.errors.push(TypeError::InvalidMemberTarget { span: segment.span });
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
                self.errors.push(TypeError::UnknownStructType { span: segment.span });
                return None;
            };
            let Some(field_type) = fields.get(&field_name) else {
                self.errors.push(TypeError::UnknownStructField { span: segment.span, name: field_name });
                return None;
            };
            let mapping = self.generic_mapping_for_type_id(current_type);
            current_type =
                if mapping.is_empty() { *field_type } else { self.substitute_type_id(*field_type, &mapping) };
        }
        Some(current_type)
    }

    pub(in crate::types::checker) fn type_id_for_enum_path(
        &mut self,
        _span: crate::syntax::SpanInfo,
        path: &Spanned<crate::syntax::EnumPath>,
    ) -> Option<TypeId> {
        let type_span = path.node.type_path.span;
        match self.resolved_type_at(type_span) {
            Some(ResolvedType::Item(item_id)) => self.named_types.get(&item_id).copied(),
            Some(ResolvedType::Generic(name)) => self.generic_params.get(&name).copied(),
            None => {
                let type_name =
                    path.node.type_path.node.segments.last().map(|segment| segment.node.name.node.name.as_str())?;
                self.item_id_for_name(type_name, ItemKind::Enum)
                    .and_then(|item_id| self.named_types.get(&item_id).copied())
            }
        }
    }
}
