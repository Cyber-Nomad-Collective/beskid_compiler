//! Manifest builtin and Corelib service signatures mapped to semantic types.

use super::super::super::layouts::unique_assembled_type_in_module;
use super::super::super::*;
use super::*;

/// Exact source signature for a publicly callable ABI-v5 manifest builtin.
pub(in crate::semantic_contract) fn manifest_builtin_abi_signature(
    builtin: ManifestBuiltin,
) -> Result<ItemSignature, SemanticError> {
    let declaration = beskid_abi::generated::abi_v5_contract::ABI_V5_SOURCE_BUILTINS
        .iter()
        .find(|candidate| candidate.name == builtin.name && candidate.symbol == builtin.symbol)
        .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
    let parameters = declaration
        .params
        .iter()
        .map(|ty| manifest_type_to_semantic(ty))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
    let result = manifest_type_to_semantic(declaration.result)
        .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
    Ok(ItemSignature { parameters: parameters.into(), result })
}

fn manifest_type_to_semantic(ty: &str) -> Option<SemanticTypeId> {
    Some(match ty {
        "string" => SemanticTypeId::STRING,
        "pointer" => SemanticTypeId::POINTER,
        "usize" => SemanticTypeId::WORD,
        // ABI-v5's three supported targets are 64-bit. Signed native-width values
        // retain signed comparison/conversion behavior, unlike unsigned `word`.
        "isize" => SemanticTypeId::I64,
        "i8" => SemanticTypeId::I8,
        "i16" => SemanticTypeId::I16,
        "i32" => SemanticTypeId::I32,
        "i64" => SemanticTypeId::I64,
        "u8" => SemanticTypeId::U8,
        "u16" => SemanticTypeId::U16,
        "u32" => SemanticTypeId::U32,
        "u64" => SemanticTypeId::U64,
        "f32" => SemanticTypeId::F32,

        "f64" => SemanticTypeId::F64,
        "void" => SemanticTypeId::UNIT,
        "never" => SemanticTypeId::NEVER,
        _ => return None,
    })
}

pub(in crate::semantic_contract) fn builtin_type_to_semantic(
    ty: beskid_analysis::builtins::BuiltinType,
) -> Option<SemanticTypeId> {
    use beskid_analysis::builtins::BuiltinType;
    Some(match ty {
        BuiltinType::I8 => SemanticTypeId::I8,
        BuiltinType::I16 => SemanticTypeId::I16,
        BuiltinType::I64 => SemanticTypeId::I64,
        BuiltinType::U8 => SemanticTypeId::U8,
        BuiltinType::U16 => SemanticTypeId::U16,
        BuiltinType::F32 => SemanticTypeId::F32,
        BuiltinType::Isize => SemanticTypeId::I64,
        BuiltinType::Bool => SemanticTypeId::BOOL,
        BuiltinType::Char => SemanticTypeId::CHAR,
        BuiltinType::String => SemanticTypeId::STRING,
        BuiltinType::Ptr => SemanticTypeId::POINTER,
        BuiltinType::Usize => SemanticTypeId::WORD,
        BuiltinType::U64 => SemanticTypeId::U64,
        BuiltinType::U32 => SemanticTypeId::U32,
        BuiltinType::I32 => SemanticTypeId::I32,
        BuiltinType::F64 => SemanticTypeId::F64,
        BuiltinType::Unit => SemanticTypeId::UNIT,
        BuiltinType::Never => SemanticTypeId::NEVER,
        // A source type parameter has no fixed ABI type. Typed value services lower through
        // their exact Corelib service transport, never through a runtime-intrinsic signature.
        BuiltinType::TypeParameter => return None,
        // Managed arrays cross the call boundary as their header pointer.
        BuiltinType::TypeParameterArray | BuiltinType::Bytes => SemanticTypeId::POINTER,
    })
}
/// ABI facts for compiler-embedded Corelib service facades. These are deliberately available
/// only after [`CallLowering::CorelibService`] has proved the current source corpus; user source
/// that merely spells one of these names remains unauthorized and receives no import signature.
pub(in crate::semantic_contract) fn corelib_service_abi_signature(service: CorelibService) -> Option<ItemSignature> {
    let adapted = match service.name {
        "__panic_str" => Some((vec![SemanticTypeId::STRING], SemanticTypeId::NEVER)),
        "__str_from_bytes_utf8" => Some((vec![SemanticTypeId::POINTER], SemanticTypeId::STRING)),
        "__syscall_write" => Some((vec![SemanticTypeId::I64, SemanticTypeId::STRING], SemanticTypeId::I64)),
        "__syscall_write_bytes" => Some((vec![SemanticTypeId::I64, SemanticTypeId::POINTER], SemanticTypeId::I64)),
        _ => None,
    };
    if let Some((parameters, result)) = adapted {
        return Some(ItemSignature { parameters: parameters.into(), result });
    }
    let abi = beskid_abi::runtime_source::canonical_corelib_service_abi(service)?;
    let parameters = abi.parameters.into_iter().map(corelib_service_abi_type).collect::<Option<Vec<_>>>()?;
    let result = corelib_service_abi_type(abi.result)?;
    Some(ItemSignature { parameters: parameters.into(), result })
}

fn corelib_service_abi_type(ty: beskid_abi::runtime_source::CorelibServiceAbiType) -> Option<SemanticTypeId> {
    use beskid_abi::runtime_source::CorelibServiceAbiType;
    Some(match ty {
        CorelibServiceAbiType::Pointer => SemanticTypeId::POINTER,
        CorelibServiceAbiType::String => SemanticTypeId::STRING,
        CorelibServiceAbiType::Usize => SemanticTypeId::WORD,
        CorelibServiceAbiType::I64 => SemanticTypeId::I64,
        CorelibServiceAbiType::I32 => SemanticTypeId::I32,
        CorelibServiceAbiType::U32 => SemanticTypeId::U32,
        CorelibServiceAbiType::U64 => SemanticTypeId::U64,
        CorelibServiceAbiType::U8 => SemanticTypeId::U8,
        CorelibServiceAbiType::F64 => SemanticTypeId::F64,
        CorelibServiceAbiType::F32 => SemanticTypeId::F32,
        CorelibServiceAbiType::Void => SemanticTypeId::UNIT,
        CorelibServiceAbiType::Never => SemanticTypeId::NEVER,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_authorized_corelib_service_has_an_exact_source_level_abi_signature() {
        let target = beskid_abi::abi_v5::TargetMetadata::supported()
            .into_iter()
            .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
            .expect("linux target");
        let manifest = beskid_abi::abi_v5::AbiManifestV5::canonical_runtime(target);
        let capability = beskid_abi::runtime_source::canonical_corelib_service_capability(&manifest)
            .expect("Corelib service capability");

        for service in capability.services() {
            let signature = corelib_service_abi_signature(*service).unwrap_or_else(|| {
                panic!(
                    "authorized service {} in {} has no source-level ABI signature",
                    service.name, service.source_path
                )
            });
            if service.name == "__str_from_bytes_utf8" {
                assert_eq!(
                    signature.result,
                    SemanticTypeId::STRING,
                    "the native pointer result must retain the source-level string type"
                );
            }
        }
    }
}
