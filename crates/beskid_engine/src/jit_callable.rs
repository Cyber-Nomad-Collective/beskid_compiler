use beskid_analysis::hir::HirPrimitiveType;
use beskid_analysis::types::TypeInfo;

#[derive(Debug, Clone, Copy)]
pub(crate) enum EntryReturnKind {
    Unit,
    Never,
    PointerLike,
    I64,
    I32,
    U8,
    Bool,
    F64,
    Char,
}

impl EntryReturnKind {
    pub(crate) fn from_type_info(info: &TypeInfo) -> Self {
        match info {
            TypeInfo::Primitive(HirPrimitiveType::Unit) => Self::Unit,
            TypeInfo::Primitive(HirPrimitiveType::Never) => Self::Never,
            TypeInfo::Primitive(HirPrimitiveType::String)
            | TypeInfo::Named(_)
            | TypeInfo::GenericParam(_)
            | TypeInfo::Applied { .. }
            | TypeInfo::Function { .. }
            | TypeInfo::Array(_)
            | TypeInfo::Fiber(_) => Self::PointerLike,
            TypeInfo::Primitive(HirPrimitiveType::I64) => Self::I64,
            TypeInfo::Primitive(HirPrimitiveType::I32) => Self::I32,
            TypeInfo::Primitive(HirPrimitiveType::U8) => Self::U8,
            TypeInfo::Primitive(HirPrimitiveType::Bool) => Self::Bool,
            TypeInfo::Primitive(HirPrimitiveType::F64) => Self::F64,
            TypeInfo::Primitive(HirPrimitiveType::Char) => Self::Char,
        }
    }
}

pub(crate) struct JitCallable;

impl JitCallable {
    pub(crate) fn execute_and_format(ptr: *const u8, return_info: &TypeInfo) -> String {
        Self::format_i64_result(
            Self::execute_as_i64(ptr, EntryReturnKind::from_type_info(return_info)),
            EntryReturnKind::from_type_info(return_info),
        )
    }

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
            EntryReturnKind::I32 => {
                // SAFETY: Signature is selected from typed return info.
                unsafe { invoke0::<i32>(ptr) as i64 }
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
            EntryReturnKind::I32 => (value as i32).to_string(),
            EntryReturnKind::U8 => (value as u8).to_string(),
            EntryReturnKind::Bool => ((value as u8) != 0).to_string(),
            EntryReturnKind::F64 => f64::from_bits(value as u64).to_string(),
            EntryReturnKind::Char => std::char::from_u32(value as u32)
                .unwrap_or('\u{FFFD}')
                .to_string(),
        }
    }
}

unsafe fn invoke0<R>(ptr: *const u8) -> R {
    let callable: extern "C" fn() -> R = unsafe { std::mem::transmute(ptr) };
    callable()
}
