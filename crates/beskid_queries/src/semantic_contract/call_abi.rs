//! Focused ABI facts for callable syntax constructs.

use super::{ItemSignature, SemanticError, SemanticTypeId};

/// Translate the manifest-owned builtin index into the exact syntax ABI shape.
/// This is semantic evidence only; code generation still requires the canonical-runtime
/// capability before emitting an intrinsic.
pub(super) fn runtime_intrinsic_signature(index: u32) -> Result<ItemSignature, SemanticError> {
    let spec = beskid_analysis::builtins::builtin_specs()
        .get(usize::try_from(index).map_err(|_| SemanticError::unavailable("call_abi_signature"))?)
        .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
    let parameters = spec
        .params
        .iter()
        .copied()
        .map(builtin_type_to_semantic)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
    let result = builtin_type_to_semantic(spec.returns)
        .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
    Ok(ItemSignature { parameters: parameters.into(), result })
}

fn builtin_type_to_semantic(ty: beskid_analysis::builtins::BuiltinType) -> Option<SemanticTypeId> {
    use beskid_analysis::builtins::BuiltinType;
    Some(match ty {
        BuiltinType::String => SemanticTypeId::STRING,
        BuiltinType::Ptr => SemanticTypeId::POINTER,
        BuiltinType::Usize => SemanticTypeId::WORD,
        BuiltinType::U64 => SemanticTypeId::I64,
        BuiltinType::Unit => SemanticTypeId::UNIT,
        BuiltinType::Never => SemanticTypeId::NEVER,
    })
}
