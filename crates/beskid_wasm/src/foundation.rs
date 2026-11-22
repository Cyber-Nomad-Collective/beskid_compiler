//! Scalar and UTF-8 services reached through the existing Foundation facades.
use anyhow::{Result, ensure};
use beskid_abi::runtime_source::{CorelibServiceAbiType as Abi, canonical_corelib_service_abi_for_adapter};
use cranelift_codegen::ir::{AbiParam, Signature, types};
use std::collections::HashSet;

pub(crate) const FILE_READ: &str = "beskid_rt_v5_fs_read_text";
pub(crate) const FILE_WRITE: &str = "beskid_rt_v5_fs_write_text";
pub(crate) const FILE_EXISTS: &str = "beskid_rt_v5_fs_exists";
pub(crate) const FILE_MKDIR: &str = "beskid_rt_v5_fs_mkdir";
pub(crate) const FILE_DELETE: &str = "beskid_rt_v5_fs_delete";
pub(crate) const FILES: [&str; 5] = [FILE_READ, FILE_WRITE, FILE_EXISTS, FILE_MKDIR, FILE_DELETE];

pub(crate) struct Service {
    pub id: &'static str,
    pub args: &'static [Abi],
    pub result: Abi,
}

/// IEEE bit projections are pure Wasm reinterpret instructions, including for NaN payloads.
pub(crate) fn float_bit_types(name: &str) -> Option<(types::Type, types::Type)> {
    use types::{F32, F64, I32, I64};
    match name {
        "beskid_rt_v5_float_to_bits32" => Some((F32, I32)),
        "beskid_rt_v5_float_from_bits32" => Some((I32, F32)),
        "beskid_rt_v5_float_to_bits64" => Some((F64, I64)),
        "beskid_rt_v5_float_from_bits64" => Some((I64, F64)),
        _ => None,
    }
}

pub(crate) fn service(name: &str) -> Option<Service> {
    use Abi::{I32, I64, Pointer, String};
    let (id, args, result): (_, &[_], _) = match name {
        "beskid_rt_v5_args_count" => ("arguments", &[], I64),
        beskid_abi::SYM_GC_COLLECT => ("testCollector", &[], Abi::Usize),
        "fiber_processor_count" => ("runtime", &[], Abi::Usize),
        "tty_winsize" => ("terminal", &[I64], I64),
        "beskid_rt_v5_args_get" => ("arguments", &[I64], String),
        "env_get" => ("environment", &[Pointer], Pointer),
        "env_set" => ("environment", &[Pointer, Pointer], I32),
        "env_getcwd" => ("environment", &[], Pointer),
        FILE_READ | FILE_WRITE => ("files", &[Pointer, Pointer], I32),
        FILE_EXISTS | FILE_MKDIR | FILE_DELETE => ("files", &[Pointer], I32),
        _ => return None,
    };
    Some(Service { id, args, result })
}

impl Service {
    pub fn returns_string(&self) -> bool { matches!(self.result, Abi::Pointer | Abi::String) }
    pub fn needs_strings(&self) -> bool { self.returns_string() || self.args.contains(&Abi::Pointer) }
}

pub(crate) fn admitted(name: &str, signature: &Signature, authorized: &HashSet<String>) -> Result<bool> {
    let Some(service) = service(name) else { return Ok(false) };
    ensure!(authorized.contains(name), "browser service {name} requires its compiler-authorized Corelib declaration");
    ensure!(canonical_corelib_service_abi_for_adapter(name).is_some_and(|abi| abi.parameters == service.args && abi.result == service.result),
        "incompatible canonical Corelib service {name}");
    let word = AbiParam::new(types::I64);
    let result = if service.result == Abi::I32 { AbiParam::new(types::I32) } else { word };
    ensure!(signature.params.len() == service.args.len() && signature.params.iter().all(|param| *param == word)
        && signature.returns == [result], "invalid CLIF signature for browser service {name}");
    Ok(true)
}
