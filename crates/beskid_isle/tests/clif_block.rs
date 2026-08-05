use beskid_isle::{AstNodeKey, IsleContext, NodeFacts, NodeKind, lower_expression};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, types};
use cranelift_codegen::settings;
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use target_lexicon::Triple;

struct ClifBlockFacts { block_node: AstNodeKey, body: &'static str }

impl NodeFacts for ClifBlockFacts {
    fn node_kind(&self, n: AstNodeKey) -> Option<NodeKind> {
        (n == self.block_node).then_some(NodeKind::ClifBlock)
    }
    fn clif_block_body(&self, n: AstNodeKey) -> Option<String> {
        (n == self.block_node).then_some(self.body.to_string())
    }
    fn scalar_type(&self, n: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        (n == self.block_node).then_some(types::F64)
    }
    fn function_parameters(&self, _: AstNodeKey) -> Option<Vec<beskid_isle::ParameterSlot>> {
        Some(vec![beskid_isle::ParameterSlot {
            slot: beskid_isle::LocalSlotId { owner_node: 0, index: 0 },
            value_type: types::F64,
        }])
    }
    fn integer_literal(&self, _: AstNodeKey) -> Option<i64> { None }
}

fn make_isa() -> std::sync::Arc<dyn cranelift_codegen::isa::TargetIsa> {
    let f = settings::Flags::new(settings::builder());
    cranelift_codegen::isa::lookup(Triple::host()).unwrap().finish(f).unwrap()
}
fn make_key(db: &BeskidDatabase, id: u32) -> AstNodeKey {
    AstNodeKey {
        unit: SourceUnitId::new(db, "/tmp/M.bd".into()),
        generation: SyntaxGenerationId(1),
        node: AstNodeId(id),
    }
}

#[test]
fn clif_block_call_emits_verified_clif() {
    let db = BeskidDatabase::default();
    let kn = make_key(&db, 10);
    let facts = ClifBlockFacts { block_node: kn, body: "call @sqrt(%0)" };
    let isa = make_isa();
    let mut func = Function::with_name_signature(
        cranelift_codegen::ir::UserFuncName::user(0, 0),
        Signature {
            params: vec![AbiParam::new(types::F64)],
            returns: vec![AbiParam::new(types::F64)],
            call_conv: isa.default_call_conv(),
        },
    );
    let mut ctx = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut func, &mut ctx);
        let e = b.create_block();
        b.append_block_params_for_function_params(e);
        b.switch_to_block(e);
        b.seal_block(e);
        let p = b.block_params(e)[0];
        let mut c = IsleContext::new(&mut b, &facts);
        c.function_param_values.push(p);
        let v = lower_expression(&mut c, kn).unwrap();
        b.ins().return_(&[v]);
        b.finalize();
    }
    verify_function(&func, isa.flags()).unwrap();
    assert!(func.display().to_string().contains("call"));
}

#[test]
fn clif_block_return_param_emits_verified_clif() {
    let db = BeskidDatabase::default();
    let kn = make_key(&db, 10);
    let facts = ClifBlockFacts { block_node: kn, body: "return %0" };
    let isa = make_isa();
    let mut func = Function::with_name_signature(
        cranelift_codegen::ir::UserFuncName::user(0, 0),
        Signature {
            params: vec![AbiParam::new(types::F64)],
            returns: vec![AbiParam::new(types::F64)],
            call_conv: isa.default_call_conv(),
        },
    );
    let mut ctx = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut func, &mut ctx);
        let e = b.create_block();
        b.append_block_params_for_function_params(e);
        b.switch_to_block(e);
        b.seal_block(e);
        let p = b.block_params(e)[0];
        let mut c = IsleContext::new(&mut b, &facts);
        c.function_param_values.push(p);
        let v = lower_expression(&mut c, kn).unwrap();
        b.ins().return_(&[v]);
        b.finalize();
    }
    verify_function(&func, isa.flags()).unwrap();
    assert!(func.display().to_string().contains("return"));
}

#[test]
fn clif_block_two_arg_call_emits_verified_clif() {
    let db = BeskidDatabase::default();
    let kn = make_key(&db, 10);
    let facts = ClifBlockFacts { block_node: kn, body: "call @atan2(%0, %1)" };
    let isa = make_isa();
    let mut func = Function::with_name_signature(
        cranelift_codegen::ir::UserFuncName::user(0, 0),
        Signature {
            params: vec![AbiParam::new(types::F64), AbiParam::new(types::F64)],
            returns: vec![AbiParam::new(types::F64)],
            call_conv: isa.default_call_conv(),
        },
    );
    let mut ctx = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut func, &mut ctx);
        let e = b.create_block();
        b.append_block_params_for_function_params(e);
        b.switch_to_block(e);
        b.seal_block(e);
        let p0 = b.block_params(e)[0];
        let p1 = b.block_params(e)[1];
        let mut c = IsleContext::new(&mut b, &facts);
        c.function_param_values.push(p0);
        c.function_param_values.push(p1);
        let v = lower_expression(&mut c, kn).unwrap();
        b.ins().return_(&[v]);
        b.finalize();
    }
    verify_function(&func, isa.flags()).unwrap();
    assert!(func.display().to_string().contains("call"));
}
