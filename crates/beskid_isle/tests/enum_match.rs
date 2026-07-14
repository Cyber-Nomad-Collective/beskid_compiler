use std::path::PathBuf;

use beskid_isle::{
    AstNodeKey, EnumLayout, EnumVariantLayout, FieldLayout, FunctionEmissionError, FunctionEmitter,
    LiteralKind, LoweringErrorKind, MatchArmFact, NodeFacts, NodeKind,
};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{Type, UserFuncName, types};
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use target_lexicon::Triple;

#[derive(Clone, Copy)]
enum Arms {
    Exact,
    Wildcard,
    Missing,
    Duplicate,
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
        self.nodes[2..]
            .contains(&key)
            .then_some(LiteralKind::Integer)
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

    fn enum_payload(&self, key: AstNodeKey) -> Option<AstNodeKey> {
        (key == self.nodes[1]).then_some(self.nodes[2])
    }

    fn match_arms(&self, key: AstNodeKey) -> Option<Vec<MatchArmFact>> {
        if key != self.nodes[0] {
            return None;
        }
        Some(match self.arms {
            Arms::Exact => vec![
                MatchArmFact::variant(0, self.nodes[3]),
                MatchArmFact::variant(7, self.nodes[4]),
            ],
            Arms::Wildcard => vec![
                MatchArmFact::variant(0, self.nodes[3]),
                MatchArmFact::wildcard(self.nodes[5]),
            ],
            Arms::Missing => vec![MatchArmFact::variant(0, self.nodes[3])],
            Arms::Duplicate => vec![
                MatchArmFact::variant(0, self.nodes[3]),
                MatchArmFact::variant(0, self.nodes[4]),
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
        vec![
            EnumVariantLayout::new(0, None),
            EnumVariantLayout::new(7, Some(FieldLayout::new(types::I32, 4))),
        ],
    )
}

fn facts(pointer_type: Type, arms: Arms, variant_index: u32, layout: EnumLayout) -> EnumFacts {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Enum.bd"));
    let generation = SyntaxGenerationId(18);
    EnumFacts {
        nodes: std::array::from_fn(|index| AstNodeKey {
            unit,
            generation,
            node: AstNodeId(index as u32 + 1),
        }),
        pointer_type,
        layout,
        variant_index,
        arms,
    }
}

fn run(arms: Arms, function_index: u32) -> (i32, String) {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type(), arms, 1, valid_layout());
    let emitter = FunctionEmitter::new(isa.as_ref());
    let signature = emitter.signature([], [types::I32]);
    let function = emitter
        .emit_expression(
            UserFuncName::user(0, function_index),
            signature.clone(),
            &facts,
            facts.nodes[0],
        )
        .expect("verified enum match");
    let clif = function.display().to_string();
    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let function_id = module
        .declare_function("enum_match", Linkage::Local, &signature)
        .expect("declare");
    let mut context = module.make_context();
    context.func = function;
    module
        .define_function(function_id, &mut context)
        .expect("define");
    module.finalize_definitions().expect("finalize");
    let code = module.get_finalized_function(function_id);
    let run: extern "C" fn() -> i32 = unsafe { std::mem::transmute(code) };
    (run(), clif)
}

#[test]
fn enum_literal_and_exhaustive_match_emit_stock_clif_and_execute() {
    let (result, clif) = run(Arms::Exact, 26);
    assert_eq!(result, 200);
    assert!(clif.contains("stack_store"), "{clif}");
    assert!(clif.contains("load.i32"), "{clif}");
    assert!(clif.contains("brif"), "{clif}");
}

#[test]
fn wildcard_arm_makes_match_exhaustive_and_executes() {
    let (result, _) = run(Arms::Wildcard, 27);
    assert_eq!(result, 300);
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
fn duplicate_match_arm_is_an_exact_keyed_error() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = facts(isa.pointer_type(), Arms::Duplicate, 1, valid_layout());
    let emitter = FunctionEmitter::new(isa.as_ref());
    let error = emitter
        .emit_expression(
            UserFuncName::user(0, 31),
            emitter.signature([], [types::I32]),
            &facts,
            facts.nodes[0],
        )
        .expect_err("duplicate semantic match arms must not lower");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.nodes[0]);
    assert_eq!(error.kind(), LoweringErrorKind::InvalidMatchArms);
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
        vec![
            EnumVariantLayout::new(0, None),
            EnumVariantLayout::new(0, None),
        ],
    );
    let facts = facts(isa.pointer_type(), Arms::Exact, 0, invalid);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let error = emitter
        .emit_expression(
            UserFuncName::user(0, 29),
            emitter.signature([], [isa.pointer_type()]),
            &facts,
            facts.nodes[1],
        )
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
        .emit_expression(
            UserFuncName::user(0, 30),
            emitter.signature([], [isa.pointer_type()]),
            &facts,
            facts.nodes[1],
        )
        .expect_err("variant index must exist in semantic layout");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.nodes[1]);
    assert_eq!(error.kind(), LoweringErrorKind::InvalidEnumVariant(2));
}
