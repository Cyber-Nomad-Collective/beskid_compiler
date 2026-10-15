use beskid_queries::SemanticTypeId;

#[derive(Debug, Clone, Copy)]
pub(crate) enum EntryReturnKind {
    Unit,
    Never,
    PointerLike,
    I64,
    I8,
    I16,
    U16,
    U64,
    F32,
    I32,
    U32,
    U8,
    Bool,
    F64,
    Char,
}

impl EntryReturnKind {
    /// The sole return-ABI bridge used by the syntax → ISLE JIT path.
    pub(crate) fn from_semantic_type(ty: SemanticTypeId) -> Option<Self> {
        Some(match ty {
            SemanticTypeId::UNIT => Self::Unit,
            SemanticTypeId::NEVER => Self::Never,
            SemanticTypeId::I64 => Self::I64,
            SemanticTypeId::I8 => Self::I8,
            SemanticTypeId::I16 => Self::I16,
            SemanticTypeId::U16 => Self::U16,
            SemanticTypeId::U64 => Self::U64,
            SemanticTypeId::F32 => Self::F32,
            SemanticTypeId::I32 => Self::I32,
            SemanticTypeId::U32 => Self::U32,
            SemanticTypeId::U8 => Self::U8,
            SemanticTypeId::BOOL => Self::Bool,
            SemanticTypeId::F64 => Self::F64,
            SemanticTypeId::CHAR => Self::Char,
            SemanticTypeId::STRING | SemanticTypeId::WORD | SemanticTypeId::POINTER => Self::PointerLike,
            _ => return None,
        })
    }
}

pub(crate) struct JitCallable;

impl JitCallable {
    pub(crate) fn execute_as_i64(ptr: *const u8, kind: EntryReturnKind) -> i64 {
        match kind {
            EntryReturnKind::Unit => {
                // SAFETY: `ptr` is expected to point to a JIT function with signature `extern "C" fn()`.
                unsafe { invoke0::<()>(ptr) };
                0
            }
            EntryReturnKind::Never => {
                // SAFETY: non-returning entrypoints are represented as `extern "C" fn() -> !`.
                let callable: extern "C" fn() -> ! = unsafe { std::mem::transmute(ptr) };
                callable()
            }
            EntryReturnKind::PointerLike => {
                // SAFETY: JIT pointer-like returns are represented as `u64`.
                unsafe { invoke0::<u64>(ptr) as i64 }
            }
            EntryReturnKind::I64 => {
                // SAFETY: Signature is selected from typed return info.
                unsafe { invoke0::<i64>(ptr) }
            }
            EntryReturnKind::I8 => unsafe { invoke0::<i8>(ptr) as i64 },
            EntryReturnKind::I16 => unsafe { invoke0::<i16>(ptr) as i64 },
            EntryReturnKind::U16 => unsafe { invoke0::<u16>(ptr) as i64 },
            EntryReturnKind::U64 => unsafe { invoke0::<u64>(ptr) as i64 },
            EntryReturnKind::F32 => unsafe { invoke0::<f32>(ptr).to_bits() as i64 },
            EntryReturnKind::I32 => {
                // SAFETY: Signature is selected from typed return info.
                unsafe { invoke0::<i32>(ptr) as i64 }
            }
            EntryReturnKind::U32 => {
                // SAFETY: Signature is selected from typed return info.
                unsafe { invoke0::<u32>(ptr) as i64 }
            }
            EntryReturnKind::U8 => {
                // SAFETY: Signature is selected from typed return info.
                unsafe { invoke0::<u8>(ptr) as i64 }
            }
            EntryReturnKind::Bool => {
                // SAFETY: `bool` is ABI-lowered as `u8` by the backend.
                unsafe { invoke0::<u8>(ptr) as i64 }
            }
            EntryReturnKind::F64 => {
                // SAFETY: Signature is selected from typed return info.
                let value: f64 = unsafe { invoke0::<f64>(ptr) };
                value.to_bits() as i64
            }
            EntryReturnKind::Char => {
                // SAFETY: `char` is ABI-lowered as a `u32` scalar value.
                unsafe { invoke0::<u32>(ptr) as i64 }
            }
        }
    }

    pub(crate) fn format_i64_result(value: i64, kind: EntryReturnKind) -> String {
        match kind {
            EntryReturnKind::Unit => "ok".to_owned(),
            EntryReturnKind::Never => unreachable!("never returns"),
            EntryReturnKind::PointerLike => format!("0x{value:016x}"),
            EntryReturnKind::I64 => value.to_string(),
            EntryReturnKind::I8 => (value as i8).to_string(),
            EntryReturnKind::I16 => (value as i16).to_string(),
            EntryReturnKind::U16 => (value as u16).to_string(),
            EntryReturnKind::U64 => (value as u64).to_string(),
            EntryReturnKind::F32 => f32::from_bits(value as u32).to_string(),
            EntryReturnKind::I32 => (value as i32).to_string(),
            EntryReturnKind::U32 => (value as u32).to_string(),
            EntryReturnKind::U8 => (value as u8).to_string(),
            EntryReturnKind::Bool => ((value as u8) != 0).to_string(),
            EntryReturnKind::F64 => f64::from_bits(value as u64).to_string(),
            EntryReturnKind::Char => std::char::from_u32(value as u32).unwrap_or('\u{FFFD}').to_string(),
        }
    }
}

unsafe fn invoke0<R>(ptr: *const u8) -> R {
    let callable: extern "C" fn() -> R = unsafe { std::mem::transmute(ptr) };
    callable()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u32_result_formats_without_signed_i32_aliasing() {
        assert!(matches!(EntryReturnKind::from_semantic_type(SemanticTypeId::U32), Some(EntryReturnKind::U32)));
        assert_eq!(JitCallable::format_i64_result(u32::MAX as i64, EntryReturnKind::U32), "4294967295");
    }
}
