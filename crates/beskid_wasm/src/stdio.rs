//! Source-authorized byte streams. Reads suspend while the application supplies input.
use anyhow::{Result, ensure};
use beskid_abi::runtime_source::{CorelibServiceAbiType as Abi, canonical_corelib_service_abi_for_adapter};
use cranelift_codegen::ir::{AbiParam, Signature, types};
use std::collections::HashSet;

pub(crate) const READ: &str = "syscall_read";
pub(crate) const READ_BYTES: &str = "syscall_read_bytes";
pub(crate) const WRITE_BYTES: &str = "syscall_write_bytes";
pub(crate) const READS: [&str; 2] = [READ, READ_BYTES];

pub(crate) fn handles(name: &str) -> bool { matches!(name, READ | READ_BYTES | WRITE_BYTES) }

pub(crate) fn admitted(name: &str, signature: &Signature, authorized: &HashSet<String>) -> Result<bool> {
    if !handles(name) { return Ok(false); }
    ensure!(authorized.contains(name), "browser service {name} requires its compiler-authorized Corelib declaration");
    ensure!(canonical_corelib_service_abi_for_adapter(name).is_some_and(|abi|
        abi.parameters == [Abi::I32, Abi::Pointer, Abi::Usize] && abi.result == Abi::I64),
        "incompatible canonical byte stream service {name}");
    ensure!(signature.params == [AbiParam::new(types::I32), AbiParam::new(types::I64), AbiParam::new(types::I64)]
        && signature.returns == [AbiParam::new(types::I64)], "invalid CLIF signature for {name}");
    Ok(true)
}
