use std::collections::HashMap;
use std::path::PathBuf;

use beskid_isle::{
    AstNodeKey, CallImportError, CallImporter, CallKind, DirectCallee, FunctionEmissionError, FunctionEmitter,
    LiteralKind, LoweringErrorKind, NodeFacts, NodeKind,
};
use beskid_queries::{AstNodeId, BeskidDatabase, SemanticTypeId, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{AbiParam, FuncRef, Signature, UserFuncName, types};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::settings;
use cranelift_frontend::FunctionBuilder;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use target_lexicon::Triple;

extern "C" fn add_one(value: i32) -> i32 {
    value + 1
}

struct CallFacts {
    call: AstNodeKey,
    argument: AstNodeKey,
    callee: DirectCallee,
    signature: Signature,
    argument_kind: NodeKind,
    argument_type: cranelift_codegen::ir::Type,
    call_type: cranelift_codegen::ir::Type,
    canonical_constant: Option<i64>,
    primitive_conversion: Option<(SemanticTypeId, SemanticTypeId)>,
    semantic_argument_type: Option<SemanticTypeId>,
    semantic_call_type: Option<SemanticTypeId>,
}

fn direct_callee_key() -> AstNodeKey {
    let db = BeskidDatabase::default();
    AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/Callee.bd")),
        generation: SyntaxGenerationId(15),
        node: beskid_queries::AstNodeId(7),
    }
}

impl NodeFacts for CallFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        if key == self.call {
            Some(NodeKind::CallExpression)
        } else if key == self.argument {
            Some(self.argument_kind)
        } else {
            None
        }
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        (key == self.argument).then_some(LiteralKind::Integer)
    }

    fn call_kind(&self, key: AstNodeKey) -> Option<CallKind> {
        (key == self.call).then_some(if self.primitive_conversion.is_some() {
            CallKind::PrimitiveNumericConversion
        } else {
            CallKind::Direct
        })
    }

    fn primitive_numeric_conversion(&self, key: AstNodeKey) -> Option<(SemanticTypeId, SemanticTypeId)> {
        (key == self.call).then_some(self.primitive_conversion).flatten()
    }

    fn semantic_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        if key == self.argument {
            self.semantic_argument_type
        } else if key == self.call {
            self.semantic_call_type
        } else {
            None
        }
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        (key == self.argument && self.argument_kind == NodeKind::LiteralExpression).then_some(41)
    }

    fn constant_integer(&self, key: AstNodeKey) -> Option<i64> {
        (key == self.argument && self.argument_kind == NodeKind::PathExpression)
            .then_some(self.canonical_constant)
            .flatten()
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        if key == self.argument {
            Some(self.argument_type)
        } else if key == self.call {
            Some(self.call_type)
        } else {
            None
        }
    }

    fn canonical_runtime_constant_integer(&self, key: AstNodeKey) -> Option<i64> {
        (key == self.argument).then_some(self.canonical_constant).flatten()
    }

    fn direct_callee(&self, key: AstNodeKey) -> Option<DirectCallee> {
        (key == self.call).then_some(self.callee.clone())
    }

    fn call_signature(&self, key: AstNodeKey) -> Option<Signature> {
        (key == self.call).then_some(self.signature.clone())
    }

    fn call_arguments(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (key == self.call).then_some(vec![self.argument])
    }
}

struct KnownCallImporter {
    module: JITModule,
    expected: DirectCallee,
}

impl CallImporter for KnownCallImporter {
    fn import(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: DirectCallee,
        signature: &Signature,
    ) -> Result<FuncRef, CallImportError> {
        if callee != self.expected {
            return Err(CallImportError::UnknownCallee);
        }
        let function = self
            .module
            .declare_function("add_one", Linkage::Import, signature)
            .map_err(|_| CallImportError::UnknownCallee)?;
        Ok(self.module.declare_func_in_func(function, builder.func))
    }
}

fn call_facts(isa: &dyn TargetIsa, callee: DirectCallee) -> CallFacts {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/Call.bd"));
    let generation = SyntaxGenerationId(15);
    CallFacts {
        call: AstNodeKey { unit, generation, node: AstNodeId(1) },
        argument: AstNodeKey { unit, generation, node: AstNodeId(2) },
        callee,
        signature: Signature {
            params: vec![AbiParam::new(types::I32)],
            returns: vec![AbiParam::new(types::I32)],
            call_conv: isa.default_call_conv(),
        },
        argument_kind: NodeKind::LiteralExpression,
        argument_type: types::I32,
        call_type: types::I32,
        canonical_constant: None,
        primitive_conversion: None,
        semantic_argument_type: None,
        semantic_call_type: None,
    }
}

#[test]
fn primitive_numeric_conversion_rejects_a_fact_whose_semantic_type_does_not_match_the_syntax_type() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let mut facts = call_facts(isa.as_ref(), DirectCallee::item(direct_callee_key()));
    facts.call_type = types::I64;
    facts.primitive_conversion = Some((SemanticTypeId::WORD, SemanticTypeId::I64));
    facts.semantic_argument_type = Some(SemanticTypeId::I64);
    facts.semantic_call_type = Some(SemanticTypeId::I64);

    let emitter = FunctionEmitter::new(isa.as_ref());
    let error = emitter
        .emit_expression(UserFuncName::user(0, 23), emitter.signature([], [types::I64]), &facts, facts.call)
        .expect_err("a mismatched conversion fact must fail before generating CLIF");

    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.call);
    assert_eq!(error.kind(), LoweringErrorKind::MissingRuleOrFact);
}

fn importer(isa: std::sync::Arc<dyn TargetIsa>, expected: DirectCallee) -> KnownCallImporter {
    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    builder.symbol("add_one", add_one as *const u8);
    KnownCallImporter { module: JITModule::new(builder), expected }
}

#[test]
fn direct_call_imports_semantic_callee_and_executes() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let facts = call_facts(isa.as_ref(), DirectCallee::item(direct_callee_key()));
    let emitter = FunctionEmitter::new(isa.as_ref());
    let signature = emitter.signature([], [types::I32]);
    let mut importer = importer(isa.clone(), facts.callee.clone());
    let function = emitter
        .emit_expression_with_call_importer(
            UserFuncName::user(0, 18),
            signature.clone(),
            &facts,
            facts.call,
            &mut importer,
        )
        .expect("verified direct call");
    assert!(function.display().to_string().contains("call"));

    let function_id = importer.module.declare_function("caller", Linkage::Local, &signature).expect("declare caller");
    let mut context = importer.module.make_context();
    context.func = function;
    importer.module.define_function(function_id, &mut context).expect("define caller");
    importer.module.finalize_definitions().expect("finalize");
    let code = importer.module.get_finalized_function(function_id);
    let run: extern "C" fn() -> i32 = unsafe { std::mem::transmute(code) };
    assert_eq!(run(), 42);
}

#[test]
fn canonical_runtime_constant_materializes_at_direct_word_parameter_width() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let mut facts = call_facts(isa.as_ref(), DirectCallee::item(direct_callee_key()));
    let word = isa.pointer_type();
    facts.signature = Signature {
        params: vec![AbiParam::new(word)],
        returns: vec![AbiParam::new(word)],
        call_conv: isa.default_call_conv(),
    };
    facts.call_type = word;
    facts.argument_kind = NodeKind::PathExpression;
    facts.canonical_constant = Some(3480);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let mut importer = importer(isa.clone(), facts.callee.clone());
    let function = emitter
        .emit_expression_with_call_importer(
            UserFuncName::user(0, 20),
            emitter.signature([], [word]),
            &facts,
            facts.call,
            &mut importer,
        )
        .expect("compiler-owned constant must materialize at the direct ABI word width");
    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i64 3480") || clif.contains("iconst.i32 3480"), "{clif}");
}

#[test]
fn canonical_runtime_constant_direct_abi_materialization_rejects_negative_values() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let mut facts = call_facts(isa.as_ref(), DirectCallee::item(direct_callee_key()));
    let word = isa.pointer_type();
    facts.signature = Signature {
        params: vec![AbiParam::new(word)],
        returns: vec![AbiParam::new(word)],
        call_conv: isa.default_call_conv(),
    };
    facts.call_type = word;
    facts.argument_kind = NodeKind::PathExpression;
    facts.canonical_constant = Some(-1);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let mut importer = importer(isa.clone(), facts.callee.clone());
    let error = emitter
        .emit_expression_with_call_importer(
            UserFuncName::user(0, 21),
            emitter.signature([], [word]),
            &facts,
            facts.call,
            &mut importer,
        )
        .expect_err("negative constants must not silently become unsigned ABI words");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.call);
    assert_eq!(error.kind(), LoweringErrorKind::MissingRuleOrFact);
}

#[test]
fn canonical_runtime_literal_is_not_re_materialized_as_a_direct_word_argument() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let mut facts = call_facts(isa.as_ref(), DirectCallee::item(direct_callee_key()));
    let word = isa.pointer_type();
    facts.signature = Signature {
        params: vec![AbiParam::new(word)],
        returns: vec![AbiParam::new(word)],
        call_conv: isa.default_call_conv(),
    };
    facts.call_type = word;
    // Even a synthetic authority claim cannot widen a literal: only a path
    // resolving to a canonical module constant may use the contextual rule.
    facts.canonical_constant = Some(3480);
    let emitter = FunctionEmitter::new(isa.as_ref());
    let mut importer = importer(isa.clone(), facts.callee.clone());
    let error = emitter
        .emit_expression_with_call_importer(
            UserFuncName::user(0, 22),
            emitter.signature([], [word]),
            &facts,
            facts.call,
            &mut importer,
        )
        .expect_err("literals must not be implicitly widened by canonical constant authority");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.call);
    assert_eq!(error.kind(), LoweringErrorKind::MissingRuleOrFact);
}

#[test]
fn unknown_direct_callee_is_an_exact_keyed_error() {
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let unknown = DirectCallee::item(AstNodeKey {
        unit: SourceUnitId::new(&BeskidDatabase::default(), PathBuf::from("/tmp/Unknown.bd")),
        generation: SyntaxGenerationId(15),
        node: beskid_queries::AstNodeId(999),
    });
    let facts = call_facts(isa.as_ref(), unknown.clone());
    let emitter = FunctionEmitter::new(isa.as_ref());
    let mut importer = importer(isa.clone(), DirectCallee::item(direct_callee_key()));
    let error = emitter
        .emit_expression_with_call_importer(
            UserFuncName::user(0, 19),
            emitter.signature([], [types::I32]),
            &facts,
            facts.call,
            &mut importer,
        )
        .expect_err("unknown callees must not fall back to a guessed symbol");

    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.call);
    assert_eq!(error.kind(), LoweringErrorKind::UnknownCallee(unknown));
}

#[test]
fn source_callees_with_the_same_node_id_in_different_units_are_distinct() {
    let db = BeskidDatabase::default();
    let generation = SyntaxGenerationId(15);
    let left = AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/Left.bd")),
        generation,
        node: beskid_queries::AstNodeId(7),
    };
    let right = AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/Right.bd")),
        generation,
        node: beskid_queries::AstNodeId(7),
    };

    let symbols = HashMap::from([(DirectCallee::item(left), "Left"), (DirectCallee::item(right), "Right")]);

    assert_ne!(DirectCallee::item(left), DirectCallee::item(right));
    assert_eq!(symbols[&DirectCallee::item(left)], "Left");
    assert_eq!(symbols[&DirectCallee::item(right)], "Right");
}
