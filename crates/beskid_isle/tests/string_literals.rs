use std::path::PathBuf;
use std::sync::Arc;

use beskid_isle::{
    AstNodeKey, FunctionEmissionError, FunctionEmitter, LiteralKind, LoweringErrorKind, NodeFacts,
    NodeKind, StringInterner, StringMaterializationError,
};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{InstBuilder, UserFuncName, Value, types};
use cranelift_codegen::settings;
use cranelift_frontend::FunctionBuilder;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataDescription, Linkage, Module, default_libcall_names};
use target_lexicon::Triple;

struct StringFacts {
    key: AstNodeKey,
    text: String,
}

impl NodeFacts for StringFacts {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        (key == self.key).then_some(NodeKind::LiteralExpression)
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        (key == self.key).then_some(LiteralKind::String)
    }

    fn integer_literal(&self, _key: AstNodeKey) -> Option<i64> {
        None
    }

    fn string_literal(&self, key: AstNodeKey) -> Option<Arc<str>> {
        (key == self.key).then(|| Arc::from(self.text.as_str()))
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        (key == self.key).then_some(types::I64)
    }
}

struct ModuleStringInterner {
    module: JITModule,
    interned: Vec<String>,
}

impl StringInterner for ModuleStringInterner {
    fn intern(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        _key: AstNodeKey,
        text: &str,
    ) -> Result<Value, StringMaterializationError> {
        let symbol = format!("__test_string_{}", self.interned.len());
        let data = self
            .module
            .declare_data(&symbol, Linkage::Local, false, false)
            .map_err(|_| StringMaterializationError::Artifact("declare literal data"))?;
        let mut description = DataDescription::new();
        description.define(text.as_bytes().to_vec().into_boxed_slice());
        self.module
            .define_data(data, &description)
            .map_err(|_| StringMaterializationError::Artifact("define literal data"))?;
        let global = self.module.declare_data_in_func(data, builder.func);
        self.interned.push(text.to_owned());
        Ok(builder
            .ins()
            .global_value(self.module.isa().pointer_type(), global))
    }
}

struct FailingStringInterner;

impl StringInterner for FailingStringInterner {
    fn intern(
        &mut self,
        _builder: &mut FunctionBuilder<'_>,
        _key: AstNodeKey,
        _text: &str,
    ) -> Result<Value, StringMaterializationError> {
        Err(StringMaterializationError::DispatchEmission(
            "dispatch call result",
        ))
    }
}

#[test]
fn string_rule_interns_data_and_emits_verified_stock_clif() {
    let flags = settings::Flags::new(settings::builder());
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(flags)
        .expect("host flags");
    let mut interner = ModuleStringInterner {
        module: JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names())),
        interned: Vec::new(),
    };
    let db = BeskidDatabase::default();
    let key = AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/String.bd")),
        generation: SyntaxGenerationId(7),
        node: AstNodeId(1),
    };
    let facts = StringFacts {
        key,
        text: "Beskid".to_owned(),
    };
    let emitter = FunctionEmitter::new(isa.as_ref());
    let function = emitter
        .emit_expression_with_string_interner(
            UserFuncName::user(0, 10),
            emitter.signature([], [isa.pointer_type()]),
            &facts,
            key,
            &mut interner,
        )
        .expect("verified string rule");

    assert_eq!(interner.interned, ["Beskid"]);
    let clif = function.display().to_string();
    assert!(clif.contains("global_value"), "{clif}");
}

#[test]
fn string_materialization_failure_is_a_specific_lowering_error() {
    let flags = settings::Flags::new(settings::builder());
    let isa = cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(flags)
        .expect("host flags");
    let db = BeskidDatabase::default();
    let key = AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/StringFailure.bd")),
        generation: SyntaxGenerationId(8),
        node: AstNodeId(1),
    };
    let facts = StringFacts {
        key,
        text: "Beskid".to_owned(),
    };
    let error = FunctionEmitter::new(isa.as_ref())
        .emit_expression_with_string_interner(
            UserFuncName::user(0, 11),
            FunctionEmitter::new(isa.as_ref()).signature([], [isa.pointer_type()]),
            &facts,
            key,
            &mut FailingStringInterner,
        )
        .expect_err("string materialization errors must not collapse into missing facts");

    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error, got {error}");
    };
    assert_eq!(
        error.kind(),
        LoweringErrorKind::StringMaterialization(StringMaterializationError::DispatchEmission(
            "dispatch call result",
        )),
    );
}
