use crate::resolve::{ItemId, ItemKind};
use crate::syntax::Spanned;
use crate::syntax::{EnumConstructorExpression, Expression, MemberExpression, PathExpression, StructLiteralExpression};
use crate::types::path_value::{first_field_segment_name, resolve_path_base_local};
use crate::types::result::{MethodReceiverSource, TypeError};
use crate::types::{TypeId, TypeInfo};

use super::super::TypeChecker;

impl<'a> TypeChecker<'a> {
    pub(super) fn type_struct_literal_expression(
        &mut self,
        literal: &Spanned<StructLiteralExpression>,
    ) -> Option<TypeId> {
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
            if self.field_inaccessible(item_id, &name) {
                self.errors.push(TypeError::InaccessibleStructField { span: field.node.name.span, name });
                continue;
            }
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
            if self.field_inaccessible(item_id, name) {
                self.errors.push(TypeError::InaccessibleStructField { span: literal.span, name: name.clone() });
                continue;
            }
            self.errors.push(TypeError::MissingStructField { span: literal.span, name: name.clone() });
        }

        Some(type_id)
    }

    pub(super) fn type_enum_constructor_expression(
        &mut self,
        constructor: &Spanned<EnumConstructorExpression>,
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
        let Some(declared_fields) = variants.get(&variant_name).cloned() else {
            self.errors.push(TypeError::UnknownEnumVariant {
                span: constructor.node.path.node.variant.span,
                name: variant_name,
            });
            return Some(type_id);
        };
        if constructor.node.args.len() != declared_fields.len() {
            self.errors.push(TypeError::EnumConstructorMismatch {
                span: constructor.span,
                expected: declared_fields.len(),
                actual: constructor.node.args.len(),
            });
            return Some(type_id);
        }

        let generic_names = self.generic_items.get(&item_id).cloned().unwrap_or_default();
        if generic_names.is_empty() || !self.generic_mapping_for_type_id(type_id).is_empty() {
            // Non-generic enum or an explicitly applied path: the declared fields, substituted by
            // the path's own arguments, are the expectation for each argument.
            let mapping = self.generic_mapping_for_type_id(type_id);
            let fields = self.substitute_all(&declared_fields, &mapping);
            self.type_constructor_arguments(constructor, &fields);
            return Some(type_id);
        }
        let expected_application = self.contextual_expected_type.filter(|expected| {
            self.named_item_id(*expected) == Some(item_id) && !self.generic_mapping_for_type_id(*expected).is_empty()
        });
        if let Some(expected) = expected_application {
            // `Result::Ok(Option::Some(0_i64))` returned as `Result<Option<i64>, E>`: the
            // contextual application fixes every generic argument, and each payload is typed
            // against its substituted field so nested generic variants receive their own
            // expected application (mirrors `type_argument_with_expected` for call arguments).
            let mapping = self.generic_mapping_for_type_id(expected);
            let fields = self.substitute_all(&declared_fields, &mapping);
            self.type_constructor_arguments(constructor, &fields);
            return Some(expected);
        }

        // No expected application: instantiate the generic enum from its payload types, the
        // same way a generic call infers its arguments. An enclosing context names another type
        // here, so it must not leak into the payloads.
        let previous = self.contextual_expected_type.take();
        let actuals: Vec<Option<TypeId>> = constructor.node.args.iter().map(|arg| self.type_expression(arg)).collect();
        self.contextual_expected_type = previous;
        let mut bindings = std::collections::HashMap::new();
        for (field, actual) in declared_fields.iter().zip(actuals.iter()) {
            if let Some(actual) = actual {
                self.bind_generic_parameters(*field, *actual, &mut bindings);
            }
        }
        let fields = self.substitute_all(&declared_fields, &bindings);
        for ((arg, expected), actual) in constructor.node.args.iter().zip(fields.iter()).zip(actuals.iter()) {
            if let Some(actual) = actual {
                self.require_same_type(arg.span, *expected, *actual);
            }
        }
        let arguments: Option<Vec<TypeId>> = generic_names.iter().map(|name| bindings.get(name).copied()).collect();
        match arguments {
            Some(args) => Some(self.type_table.intern(TypeInfo::Applied { base: item_id, args })),
            None => Some(type_id),
        }
    }

    fn substitute_all(&mut self, types: &[TypeId], mapping: &std::collections::HashMap<String, TypeId>) -> Vec<TypeId> {
        if mapping.is_empty() {
            return types.to_vec();
        }
        types.iter().map(|type_id| self.substitute_type_id(*type_id, mapping)).collect()
    }

    fn type_constructor_arguments(&mut self, constructor: &Spanned<EnumConstructorExpression>, fields: &[TypeId]) {
        for (arg, expected) in constructor.node.args.iter().zip(fields.iter()) {
            if let Some(actual) = self.type_argument_with_expected(arg, *expected) {
                self.require_same_type(arg.span, *expected, actual);
            }
        }
    }

    /// Bind the generic parameters a declared payload type mentions to the parts of the actual
    /// payload type at the same position. The first binding of a parameter wins; a later
    /// disagreement surfaces through `require_same_type` on the substituted field.
    fn bind_generic_parameters(
        &self,
        declared: TypeId,
        actual: TypeId,
        bindings: &mut std::collections::HashMap<String, TypeId>,
    ) {
        match (self.type_table.get(declared), self.type_table.get(actual)) {
            (Some(TypeInfo::GenericParam(name)), _) => {
                bindings.entry(name.clone()).or_insert(actual);
            }
            (
                Some(TypeInfo::Applied { base: declared_base, args: declared_args }),
                Some(TypeInfo::Applied { base: actual_base, args: actual_args }),
            ) if declared_base == actual_base && declared_args.len() == actual_args.len() => {
                let pairs: Vec<(TypeId, TypeId)> =
                    declared_args.iter().copied().zip(actual_args.iter().copied()).collect();
                for (declared_arg, actual_arg) in pairs {
                    self.bind_generic_parameters(declared_arg, actual_arg, bindings);
                }
            }
            (Some(TypeInfo::Array(declared_element)), Some(TypeInfo::Array(actual_element))) => {
                let (declared_element, actual_element) = (*declared_element, *actual_element);
                self.bind_generic_parameters(declared_element, actual_element, bindings);
            }
            _ => {}
        }
    }

    pub(super) fn type_member_expression(&mut self, member: &Spanned<MemberExpression>) -> Option<TypeId> {
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
        if self.field_inaccessible(item_id, &name) {
            self.errors.push(TypeError::InaccessibleStructField { span: member.node.member.span, name });
            return None;
        }
        let field_type = if mapping.is_empty() { *field_type } else { self.substitute_type_id(*field_type, &mapping) };
        Some(field_type)
    }

    /// A field without `pub` is private to the source unit that declares its type (E1211). The
    /// declaring unit, including the type's inline methods, keeps full access; an `extend type`
    /// body is judged by E1511 instead. An item without a known declaring source is never judged
    /// here: the semantic query gate remains the authority for those programs.
    pub(in crate::types::checker) fn field_inaccessible(&self, type_item: ItemId, field_name: &str) -> bool {
        if self.in_extend_type_body && self.current_receiver_item_id == Some(type_item) {
            return false;
        }
        let private_fields = self.private_field_index.get_or_init(|| {
            self.resolution
                .items
                .iter()
                .filter(|info| info.kind == ItemKind::Field && info.visibility != crate::syntax::Visibility::Public)
                .filter_map(|info| {
                    let parent = info.parent_id?;
                    let prefix = format!("{}::", self.resolution.items.get(parent.0)?.name);
                    Some((parent, info.name.strip_prefix(&prefix)?.to_string()))
                })
                .collect()
        });
        if !private_fields.contains(&(type_item, field_name.to_string())) {
            return false;
        }
        let Some(declared) = self.resolution.items.get(type_item.0).and_then(|info| info.source_path.as_ref()) else {
            return false;
        };
        let Some(current) = self.current_source_path.as_ref() else {
            return false;
        };
        !crate::paths::same_file(declared, current)
            && !canonical_private_field_admission(current, declared, field_name)
    }

    pub(super) fn is_event_member_expression(&self, member: &Spanned<MemberExpression>) -> bool {
        let Some(target_type) = self.node_types.get(&member.node.target.id).copied() else {
            return false;
        };
        let Some(item_id) = self.named_item_id(target_type) else {
            return false;
        };
        self.struct_event_fields.get(&item_id).and_then(|fields| fields.get(&member.node.member.node.name)).is_some()
    }

    pub(super) fn is_event_path_expression(&self, path_expr: &Spanned<PathExpression>) -> bool {
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
        callee: &Spanned<Expression>,
    ) -> Option<(MethodReceiverSource, TypeId, crate::resolve::ItemId, TypeId)> {
        match &callee.node {
            Expression::Member(member) => {
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
            Expression::Path(path_expr) => {
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

/// The exact compiler-owned private-field admissions of the semantic query gate
/// (`beskid_queries` `canonical_private_field_admission`), matched here by logical corelib path
/// only. The query gate proves exact source identity and stays the authority; this front-end
/// check only avoids reporting E1211 where the gate admits the access.
fn canonical_private_field_admission(current: &std::path::Path, declared: &std::path::Path, field_name: &str) -> bool {
    use beskid_abi::runtime_source::{
        CANONICAL_CORELIB_MUTEX_GUARD_SOURCE_PATH, CANONICAL_CORELIB_MUTEX_SOURCE_PATH,
        CANONICAL_FOUNDATION_DEADLINE_SOURCE_PATH, CANONICAL_FOUNDATION_PROCESS_SOURCE_PATH,
        CANONICAL_NETWORK_INTERNAL_SOURCE_PATH, CANONICAL_NETWORK_RESOURCES_SOURCE_PATH,
        CANONICAL_NETWORK_TCP_LISTENER_SOURCE_PATH, CANONICAL_NETWORK_TCP_STREAM_SOURCE_PATH,
        CANONICAL_NETWORK_UDP_SOCKET_SOURCE_PATH,
    };
    const ADMISSIONS: &[(&str, &str, &str)] = &[
        (CANONICAL_NETWORK_INTERNAL_SOURCE_PATH, CANONICAL_FOUNDATION_DEADLINE_SOURCE_PATH, "monotonicNanos"),
        (CANONICAL_FOUNDATION_PROCESS_SOURCE_PATH, CANONICAL_FOUNDATION_DEADLINE_SOURCE_PATH, "monotonicNanos"),
        (CANONICAL_NETWORK_RESOURCES_SOURCE_PATH, CANONICAL_NETWORK_TCP_LISTENER_SOURCE_PATH, "handle"),
        (CANONICAL_NETWORK_RESOURCES_SOURCE_PATH, CANONICAL_NETWORK_TCP_STREAM_SOURCE_PATH, "handle"),
        (CANONICAL_NETWORK_RESOURCES_SOURCE_PATH, CANONICAL_NETWORK_UDP_SOCKET_SOURCE_PATH, "handle"),
        (CANONICAL_CORELIB_MUTEX_SOURCE_PATH, CANONICAL_CORELIB_MUTEX_GUARD_SOURCE_PATH, "mutexHandle"),
    ];
    ADMISSIONS.iter().any(|(owner, declaration, field)| {
        *field == field_name && current.ends_with(owner) && declared.ends_with(declaration)
    })
}
