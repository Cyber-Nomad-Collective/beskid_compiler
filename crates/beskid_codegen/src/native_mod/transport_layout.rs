//! Source identity recursion using the existing canonical aggregate/enum layout authorities.
use super::adapter_plan::*;
use crate::CodegenInput;
use anyhow::{Context, Result, bail};
use beskid_queries::{
    AggregateFieldShape, AggregateLayoutFact, AstNodeKey, EnumLayoutFact, EnumVariantLayoutFact,
    NativeModTransportBody, NativeModTransportType as SourceType, native_mod_transport_nominal,
};
use std::{collections::HashMap, sync::Arc};

pub(super) struct TransportLayouts {
    pub types: Vec<NativeAdapterType>,
    pub syntax_types: Vec<NativeSyntaxType>,
    pub arrays: Vec<crate::ArrayStaticPlan>,
    identities: HashMap<SourceType, u32>,
}
impl TransportLayouts {
    pub fn new() -> Self {
        Self { types: Vec::new(), syntax_types: Vec::new(), arrays: Vec::new(), identities: HashMap::new() }
    }
    pub fn issue(&mut self, input: &CodegenInput<'_>, context: AstNodeKey, ty: &SourceType) -> Result<u32> {
        self.issue_at(input, context, ty, 0)
    }
    fn issue_at(
        &mut self,
        input: &CodegenInput<'_>,
        context: AstNodeKey,
        ty: &SourceType,
        depth: usize,
    ) -> Result<u32> {
        if depth > 128 || self.types.len() >= 65536 {
            bail!("native transport layout budget exceeded");
        }
        if let Some(id) = self.identities.get(ty) {
            return Ok(*id);
        }
        let id = u32::try_from(self.types.len())?;
        self.identities.insert(ty.clone(), id);
        // The reserved slot permits recursive nominal graphs without recursively rebuilding them.
        self.types.push(NativeAdapterType {
            id,
            name: String::new(),
            kind: NativeAdapterKind::Scalar(beskid_queries::SemanticTypeId::UNIT),
        });
        let (name, kind) = match ty {
            SourceType::Scalar(scalar) => (format!("{scalar:?}"), NativeAdapterKind::Scalar(*scalar)),
            SourceType::Function { .. } => bail!("source function values cannot cross invocation transport"),
            SourceType::Array(element) => {
                let element_id = self.issue_at(input, context, element, depth + 1)?;
                let plan = input
                    .build_array_static_plan(context, element.abi_type(), 0, Some(&[id]))
                    .context("canonical array allocation metadata unavailable")?;
                let request = input
                    .abi_manifest()
                    .layouts
                    .iter()
                    .find(|layout| layout.name == "BeskidArrayAllocationRequest")
                    .context("canonical array request layout unavailable")?;
                let count_offset = request
                    .fields
                    .iter()
                    .find(|field| field.name == "length")
                    .context("array count field unavailable")?
                    .offset;
                // Same primitive array view as canonical ISLE collection load/store: data
                // at zero and length one target pointer word later (capacity follows).
                let pointer_bytes = u64::from(input.target().pointer_width) / 8;
                let kind = NativeAdapterKind::Array {
                    element: element_id,
                    stride: plan.stride,
                    data_offset: 0,
                    length_offset: pointer_bytes,
                    request_getter: format!("__beskid_mod_array_request_{id}"),
                    request_bytes: request.size,
                    count_offset,
                };
                self.arrays.push(plan);
                (format!("array_{element_id}"), kind)
            }
            SourceType::Nominal { declaration, .. } => {
                let nominal = native_mod_transport_nominal(input.database(), context, ty)?
                    .context("canonical nominal projection unavailable")?;
                let unit = input
                    .typed_program()
                    .assembly
                    .units
                    .iter()
                    .find(|unit| {
                        beskid_queries::SourceUnitId::new(input.database(), unit.path.clone()) == declaration.unit
                    })
                    .context("native nominal unit absent")?;
                if let Some(package) = input.typed_program().assembly.package_identities().for_source(&unit.path) {
                    if let Some(relative) = package.relative_source_path(&unit.path) {
                        if relative.starts_with("Beskid/Syntax/Nodes/") || relative == "Beskid/Syntax/Nodes.bd" {
                            super::authority::require_sdk_package(input, *declaration)?;
                            beskid_abi::sdk_source::verify_canonical_sdk_source(
                                &format!("src/{relative}"),
                                unit.source.as_bytes(),
                            )?;
                            self.syntax_types.push(NativeSyntaxType {
                                type_id: id,
                                schema_type: format!("Beskid.Syntax.Nodes.{}", nominal.name()),
                            });
                        }
                    }
                }
                let shape = |ty: &SourceType| match ty {
                    SourceType::Scalar(scalar) => AggregateFieldShape::Scalar(*scalar),
                    SourceType::Nominal { declaration, .. } => AggregateFieldShape::Nominal(*declaration),
                    _ => AggregateFieldShape::ManagedReference(ty.abi_type()),
                };
                match nominal.body() {
                    NativeModTransportBody::Record(fields) => {
                        let fact = AggregateLayoutFact {
                            fields: fields
                                .iter()
                                .map(|field| (Arc::from(field.name()), shape(field.ty())))
                                .collect::<Vec<_>>()
                                .into(),
                        };
                        let layout = input
                            .aggregate_object_layout_from_fact(*declaration, &fact)
                            .context("canonical record geometry unavailable")?;
                        let mut projected = Vec::new();
                        for (field, slot) in fields.iter().zip(layout.fields.iter()) {
                            projected.push(NativeAdapterField {
                                name: field.name().to_owned(),
                                type_id: self.issue_at(input, context, field.ty(), depth + 1)?,
                                offset: slot.field_offset,
                            });
                        }
                        (nominal.name().to_owned(), NativeAdapterKind::Record { fields: projected })
                    }
                    NativeModTransportBody::Enum(variants) => {
                        let fact = EnumLayoutFact {
                            variants: variants
                                .iter()
                                .map(|variant| EnumVariantLayoutFact {
                                    name: Arc::from(variant.name()),
                                    fields: variant
                                        .fields()
                                        .iter()
                                        .map(|field| (Arc::from(field.name()), shape(field.ty())))
                                        .collect::<Vec<_>>()
                                        .into(),
                                })
                                .collect::<Vec<_>>()
                                .into(),
                        };
                        let header = input
                            .abi_manifest()
                            .layouts
                            .iter()
                            .find(|layout| layout.name == "BeskidObjectHeader")
                            .context("object header unavailable")?;
                        let layout = fact
                            .scalar_payload_object_layout(input.target().pointer_width, header.size, header.alignment)
                            .context("canonical enum geometry unavailable")?;
                        let mut projected = Vec::new();
                        for (ordinal, (variant, slots)) in variants.iter().zip(layout.variants.iter()).enumerate() {
                            let mut fields = Vec::new();
                            for (field, slot) in variant.fields().iter().zip(slots.payload_fields.iter()) {
                                fields.push(NativeAdapterField {
                                    name: field.name().to_owned(),
                                    type_id: self.issue_at(input, context, field.ty(), depth + 1)?,
                                    offset: slot.map(|(_, offset)| offset).unwrap_or(0),
                                });
                            }
                            projected.push(NativeAdapterVariant {
                                name: variant.name().to_owned(),
                                tag: u64::try_from(ordinal)?,
                                fields,
                            });
                        }
                        (
                            nominal.name().to_owned(),
                            NativeAdapterKind::Enum {
                                tag_offset: layout.tag_offset,
                                tag_bytes: 8,
                                variants: projected,
                            },
                        )
                    }
                }
            }
        };
        self.types[id as usize] = NativeAdapterType { id, name, kind };
        Ok(id)
    }
}
