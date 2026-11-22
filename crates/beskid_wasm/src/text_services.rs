use std::collections::HashSet;

use anyhow::{Result, ensure};
use beskid_abi::{
    SYM_SYSCALL_WRITE,
    runtime_source::{CorelibServiceAbiType, canonical_corelib_service_abi_for_adapter},
};
use cranelift_codegen::ir::{AbiParam, Signature, types::I64};

pub(crate) const PANIC_MESSAGE: &str = "beskid_trap_message";
pub(crate) const PANIC_CODE: &str = "beskid_trap_code";
pub(crate) const FORMAT_F64: &str = "str_from_f64_bits";

pub(crate) fn uses_string_transfer(name: &str) -> bool {
    reads_string(name) || name == FORMAT_F64
}

pub(crate) fn reads_string(name: &str) -> bool {
    matches!(name, SYM_SYSCALL_WRITE | PANIC_MESSAGE)
}

pub(crate) fn is_host_import(name: &str, signature: &Signature, corelib: &HashSet<String>) -> Result<bool> {
    use CorelibServiceAbiType::{I64 as ManifestI64, Never, Pointer};
    let (parameters, result) = match name {
        SYM_SYSCALL_WRITE => (&[ManifestI64, Pointer][..], ManifestI64),
        PANIC_MESSAGE => (&[Pointer][..], Never),
        PANIC_CODE => (&[ManifestI64][..], Never),
        FORMAT_F64 => (&[ManifestI64][..], Pointer),
        _ => return Ok(false),
    };
    ensure!(corelib.contains(name), "browser service {name} requires its compiler-authorized Corelib declaration");
    ensure!(
        canonical_corelib_service_abi_for_adapter(name)
            .is_some_and(|abi| abi.parameters == parameters && abi.result == result),
        "canonical Corelib service {name} has a different text service ABI"
    );
    let scalar = AbiParam::new(I64);
    let returns = if result == Never { &[][..] } else { &[scalar][..] };
    ensure!(
        signature.params.len() == parameters.len()
            && signature.params.iter().all(|param| *param == scalar)
            && signature.returns == returns,
        "browser service {name} has an invalid signature"
    );
    Ok(true)
}
