use crate::{AbiParamKind, AbiReturnKind};

use super::service_table::CorelibService;

/// One source-independent ABI slot selected for a source-authorized Corelib service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorelibServiceAbiType {
    Pointer,
    String,
    Usize,
    I64,
    I32,
    U32,
    U8,
    F64,
    Void,
    Never,
}

/// The unique native ABI shape behind a source-authorized Corelib service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorelibServiceAbi {
    pub parameters: Vec<CorelibServiceAbiType>,
    pub result: CorelibServiceAbiType,
}

/// Resolve a service adapter against the canonical ABI-v5 bindings.
///
/// Generated Corelib-service bindings, manifest source builtins, and soft builtins intentionally
/// share this one lookup. Conflicting target shapes or duplicate declarations fail closed.
pub fn canonical_corelib_service_abi(service: CorelibService) -> Option<CorelibServiceAbi> {
    canonical_corelib_service_abi_for_adapter(service.symbol)
}

/// Resolve the unique canonical ABI-v5 shape for an already source-authorized adapter symbol.
pub fn canonical_corelib_service_abi_for_adapter(symbol: &str) -> Option<CorelibServiceAbi> {
    let mut bindings = crate::generated::abi_v5_contract::ABI_V5_CORELIB_SERVICE_BINDINGS
        .iter()
        .filter(|binding| binding.adapter == symbol);
    if let Some(binding) = bindings.next() {
        if bindings.any(|candidate| candidate.params != binding.params || candidate.result != binding.result) {
            return None;
        }
        return Some(CorelibServiceAbi {
            parameters: binding.params.iter().copied().map(corelib_service_abi_type).collect::<Option<Vec<_>>>()?,
            result: corelib_service_abi_type(binding.result)?,
        });
    }

    // Manifest source builtins carry the exact export types (for example a word-sized `usize`
    // status). They take precedence over the coarser soft-builtin register classes so the
    // source-facing ABI and the import preflight select one identical shape.
    let mut declarations = crate::generated::abi_v5_contract::ABI_V5_SOURCE_BUILTINS
        .iter()
        .filter(|declaration| declaration.symbol == symbol);
    if let Some(declaration) = declarations.next() {
        if declarations.any(|candidate| candidate.params != declaration.params || candidate.result != declaration.result)
        {
            return None;
        }
        return Some(CorelibServiceAbi {
            parameters: declaration.params.iter().copied().map(corelib_service_abi_type).collect::<Option<Vec<_>>>()?,
            result: corelib_service_abi_type(declaration.result)?,
        });
    }

    let mut builtins = crate::all_builtin_specs().filter(|binding| binding.symbol == symbol);
    let binding = builtins.next()?;
    if builtins.any(|candidate| candidate.params != binding.params || candidate.returns != binding.returns) {
        return None;
    }
    Some(CorelibServiceAbi {
        parameters: binding.params.iter().copied().map(corelib_soft_builtin_parameter_type).collect(),
        result: corelib_soft_builtin_return_type(binding.returns),
    })
}

pub(super) fn corelib_service_abi_type(ty: &str) -> Option<CorelibServiceAbiType> {
    Some(match ty {
        "pointer" => CorelibServiceAbiType::Pointer,
        "string" => CorelibServiceAbiType::String,
        "usize" | "isize" => CorelibServiceAbiType::Usize,
        "i64" => CorelibServiceAbiType::I64,
        "i32" => CorelibServiceAbiType::I32,
        "u32" => CorelibServiceAbiType::U32,
        "u8" => CorelibServiceAbiType::U8,
        "f64" => CorelibServiceAbiType::F64,
        "void" => CorelibServiceAbiType::Void,
        "never" => CorelibServiceAbiType::Never,
        _ => return None,
    })
}

fn corelib_soft_builtin_parameter_type(ty: AbiParamKind) -> CorelibServiceAbiType {
    match ty {
        AbiParamKind::Ptr => CorelibServiceAbiType::Pointer,
        AbiParamKind::I64 => CorelibServiceAbiType::I64,
        AbiParamKind::F64 => CorelibServiceAbiType::F64,
    }
}

fn corelib_soft_builtin_return_type(ty: AbiReturnKind) -> CorelibServiceAbiType {
    match ty {
        AbiReturnKind::Void => CorelibServiceAbiType::Void,
        AbiReturnKind::Never => CorelibServiceAbiType::Never,
        AbiReturnKind::Ptr => CorelibServiceAbiType::Pointer,
        AbiReturnKind::I64 => CorelibServiceAbiType::I64,
        AbiReturnKind::I32 => CorelibServiceAbiType::I32,
        AbiReturnKind::F64 => CorelibServiceAbiType::F64,
    }
}

pub(super) fn corelib_binding_abi(
    binding: &crate::generated::abi_v5_contract::GeneratedCorelibServiceBinding,
) -> Option<CorelibServiceAbi> {
    Some(CorelibServiceAbi {
        parameters: binding.params.iter().copied().map(corelib_service_abi_type).collect::<Option<Vec<_>>>()?,
        result: corelib_service_abi_type(binding.result)?,
    })
}
