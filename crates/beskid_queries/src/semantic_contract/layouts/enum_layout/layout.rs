//! Enum storage layout and the tracked enum layout fact.

use super::super::super::*;
use super::super::explicit_local_declaration_type;
use super::*;
use crate::semantic_contract::typing::{managed_reference_kind_for_syntax_type, pattern_binding_fact_in_environment};

impl EnumLayoutFact {
    /// Compute the sole target-specific physical authority for ABI-v5 enum payloads.
    ///
    /// Pointer and scalar payloads use distinct union slots so the static pointer map traces every
    /// managed payload without scanning scalar bits.
    pub fn scalar_payload_object_layout(
        &self,
        pointer_width: u8,
        header_size: u64,
        header_alignment: u64,
    ) -> Option<EnumScalarPayloadObjectLayout> {
        #[derive(Clone, Copy)]
        struct StorageClass {
            ty: SemanticTypeId,
            size: u64,
            alignment: u64,
        }

        if header_size < 16 || header_alignment == 0 || !header_alignment.is_power_of_two() {
            return None;
        }
        let mut storage_by_position = Vec::<(Option<StorageClass>, Option<StorageClass>)>::new();
        let payloads = self
            .variants
            .iter()
            .map(|variant| {
                variant
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(position, (_, shape))| {
                        if matches!(shape, AggregateFieldShape::Scalar(SemanticTypeId::UNIT)) {
                            return Some(None);
                        }
                        let ty = match shape {
                            AggregateFieldShape::Scalar(ty) => *ty,
                            AggregateFieldShape::Nominal(_) => SemanticTypeId::POINTER,
                        };
                        let layout = ty.scalar_abi_layout(pointer_width)?;
                        if storage_by_position.len() <= position {
                            storage_by_position.resize(position + 1, (None, None));
                        }
                        let (scalar_storage, pointer_storage) = &mut storage_by_position[position];
                        let storage = if layout.is_pointer { pointer_storage } else { scalar_storage };
                        if storage.is_none_or(|current| {
                            layout.size > current.size
                                || (layout.size == current.size && layout.alignment > current.alignment)
                        }) {
                            *storage = Some(StorageClass { ty, size: layout.size, alignment: layout.alignment });
                        }
                        Some(Some((ty, layout.is_pointer)))
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .collect::<Option<Vec<_>>>()?;

        let tag_offset = align_to_layout(header_size, 4)?;
        let mut end = tag_offset.checked_add(4)?;
        let mut object_alignment = header_alignment.max(4);
        let mut storage_fields = Vec::with_capacity(storage_by_position.len() * 2);
        let mut offsets_by_position = Vec::with_capacity(storage_by_position.len());
        let mut pointer_map_offsets = Vec::new();
        for (scalar_storage, pointer_storage) in storage_by_position {
            let scalar_offset = append_storage(
                scalar_storage.map(|storage| (storage.ty, storage.size, storage.alignment)),
                &mut end,
                &mut object_alignment,
                &mut storage_fields,
            )?;
            let pointer_offset = append_storage(
                pointer_storage.map(|storage| (storage.ty, storage.size, storage.alignment)),
                &mut end,
                &mut object_alignment,
                &mut storage_fields,
            )?;
            pointer_map_offsets.extend(pointer_offset);
            offsets_by_position.push((scalar_offset, pointer_offset));
        }
        let variants = payloads
            .into_iter()
            .map(|payloads| {
                let payload_fields = payloads
                    .into_iter()
                    .enumerate()
                    .map(|(position, payload)| {
                        let Some((ty, is_pointer)) = payload else {
                            return Some(None);
                        };
                        let (scalar_offset, pointer_offset) = offsets_by_position[position];
                        let offset = if is_pointer { pointer_offset? } else { scalar_offset? };
                        Some(Some((ty, offset)))
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(EnumScalarPayloadVariantLayout { payload_fields: payload_fields.into() })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(EnumScalarPayloadObjectLayout {
            object_size: align_to_layout(end, object_alignment)?,
            object_alignment,
            tag_offset,
            storage_fields: storage_fields.into(),
            pointer_map_offsets: pointer_map_offsets.into(),
            variants: variants.into(),
        })
    }
}

fn append_storage(
    storage: Option<(SemanticTypeId, u64, u64)>,
    end: &mut u64,
    object_alignment: &mut u64,
    fields: &mut Vec<(SemanticTypeId, u64)>,
) -> Option<Option<u64>> {
    let Some((ty, size, alignment)) = storage else {
        return Some(None);
    };
    *end = align_to_layout(*end, alignment)?;
    let offset = *end;
    *end = end.checked_add(size)?;
    *object_alignment = (*object_alignment).max(alignment);
    fields.push((ty, offset));
    Some(Some(offset))
}

fn align_to_layout(value: u64, alignment: u64) -> Option<u64> {
    (alignment > 0 && alignment.is_power_of_two()).then_some(())?;
    value.checked_add(alignment - 1).map(|value| value & !(alignment - 1))
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn enum_layout_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<EnumLayoutFact> {
    with_node(db, syntax, key, |program, index, node| {
        if node.of::<beskid_analysis::syntax::ScopedUseStatement>().is_some() {
            return Some(scoped_cleanup(db, key).and_then(|fact| {
                fact.and_then(|fact| fact.enclosing_layout)
                    .ok_or_else(|| SemanticError::unavailable("scoped_cleanup_layout"))
            }));
        }
        if let Some(definition) = node.of::<beskid_analysis::syntax::EnumDefinition>() {
            return Some(enum_layout_from_definition(db, program, index, key, definition, None));
        }
        node.of::<beskid_analysis::syntax::EnumConstructorExpression>()
            .map(|constructor| {
                let type_path = contextual_enum_constructor_type_path(db, program, index, key, constructor)
                    .unwrap_or_else(|| constructor.path.node.type_path.node.clone());
                instantiated_enum_layout_for_path(db, key, &type_path)
            })
            .or_else(|| {
                node.of::<beskid_analysis::syntax::TryExpression>()
                    .map(|_| Ok(try_expression_fact_for_node(db, program, index, key, node)?.operand_layout))
            })
    })?
    .transpose()
}
