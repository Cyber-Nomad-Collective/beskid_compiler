//! Static allocation metadata for the canonical empty value of a destructive read `move x.f`
//! (`language-meta--memory-model--memory-and-references`, "Destructive read of a field").
//!
//! The empty value has no source literal, so its descriptor, pointer map and allocation request
//! are keyed on the `move` node and the specialization of the item that contains it, from the
//! applied field type `beskid_queries::canonical_empty_value` proves. Module data collection
//! emits them with the literal plans; the ISLE facts adapter reads the same symbols.

use std::sync::Arc;

use beskid_isle::AstNodeKey;
use beskid_queries::{
    CanonicalEmptyValue, GenericSpecializationInstance, ManagedReferenceKind, SemanticTypeId, canonical_empty_value,
    generic_specialization_identity,
};

use crate::{
    CodegenInput,
    aggregate_static::{AggregateStaticField, AggregateStaticPlan},
    array_static::ArrayStaticPlan,
};

/// The static metadata of one canonical empty value: the empty array of the array and list forms
/// and the managed object of the `Option::None` and `List<T>` forms.
pub(crate) struct CanonicalEmptyStatics {
    pub(crate) value: CanonicalEmptyValue,
    pub(crate) array: Option<ArrayStaticPlan>,
    pub(crate) object: Option<AggregateStaticPlan>,
}

impl CodegenInput<'_> {
    /// The canonical empty metadata of the destructive read `key` inside `specialization`, or
    /// `None` when `key` is not a destructive read or its applied field type is not proven.
    pub(crate) fn canonical_empty_statics(
        &self,
        key: AstNodeKey,
        specialization: Option<&GenericSpecializationInstance>,
    ) -> Option<CanonicalEmptyStatics> {
        let value = canonical_empty_value(self.database(), key, specialization).ok().flatten()?;
        let identity = specialization.map(generic_specialization_identity);
        let empty_array = |element: SemanticTypeId, element_managed: ManagedReferenceKind| {
            let mut plan = self.build_array_static_plan(key, element, 0, identity.as_deref())?;
            if element_managed == ManagedReferenceKind::NativeOrScalar {
                // A native pointer element is not traced.
                plan.pointer_map_offsets = Arc::from([]);
            }
            Some(plan)
        };
        let (array, object) = match &value {
            CanonicalEmptyValue::EmptyArray { element, element_managed } => {
                (Some(empty_array(*element, *element_managed)?), None)
            }
            CanonicalEmptyValue::OptionNone { layout, .. } => {
                let header = self.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidObjectHeader")?;
                let physical =
                    layout.scalar_payload_object_layout(self.target().pointer_width, header.size, header.alignment)?;
                let fields = std::iter::once(AggregateStaticField {
                    abi_type: SemanticTypeId::I32,
                    field_offset: physical.tag_offset,
                })
                .chain(
                    physical
                        .storage_fields
                        .iter()
                        .map(|(abi_type, field_offset)| AggregateStaticField { abi_type: *abi_type, field_offset: *field_offset }),
                )
                .collect::<Vec<_>>();
                let symbol = self.canonical_empty_identity(key, "none", identity.as_deref())?;
                (
                    None,
                    Some(AggregateStaticPlan {
                        descriptor_flags: 0,
                        descriptor_getter: None,
                        literal: key,
                        descriptor_symbol: format!("__beskid_canonical_descriptor_{symbol}"),
                        pointer_map_symbol: format!("__beskid_canonical_pointer_map_{symbol}"),
                        allocation_request_symbol: format!("__beskid_canonical_request_{symbol}"),
                        object_size: physical.object_size,
                        object_alignment: physical.object_alignment,
                        pointer_map_offsets: physical.pointer_map_offsets,
                        fields: fields.into(),
                    }),
                )
            }
            CanonicalEmptyValue::EmptyList { declaration, layout, element, element_managed, .. } => {
                let object = self.aggregate_object_layout_from_fact(*declaration, layout)?;
                if !object.injected_fields.is_empty() {
                    return None;
                }
                let symbol = self.canonical_empty_identity(key, "list", identity.as_deref())?;
                (
                    Some(empty_array(*element, *element_managed)?),
                    Some(AggregateStaticPlan {
                        descriptor_flags: 0,
                        descriptor_getter: None,
                        literal: key,
                        descriptor_symbol: format!("__beskid_canonical_descriptor_{symbol}"),
                        pointer_map_symbol: format!("__beskid_canonical_pointer_map_{symbol}"),
                        allocation_request_symbol: format!("__beskid_canonical_request_{symbol}"),
                        object_size: object.object_size,
                        object_alignment: object.object_alignment,
                        pointer_map_offsets: object.pointer_map_offsets,
                        fields: object.fields,
                    }),
                )
            }
        };
        Some(CanonicalEmptyStatics { value, array, object })
    }

    /// `<namespace>_<form>_u<unit>_g<generation>_n<node>[_<specialization>]` for the static
    /// symbols of the canonical empty value of the `move` node `key`.
    fn canonical_empty_identity(&self, key: AstNodeKey, form: &str, specialization: Option<&[u32]>) -> Option<String> {
        let unit = self
            .typed_program()
            .assembly
            .units
            .iter()
            .position(|unit| beskid_queries::SourceUnitId::new(self.database(), unit.path.clone()) == key.unit)?;
        let namespace = self
            .artifact_namespace()
            .chars()
            .map(|character| if character.is_ascii_alphanumeric() { character } else { '_' })
            .collect::<String>();
        let mut identity =
            format!("{namespace}_{form}_u{unit}_g{}_n{}", self.generation_tag(key.generation), key.node.0);
        for component in specialization.into_iter().flatten() {
            identity.push('_');
            identity.push_str(&component.to_string());
        }
        Some(identity)
    }
}
