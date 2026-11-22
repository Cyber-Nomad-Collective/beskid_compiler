//! Recover the source type of each traced slot using the canonical enum layout.
use std::sync::Arc;

use beskid_queries::{GenericSpecializationInstance, SourceNominalType, SourceSignatureType, source_enum_variant_fields};

use crate::{CodegenInput, aggregate_static::AggregateStaticPlan};

pub(super) fn enum_fields(
    input: &CodegenInput<'_>,
    plan: &AggregateStaticPlan,
    nominal: &SourceNominalType,
    specialization: Option<&GenericSpecializationInstance>,
) -> Option<Arc<[SourceSignatureType]>> {
    let mut fields: Vec<_> = plan.fields.iter().map(|field| SourceSignatureType::Scalar(field.abi_type)).collect();
    if !plan.pointer_map_offsets.is_empty() {
        let (physical, _) = input.enum_storage_layout(plan.literal, specialization)?;
        let source = source_enum_variant_fields(input.database(), nominal).ok()??;
        if physical.variants.len() != source.len() {
            return None;
        }
        let mut traced = std::collections::HashMap::new();
        for (variant, types) in physical.variants.iter().zip(source) {
            if variant.payload_fields.len() != types.len() {
                return None;
            }
            for (payload, ty) in variant.payload_fields.iter().zip(types) {
                if let Some((_, offset)) = payload
                    && physical.pointer_map_offsets.contains(offset)
                    && traced.insert(*offset, ty.clone()).is_some_and(|prior| prior != ty)
                {
                    return None;
                }
            }
        }
        for (field, ty) in plan.fields.iter().zip(&mut fields) {
            if physical.pointer_map_offsets.contains(&field.field_offset) {
                *ty = traced.remove(&field.field_offset)?;
            }
        }
    }
    Some(fields.into())
}
