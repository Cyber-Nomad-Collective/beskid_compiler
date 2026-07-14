use std::path::PathBuf;

use beskid_isle::{AstNodeKey, FunctionEmitter, LiteralKind, NodeFacts, NodeKind, StringInterner};
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

    fn string_literal(&self, key: AstNodeKey) -> Option<&str> {
        (key == self.key).then_some(self.text.as_str())
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
    ) -> Option<Value> {
        let symbol = format!("__test_string_{}", self.interned.len());
        let data = self
            .module
            .declare_data(&symbol, Linkage::Local, false, false)
            .ok()?;
        let mut description = DataDescription::new();
        description.define(text.as_bytes().to_vec().into_boxed_slice());
        self.module.define_data(data, &description).ok()?;
        let global = self.module.declare_data_in_func(data, builder.func);
        self.interned.push(text.to_owned());
        Some(
            builder
                .ins()
                .global_value(self.module.isa().pointer_type(), global),
        )
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
