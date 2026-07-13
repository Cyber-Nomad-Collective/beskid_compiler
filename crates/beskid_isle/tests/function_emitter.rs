use std::path::PathBuf;
use std::str::FromStr;

use beskid_isle::{FunctionEmitter, LiteralKind, NodeFacts, NodeKind};
use beskid_queries::{AstNodeId, AstNodeKey, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{UserFuncName, types};
use cranelift_codegen::settings;
use target_lexicon::Triple;

#[test]
fn signatures_and_pointer_types_come_from_each_supported_isa() {
    for triple in [
        "x86_64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
    ] {
        let triple = Triple::from_str(triple).expect("supported triple syntax");
        let isa = cranelift_codegen::isa::lookup(triple.clone())
            .unwrap_or_else(|error| panic!("lookup {triple}: {error}"))
            .finish(settings::Flags::new(settings::builder()))
            .unwrap_or_else(|error| panic!("finish {triple}: {error}"));
        let emitter = FunctionEmitter::new(isa.as_ref());
        let signature = emitter.signature([], []);

        assert_eq!(signature.call_conv, isa.default_call_conv(), "{triple}");
        assert_eq!(emitter.pointer_type(), isa.pointer_type(), "{triple}");
    }
}

#[test]
fn emitter_finalizes_and_verifies_each_selected_body() {
    struct Facts(AstNodeKey);
    impl NodeFacts for Facts {
        fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
            (key == self.0).then_some(NodeKind::LiteralExpression)
        }

        fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
            (key == self.0).then_some(LiteralKind::Integer)
        }

        fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
            (key == self.0).then_some(9)
        }

        fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
            (key == self.0).then_some(types::I32)
        }
    }

    let triple = Triple::from_str("x86_64-unknown-linux-gnu").expect("triple");
    let isa = cranelift_codegen::isa::lookup(triple)
        .expect("x64 backend")
        .finish(settings::Flags::new(settings::builder()))
        .expect("x64 ISA");
    let emitter = FunctionEmitter::new(isa.as_ref());
    let db = BeskidDatabase::default();
    let key = AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/Main.bd")),
        generation: SyntaxGenerationId(1),
        node: AstNodeId(1),
    };
    let function = emitter
        .emit_expression(
            UserFuncName::user(0, 1),
            emitter.signature([], [types::I32]),
            &Facts(key),
            key,
        )
        .expect("verified function");

    assert!(function.display().to_string().contains("iconst.i32 9"));
}
