//! Parameter/return classification for builtins shared by codegen (`BUILTIN_SPECS`) and JIT hosts.

/// Scalar kinds used when building Cranelift signatures for builtins (`Ptr` vs fixed `I64`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiParamKind {
    Ptr,
    I64,
}

/// Builtin return slot shape (including [`AbiReturnKind::Never`] for diverging calls).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiReturnKind {
    Void,
    Ptr,
    I64,
    I32,
    Never,
}

/// One importable runtime function: exported `symbol` string and Cranelift ABI shape.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinFnSpec {
    pub symbol: &'static str,
    pub params: &'static [AbiParamKind],
    pub returns: AbiReturnKind,
}
