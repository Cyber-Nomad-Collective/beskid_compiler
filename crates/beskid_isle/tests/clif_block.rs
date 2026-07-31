use beskid_isle::{AstNodeKey, IsleContext, NodeFacts, NodeKind, lower_expression};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, types};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings;
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use target_lexicon::Triple;

struct ClifBlockFacts {
    block_node: AstNodeKey,
    body: &'static str,
}

impl NodeFacts for ClifBlockFacts {
    fn node_kind(&self, node: AstNodeKey) -> Option<NodeKind> {
        (node == self.block_node).then_some(NodeKind::ClifBlock)
    }
    fn clif_block_body(&self, node: AstNodeKey) -> Option<String> {
        (node == self.block_node).then_some(self.body.to_string())
    }
    fn scalar_type(&self, node: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        (node == self.block_node).then_some(types::F64)
    }
    fn function_parameters(&self, _node: AstNodeKey) -> Option<Vec<beskid_isle::ParameterSlot>> {
        Some(vec![beskid_isle::ParameterSlot {
            slot: beskid_isle::LocalSlotId { owner_node: 0, index: 0 },
            value_type: types::F64,
        }])
    }
    fn integer_literal(&self, _node: AstNodeKey) -> Option<i64> {
        None
    }
}

fn make_isa() -> std::sync::Arc<dyn cranelift_codegen::isa::TargetIsa> {
    let flags = settings::Flags::new(settings::builder());
    cranelift_codegen::isa::lookup(Triple::host()).expect("host ISA").finish(flags).expect("host flags")
}

fn make_node(db: &BeskidDatabase, id: u32) -> AstNodeKey {
    AstNodeKey {
        unit: SourceUnitId::new(db, std::path::PathBuf::from("/tmp/Math.bd")),
        generation: SyntaxGenerationId(1),
        node: AstNodeId(id),
    }
}

#[test]
fn clif_block_call_emits_verified_clif() {
    let db = BeskidDatabase::default();
    let block_node = make_node(&db, 10);
    let facts = ClifBlockFacts { block_node, body: "call @sqrt(%0)" };
    let isa = make_isa();
    let mut function = Function::with_name_signature(
        cranelift_codegen::ir::UserFuncName::user(0, 0),
        Signature { params: vec![AbiParam::new(types::F64)], returns: vec![AbiParam::new(types::F64)], call_conv: isa.default_call_conv() },
    );
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let param_value = builder.block_params(entry)[0];
        let mut context = IsleContext::new(&mut builder, &facts);
        context.function_param_values.push(param_value);
        let value = lower_expression(&mut context, block_node).expect("clif block lowering");
        builder.ins().return_(&[value]);
        builder.finalize();
    }
    verify_function(&function, isa.flags()).expect("valid stock CLIF");
    let clif = function.display().to_string();
    assert!(clif.contains("call"), "expected a call instruction: {clif}");
}

#[test]
fn clif_block_return_param_emits_verified_clif() {
    let db = BeskidDatabase::default();
    let block_node = make_node(&db, 10);
    let facts = ClifBlockFacts { block_node, body: "return %0" };
    let isa = make_isa();
    let mut function = Function::with_name_signature(
        cranelift_codegen::ir::UserFuncName::user(0, 0),
        Signature { params: vec![AbiParam::new(types::F64)], returns: vec![AbiParam::new(types::F64)], call_conv: isa.default_call_conv() },
    );
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let param_value = builder.block_params(entry)[0];
        let mut context = IsleContext::new(&mut builder, &facts);
        context.function_param_values.push(param_value);
        let value = lower_expression(&mut context, block_node).expect("clif block return lowering");
        builder.ins().return_(&[value]);
        builder.finalize();
    }
    verify_function(&function, isa.flags()).expect("valid stock CLIF");
    let clif = function.display().to_string();
    assert!(clif.contains("return"), "expected a return instruction: {clif}");
}

#[test]
fn clif_block_two_arg_call_emits_verified_clif() {
    let db = BeskidDatabase::default();
    let block_node = make_node(&db, 10);
    let facts = ClifBlockFacts { block_node, body: "call @atan2(%0, %1)" };
    let isa = make_isa();
    let mut function = Function::with_name_signature(
        cranelift_codegen::ir::UserFuncName::user(0, 0),
        Signature { params: vec![AbiParam::new(types::F64), AbiParam::new(types::F64)], returns: vec![AbiParam::new(types::F64)], call_conv: isa.default_call_conv() },
    );
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let params = builder.block_params(entry);
        let param0 = params[0];
        let param1 = params[1];
        let mut context = IsleContext::new(&mut builder, &facts);
        context.function_param_values.push(param0);
        context.function_param_values.push(param1);
        let value = lower_expression(&mut context, block_node).expect("clif block two-arg call lowering");
        builder.ins().return_(&[value]);
        builder.finalize();
    }
    verify_function(&function, isa.flags()).expect("valid stock CLIF");
    let clif = function.display().to_string();
    assert!(clif.contains("call"), "expected a call instruction: {clif}");
}
