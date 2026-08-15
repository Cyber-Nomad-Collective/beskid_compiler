use super::*;

fn layout(payloads: &[Option<SemanticTypeId>]) -> EnumLayoutFact {
    EnumLayoutFact {
        variants: payloads
            .iter()
            .enumerate()
            .map(|(index, payload)| EnumVariantLayoutFact {
                name: Arc::from(format!("Variant{index}")),
                fields: payload
                    .map(|ty| Arc::from([(Arc::from("value"), AggregateFieldShape::Scalar(ty))]))
                    .unwrap_or_else(|| Arc::from([])),
            })
            .collect::<Vec<_>>()
            .into(),
    }
}

#[test]
fn scalar_payload_layout_is_target_correct() {
    let source = layout(&[None, Some(SemanticTypeId::WORD)]);

    let target32 = source.scalar_payload_object_layout(32, 16, 8).expect("32-bit layout");
    assert_eq!(target32.tag_offset, 16);
    assert_eq!(target32.storage_fields.as_ref(), &[(SemanticTypeId::WORD, 20)]);
    assert_eq!(target32.object_size, 24);
    assert_eq!(target32.object_alignment, 8);
    assert_eq!(target32.variants[1].payload_offset, Some(20));

    let target64 = source.scalar_payload_object_layout(64, 16, 8).expect("64-bit layout");
    assert_eq!(target64.tag_offset, 16);
    assert_eq!(target64.storage_fields.as_ref(), &[(SemanticTypeId::WORD, 24)]);
    assert_eq!(target64.object_size, 32);
    assert_eq!(target64.object_alignment, 8);
}

#[test]
fn scalar_payload_layout_tracks_pointer_slots() {
    let physical = layout(&[None, Some(SemanticTypeId::STRING)])
        .scalar_payload_object_layout(32, 16, 8)
        .expect("pointer payload layout");

    assert_eq!(physical.storage_fields.as_ref(), &[(SemanticTypeId::STRING, 20)]);
    assert_eq!(physical.pointer_map_offsets.as_ref(), &[20]);
    assert_eq!(physical.variants[0].payload_type, None);
    assert_eq!(physical.variants[1].payload_type, Some(SemanticTypeId::STRING));
}

#[test]
fn scalar_payload_layout_rejects_inexact_shapes() {
    let source = EnumLayoutFact {
        variants: Arc::from([EnumVariantLayoutFact {
            name: Arc::from("Pair"),
            fields: Arc::from([
                (Arc::from("left"), AggregateFieldShape::Scalar(SemanticTypeId::I32)),
                (Arc::from("right"), AggregateFieldShape::Scalar(SemanticTypeId::I32)),
            ]),
        }]),
    };

    assert_eq!(source.scalar_payload_object_layout(64, 16, 8), None);
    assert_eq!(layout(&[Some(SemanticTypeId::I64)]).scalar_payload_object_layout(16, 16, 8), None);
}

#[test]
fn scalar_payload_layout_separates_mixed_pointer_and_scalar_slots() {
    let source = layout(&[Some(SemanticTypeId::STRING), Some(SemanticTypeId::I64)]);
    let physical = source.scalar_payload_object_layout(64, 16, 8).expect("mixed payload layout");

    assert_eq!(physical.storage_fields.as_ref(), &[(SemanticTypeId::I64, 24), (SemanticTypeId::STRING, 32)]);
    assert_eq!(physical.pointer_map_offsets.as_ref(), &[32]);
    assert_eq!(physical.variants[0].payload_offset, Some(32));
    assert_eq!(physical.variants[1].payload_offset, Some(24));
    assert_eq!(physical.object_size, 40);
}
