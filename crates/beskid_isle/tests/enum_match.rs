use std::path::PathBuf;

use beskid_isle::syntax_types::LiteralKind;
use beskid_isle::{
    AstNodeKey, EnumLayout, EnumVariantLayout, FieldLayout, FunctionEmissionError, FunctionEmitter, LoweringErrorKind,
    ManagedStructAllocation, MatchArmFact, MatchPayloadPatternFact, NodeFacts, NodeKind,
};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{Type, UserFuncName, types};
use cranelift_codegen::settings;
use target_lexicon::Triple;

#[derive(Clone, Copy)]
enum Arms {
    Exact,
    Wildcard,
    Missing,
    Duplicate,
    NestedExact,
    NestedMissing,
}

struct EnumFacts {
    nodes: [AstNodeKey; 7],
    pointer_type: Type,
    layout: EnumLayout,
    variant_index: u32,
    arms: Arms,
}

impl NodeFacts for EnumFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        if key == self.nodes[0] {
            Some(NodeKind::MatchExpression)
        } else if key == self.nodes[1] {
            Some(NodeKind::EnumLiteralExpression)
        } else if self.nodes[2..].contains(&key) {
            Some(NodeKind::LiteralExpression)
        } else {
            None
        }
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        self.nodes[2..].contains(&key).then_some(LiteralKind::Integer)
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        (key == self.nodes[0] && index == 0).then_some(self.nodes[1])
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        if key == self.nodes[2] {
            Some(42)
        } else if key == self.nodes[3] {
            Some(100)
        } else if key == self.nodes[4] {
            Some(200)
        } else if key == self.nodes[5] {
            Some(300)
        } else {
            None
        }
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<Type> {
        if key == self.nodes[1] {
            Some(self.pointer_type)
        } else if self.nodes.contains(&key) {
            Some(types::I32)
        } else {
            None
        }
    }

    fn enum_layout(&self, key: AstNodeKey) -> Option<EnumLayout> {
        (key == self.nodes[0] || key == self.nodes[1]).then(|| self.layout.clone())
    }

    fn enum_variant_index(&self, key: AstNodeKey) -> Option<u32> {
        (key == self.nodes[1]).then_some(self.variant_index)
    }

    fn managed_struct_allocation(&self, key: AstNodeKey) -> Option<ManagedStructAllocation> {
        (key == self.nodes[1])
            .then(|| ManagedStructAllocation { allocation_request_symbol: "__test_enum_allocation_request".into() })
    }

    fn enum_payloads(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (key == self.nodes[1] && !matches!(self.arms, Arms::NestedExact | Arms::NestedMissing))
            .then(|| vec![self.nodes[2]])
    }

    fn match_arms(&self, key: AstNodeKey) -> Option<Vec<MatchArmFact>> {
        if key != self.nodes[0] {
            return None;
        }
        Some(match self.arms {
            Arms::Exact => vec![
                MatchArmFact::variant(0, self.nodes[3]),
                MatchArmFact::variant_with_payload(
                    7,
                    self.nodes[4],
                    MatchPayloadPatternFact::Fields(vec![MatchPayloadPatternFact::Ignore]),
                ),
            ],
            Arms::Wildcard => vec![MatchArmFact::variant(0, self.nodes[3]), MatchArmFact::wildcard(self.nodes[5])],
            Arms::Missing => vec![MatchArmFact::variant(0, self.nodes[3])],
            Arms::Duplicate => vec![
                MatchArmFact::variant(0, self.nodes[3]),
                MatchArmFact::variant(0, self.nodes[4]),
                MatchArmFact::variant_with_payload(
                    7,
                    self.nodes[5],
                    MatchPayloadPatternFact::Fields(vec![MatchPayloadPatternFact::Ignore]),
                ),
            ],
            Arms::NestedExact => vec![
                MatchArmFact::variant_with_payload(
                    0,
                    self.nodes[3],
                    MatchPayloadPatternFact::Fields(vec![MatchPayloadPatternFact::Enum {
                        layout: nested_layout(),
                        discriminant: 0,
                        payload: Box::new(MatchPayloadPatternFact::Fields(vec![])),
                    }]),
                ),
                MatchArmFact::variant_with_payload(
                    0,
                    self.nodes[4],
                    MatchPayloadPatternFact::Fields(vec![MatchPayloadPatternFact::Enum {
                        layout: nested_layout(),
                        discriminant: 1,
                        payload: Box::new(MatchPayloadPatternFact::Fields(vec![])),
                    }]),
                ),
                MatchArmFact::variant(7, self.nodes[5]),
            ],
            Arms::NestedMissing => vec![
                MatchArmFact::variant_with_payload(
                    0,
                    self.nodes[3],
                    MatchPayloadPatternFact::Fields(vec![MatchPayloadPatternFact::Enum {
                        layout: nested_layout(),
                        discriminant: 0,
                        payload: Box::new(MatchPayloadPatternFact::Fields(vec![])),
                    }]),
                ),
                MatchArmFact::variant(7, self.nodes[5]),
            ],
        })
    }
}

fn valid_layout() -> EnumLayout {
    EnumLayout::new(
        8,
        2,
        FieldLayout::new(types::I32, 0),
        vec![EnumVariantLayout::new(0, vec![]), EnumVariantLayout::new(7, vec![Some(FieldLayout::new(types::I32, 4))])],
    )
}

fn nested_layout() -> EnumLayout {
    EnumLayout::new(
        4,
        2,
        FieldLayout::new(types::I32, 0),
        vec![EnumVariantLayout::new(0, vec![]), EnumVariantLayout::new(1, vec![])],
    )
}

fn nested_outer_layout(pointer_type: Type) -> EnumLayout {
    EnumLayout::new(
        16,
        3,
        FieldLayout::new(types::I32, 0),
        vec![
            EnumVariantLayout::new(0, vec![Some(FieldLayout::new(pointer_type, 8))]),
            EnumVariantLayout::new(7, vec![]),
        ],
    )
}

fn facts(pointer_type: Type, arms: Arms, variant_index: u32, layout: EnumLayout) -> EnumFacts {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Enum.bd"));
    let generation = SyntaxGenerationId(18);
    EnumFacts {
        nodes: std::array::from_fn(|index| AstNodeKey { unit, generation, node: AstNodeId(index as u32 + 1) }),
        pointer_type,
        layout,
        variant_index,
        arms,
    }
}

fn emit(arms: Arms, function_index: u32) -> String {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type(), arms, 1, valid_layout());
    let emitter = FunctionEmitter::new(isa.as_ref());
    let signature = emitter.signature([], [types::I32]);
    let function = emitter
        .emit_expression(UserFuncName::user(0, function_index), signature.clone(), &facts, facts.nodes[0])
        .expect("verified enum match");
    function.display().to_string()
}

#[test]
fn enum_literal_and_exhaustive_match_uses_managed_storage() {
    let clif = emit(Arms::Exact, 26);
    assert!(clif.contains("beskid_rt_v5_managed_object_allocate"), "{clif}");
    assert!(!clif.contains("stack_store"), "{clif}");
    assert!(clif.contains("load.i32"), "{clif}");
    assert!(clif.contains("brif"), "{clif}");
}

#[test]
fn wildcard_arm_makes_match_exhaustive_with_managed_storage() {
    let clif = emit(Arms::Wildcard, 27);
    assert!(clif.contains("beskid_rt_v5_managed_object_allocate"), "{clif}");
}

#[test]
fn non_exhaustive_match_is_an_exact_keyed_error() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type(), Arms::Missing, 1, valid_layout());
    let error = FunctionEmitter::new(isa.as_ref())
        .emit_expression(
            UserFuncName::user(0, 28),
            FunctionEmitter::new(isa.as_ref()).signature([], [types::I32]),
            &facts,
            facts.nodes[0],
        )
        .expect_err("missing variant must not lower");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.nodes[0]);
    assert_eq!(error.kind(), LoweringErrorKind::NonExhaustiveMatch);
}

#[test]
fn repeated_variant_arms_preserve_source_order() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type(), Arms::Duplicate, 1, valid_layout());
    let emitter = FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_expression(UserFuncName::user(0, 31), emitter.signature([], [types::I32]), &facts, facts.nodes[0])
        .expect("repeated variant arms are required for literal and nested-pattern fallthrough");
    let clif = function.display().to_string();
    let first = clif.find("iconst.i32 100").expect("first repeated arm body");
    let second = clif.find("iconst.i32 200").expect("second repeated arm body");
    assert!(first < second, "repeated arms must retain source order: {clif}");
}

#[test]
fn collectively_exhaustive_nested_enum_variants_cover_the_outer_payload() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type(), Arms::NestedExact, 1, nested_outer_layout(isa.pointer_type()));
    let emitter = FunctionEmitter::new(isa.as_ref());
    emitter
        .emit_expression(UserFuncName::user(0, 32), emitter.signature([], [types::I32]), &facts, facts.nodes[0])
        .expect("all nested nominal variants collectively cover the outer enum payload");
}

#[test]
fn missing_nested_enum_variant_keeps_the_outer_match_non_exhaustive() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type(), Arms::NestedMissing, 1, nested_outer_layout(isa.pointer_type()));
    let emitter = FunctionEmitter::new(isa.as_ref());
    let error = emitter
        .emit_expression(UserFuncName::user(0, 33), emitter.signature([], [types::I32]), &facts, facts.nodes[0])
        .expect_err("an uncovered nested nominal variant must remain non-exhaustive");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.kind(), LoweringErrorKind::NonExhaustiveMatch);
}

struct UnitMatchFacts {
    nodes: [AstNodeKey; 4],
    pointer_type: Type,
}

impl NodeFacts for UnitMatchFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        match key {
            key if key == self.nodes[0] => Some(NodeKind::MatchExpression),
            key if key == self.nodes[1] => Some(NodeKind::EnumLiteralExpression),
            key if key == self.nodes[2] || key == self.nodes[3] => Some(NodeKind::BlockExpression),
            _ => None,
        }
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        (key == self.nodes[0] && index == 0).then_some(self.nodes[1])
    }

    fn statement_count(&self, key: AstNodeKey) -> Option<u8> {
        (key == self.nodes[2] || key == self.nodes[3]).then_some(0)
    }

    fn integer_literal(&self, _key: AstNodeKey) -> Option<i64> {
        None
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<Type> {
        (key == self.nodes[1]).then_some(self.pointer_type)
    }

    fn enum_layout(&self, key: AstNodeKey) -> Option<EnumLayout> {
        (key == self.nodes[0] || key == self.nodes[1]).then(valid_layout)
    }

    fn enum_variant_index(&self, key: AstNodeKey) -> Option<u32> {
        (key == self.nodes[1]).then_some(0)
    }

    fn managed_struct_allocation(&self, key: AstNodeKey) -> Option<ManagedStructAllocation> {
        (key == self.nodes[1]).then(|| ManagedStructAllocation {
            allocation_request_symbol: "__test_unit_enum_allocation_request".into(),
        })
    }

    fn match_arms(&self, key: AstNodeKey) -> Option<Vec<MatchArmFact>> {
        (key == self.nodes[0]).then(|| {
            vec![
                MatchArmFact::variant(0, self.nodes[2]),
                MatchArmFact::variant_with_payload(
                    7,
                    self.nodes[3],
                    MatchPayloadPatternFact::Fields(vec![MatchPayloadPatternFact::Ignore]),
                ),
            ]
        })
    }
}

#[test]
fn statement_context_enum_match_lowers_unit_arm_blocks() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/UnitEnum.bd"));
    let facts = UnitMatchFacts {
        nodes: std::array::from_fn(|index| AstNodeKey {
            unit,
            generation: SyntaxGenerationId(19),
            node: AstNodeId(index as u32 + 1),
        }),
        pointer_type: isa.pointer_type(),
    };
    let emitter = FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_statement(UserFuncName::user(0, 32), emitter.signature([], []), &facts, facts.nodes[0])
        .expect("unit match statement lowers through dedicated dispatch");
    let clif = function.display().to_string();
    assert!(clif.contains("brif"), "{clif}");
}

#[test]
fn duplicate_enum_discriminant_is_an_exact_layout_error() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let invalid = EnumLayout::new(
        4,
        2,
        FieldLayout::new(types::I32, 0),
        vec![EnumVariantLayout::new(0, vec![]), EnumVariantLayout::new(0, vec![])],
    );
    let facts = facts(isa.pointer_type(), Arms::Exact, 0, invalid);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let error = emitter
        .emit_expression(UserFuncName::user(0, 29), emitter.signature([], [isa.pointer_type()]), &facts, facts.nodes[1])
        .expect_err("duplicate discriminants invalidate semantic layout");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.nodes[1]);
    assert_eq!(error.kind(), LoweringErrorKind::InvalidEnumLayout);
}

#[test]
fn unknown_enum_variant_is_an_exact_keyed_error() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type(), Arms::Exact, 2, valid_layout());
    let emitter = FunctionEmitter::new(isa.as_ref());
    let error = emitter
        .emit_expression(UserFuncName::user(0, 30), emitter.signature([], [isa.pointer_type()]), &facts, facts.nodes[1])
        .expect_err("variant index must exist in semantic layout");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.nodes[1]);
    assert_eq!(error.kind(), LoweringErrorKind::InvalidEnumVariant(2));
}
