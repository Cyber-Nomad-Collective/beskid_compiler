//! Private native metadata accessors emitted into the same object as their static data.
use crate::{CodegenArtifact, ExportEntry, LoweredFunction};
use cranelift_codegen::{
    ir::{AbiParam, ExternalName, Function, GlobalValueData, InstBuilder, Signature},
    isa::TargetIsa,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

pub(super) fn append_request_getter(
    artifact: &mut CodegenArtifact,
    isa: &dyn TargetIsa,
    symbol: &str,
    request: &str,
) -> anyhow::Result<()> {
    anyhow::ensure!(crate::artifact::is_valid_link_name(symbol), "invalid metadata getter symbol");
    anyhow::ensure!(!artifact.functions.iter().any(|function| function.name == symbol), "duplicate metadata getter");
    let pointer = isa.pointer_type();
    let mut signature = Signature::new(isa.default_call_conv());
    signature.returns.push(AbiParam::new(pointer));
    let mut function = Function::with_name_signature(cranelift_codegen::ir::UserFuncName::testcase(symbol), signature);
    let mut context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut context);
        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);
        let data = builder.func.create_global_value(GlobalValueData::Symbol {
            name: ExternalName::testcase(request),
            offset: 0.into(),
            colocated: false,
            tls: false,
        });
        let address = builder.ins().symbol_value(pointer, data);
        builder.ins().return_(&[address]);
        builder.finalize(isa.frontend_config());
    }
    cranelift_codegen::verify_function(&function, isa).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    artifact.functions.push(LoweredFunction { name: symbol.to_owned(), function });
    artifact.exports.push(ExportEntry {
        beskid_name: symbol.to_owned(),
        exported_symbol: symbol.to_owned(),
        abi: "C".to_owned(),
    });
    Ok(())
}
