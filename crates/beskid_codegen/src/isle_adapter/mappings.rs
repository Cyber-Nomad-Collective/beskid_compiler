use super::*;

pub(super) fn align_to(value: u32, alignment: u32) -> Option<u32> {
    value.checked_add(alignment.checked_sub(1)?).map(|value| value / alignment * alignment)
}

pub(super) fn semantic_type_for_runtime_intrinsic(
    intrinsic: &beskid_abi::abi_v5::RuntimeIntrinsic,
) -> Option<SemanticTypeId> {
    use beskid_abi::abi_v5::AbiType;

    Some(match intrinsic.result {
        AbiType::Void => return None,
        AbiType::Pointer => SemanticTypeId::POINTER,
        AbiType::USize => SemanticTypeId::WORD,
        AbiType::I8 => SemanticTypeId::U8,
        AbiType::U8 => SemanticTypeId::U8,
        AbiType::I32 => SemanticTypeId::I32,
        AbiType::I64 => SemanticTypeId::I64,
        AbiType::F64 => SemanticTypeId::F64,
        _ => return None,
    })
}

pub(super) fn signature_for_item(isa: &dyn TargetIsa, item: ItemSignature) -> Option<beskid_isle::Signature> {
    let emitter = FunctionEmitter::new(isa);
    let parameters = item
        .parameters
        .iter()
        .copied()
        .map(|semantic| map_signature_type(isa, semantic))
        .collect::<Option<Vec<_>>>()?;
    let returns = match item.result {
        SemanticTypeId::UNIT | SemanticTypeId::NEVER => Vec::new(),
        result => vec![map_signature_type(isa, result)?],
    };
    Some(emitter.signature(parameters, returns))
}

pub(super) fn specialization_identity(signature: &ItemSignature) -> std::sync::Arc<[u32]> {
    signature
        .parameters
        .iter()
        .map(|semantic| semantic.0)
        .chain(std::iter::once(signature.result.0))
        .collect::<Vec<_>>()
        .into()
}

pub(super) fn signature_for_runtime_intrinsic(
    isa: &dyn TargetIsa,
    intrinsic: &beskid_abi::abi_v5::RuntimeIntrinsic,
) -> Option<beskid_isle::Signature> {
    let emitter = FunctionEmitter::new(isa);
    let parameters = intrinsic.params.iter().copied().map(|ty| map_abi_type(isa, ty)).collect::<Option<Vec<_>>>()?;
    let returns = if intrinsic.noreturn || intrinsic.result == beskid_abi::abi_v5::AbiType::Void {
        Vec::new()
    } else {
        vec![map_abi_type(isa, intrinsic.result)?]
    };
    Some(emitter.signature(parameters, returns))
}

pub(super) fn map_node_kind(kind: beskid_queries::IndexedNodeKind) -> Option<NodeKind> {
    match beskid_isle::classify_syntax_node_kind(kind) {
        beskid_isle::SyntaxNodeClassification::IsleLowered(kind) => Some(kind),
        beskid_isle::SyntaxNodeClassification::Structural
        | beskid_isle::SyntaxNodeClassification::UnsupportedTypedOperation => None,
    }
}

pub(super) fn runtime_intrinsic_kind_for_name(name: &str) -> Option<RuntimeIntrinsicKind> {
    Some(match name {
        "memory_copy" => RuntimeIntrinsicKind::MemoryCopy,
        "memory_set" => RuntimeIntrinsicKind::MemorySet,
        "native_word_from_pointer" => RuntimeIntrinsicKind::NativeWordFromPointer,
        "pointer_from_native_word" => RuntimeIntrinsicKind::PointerFromNativeWord,
        "pointer_add" => RuntimeIntrinsicKind::PointerAdd,
        "raw_word_load" => RuntimeIntrinsicKind::RawWordLoad,
        "raw_word_store" => RuntimeIntrinsicKind::RawWordStore,
        "raw_byte_load" => RuntimeIntrinsicKind::RawByteLoad,
        "raw_byte_store" => RuntimeIntrinsicKind::RawByteStore,
        _ => return None,
    })
}

pub(super) fn map_operator_fact(operator: beskid_queries::OperatorFact) -> OperatorFact {
    use beskid_queries::OperatorFact as Syntax;

    match operator {
        Syntax::Or => OperatorFact::Or,
        Syntax::And => OperatorFact::And,
        Syntax::BitOr => OperatorFact::BitOr,
        Syntax::BitAnd => OperatorFact::BitAnd,
        Syntax::Shl => OperatorFact::Shl,
        Syntax::Shr => OperatorFact::Shr,
        Syntax::IdentityEq => OperatorFact::IdentityEq,
        Syntax::IdentityNotEq => OperatorFact::IdentityNotEq,
        Syntax::Eq => OperatorFact::Eq,
        Syntax::NotEq => OperatorFact::NotEq,
        Syntax::Lt => OperatorFact::Lt,
        Syntax::Lte => OperatorFact::Lte,
        Syntax::Gt => OperatorFact::Gt,
        Syntax::Gte => OperatorFact::Gte,
        Syntax::Add => OperatorFact::Add,
        Syntax::Sub => OperatorFact::Sub,
        Syntax::Mul => OperatorFact::Mul,
        Syntax::Div => OperatorFact::Div,
        Syntax::Mod => OperatorFact::Mod,
        Syntax::Neg => OperatorFact::Neg,
        Syntax::Not => OperatorFact::Not,
        Syntax::StringAdd => OperatorFact::StringAdd,
        Syntax::StringEq => OperatorFact::StringEq,
        Syntax::StringNotEq => OperatorFact::StringNotEq,
    }
}

pub(super) fn map_scalar_type(semantic: SemanticTypeId) -> Option<Type> {
    Some(match semantic {
        SemanticTypeId::BOOL | SemanticTypeId::U8 => types::I8,
        SemanticTypeId::I32 => types::I32,
        SemanticTypeId::I64 => types::I64,
        SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::NEVER => return None,
        SemanticTypeId::F64 => types::F64,
        SemanticTypeId::CHAR => types::I32,
        SemanticTypeId::UNIT | SemanticTypeId::STRING => return None,
        _ => return None,
    })
}

pub(super) fn map_signature_type(isa: &dyn TargetIsa, semantic: SemanticTypeId) -> Option<Type> {
    if matches!(semantic, SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::STRING) {
        Some(isa.pointer_type())
    } else {
        map_scalar_type(semantic)
    }
}

pub(super) fn map_abi_type(isa: &dyn TargetIsa, ty: beskid_abi::abi_v5::AbiType) -> Option<Type> {
    use beskid_abi::abi_v5::AbiType;
    Some(match ty {
        AbiType::Pointer | AbiType::USize | AbiType::ISize => isa.pointer_type(),
        AbiType::I8 | AbiType::U8 => types::I8,
        AbiType::I16 | AbiType::U16 => types::I16,
        AbiType::I32 | AbiType::U32 => types::I32,
        AbiType::I64 | AbiType::U64 => types::I64,
        AbiType::V128 => types::I8X16,
        AbiType::F32 => types::F32,
        AbiType::F64 => types::F64,
        AbiType::Void => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_runtime_intrinsic_names_map_to_exact_isle_kinds() {
        for (name, expected) in [
            ("memory_copy", RuntimeIntrinsicKind::MemoryCopy),
            ("memory_set", RuntimeIntrinsicKind::MemorySet),
            ("native_word_from_pointer", RuntimeIntrinsicKind::NativeWordFromPointer),
            ("pointer_from_native_word", RuntimeIntrinsicKind::PointerFromNativeWord),
            ("pointer_add", RuntimeIntrinsicKind::PointerAdd),
            ("raw_word_load", RuntimeIntrinsicKind::RawWordLoad),
            ("raw_word_store", RuntimeIntrinsicKind::RawWordStore),
            ("raw_byte_load", RuntimeIntrinsicKind::RawByteLoad),
            ("raw_byte_store", RuntimeIntrinsicKind::RawByteStore),
        ] {
            assert_eq!(
                beskid_isle::classify_syntax_node_kind(beskid_queries::IndexedNodeKind::CallExpression),
                beskid_isle::SyntaxNodeClassification::IsleLowered(NodeKind::CallExpression),
            );
            assert_eq!(runtime_intrinsic_kind_for_name(name), Some(expected));
        }
        assert_eq!(runtime_intrinsic_kind_for_name("untrusted"), None);
    }
}
