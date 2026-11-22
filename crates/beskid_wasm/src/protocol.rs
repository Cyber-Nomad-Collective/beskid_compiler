use std::collections::HashSet;

use anyhow::{Result, ensure};
use beskid_abi::runtime_source::browser_protocol::{Value, operation};
use cranelift_codegen::ir::{AbiParam, Signature, types::I64};

pub(crate) fn admit(name: &str, signature: &Signature, authorized: &HashSet<String>) -> Result<bool> {
    let Some(operation) = operation(name) else { return Ok(false) };
    ensure!(authorized.contains(name), "browser service {name} requires its compiler-authorized Corelib declaration");
    let scalar = AbiParam::new(I64);
    let parameters = [scalar];
    let results = usize::from(operation.result != Value::Unit);
    ensure!(
        signature.params.len() == operation.parameters.len()
            && signature.params.iter().all(|param| *param == scalar)
            && signature.returns == parameters[..results],
        "invalid CLIF signature for browser service {name}"
    );
    Ok(true)
}

pub(crate) fn signature(name: &str) -> Option<(&'static [wasm_encoder::ValType], &'static [wasm_encoder::ValType])> {
    const PARAMETERS: [wasm_encoder::ValType; 8] = [wasm_encoder::ValType::I64; 8];
    let operation = operation(name)?;
    let results = usize::from(operation.result != Value::Unit);
    Some((&PARAMETERS[..operation.parameters.len()], &PARAMETERS[..results]))
}
