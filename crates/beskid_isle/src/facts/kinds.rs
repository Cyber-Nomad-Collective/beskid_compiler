//! Literal, intrinsic, operator, index, iterable, and call kinds.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiteralKind {
    Integer,
    Float,
    String,
    Char,
    Boolean,
}

/// Compiler-owned primitives available only to canonical runtime syntax.
///
/// They are selected from the manifest-backed capability, never from a user-declared extern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeIntrinsicKind {
    MemoryCopy,
    MemorySet,
    NativeWordFromPointer,
    PointerFromNativeWord,
    PointerAdd,
    RawWordLoad,
    RawWordStore,
    RawByteLoad,
    RawByteStore,
    ArchContextSize(u64),
    ArchContextAlignment(u64),
    SchedulerFiberEntryAddress,
    SchedulerReturnTrampolineAddress,
    SchedulerPollEntryInvoke,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperatorFact {
    Or,
    And,
    BitOr,
    BitAnd,
    Shl,
    Shr,
    IdentityEq,
    IdentityNotEq,
    Eq,
    NotEq,
    Lt,
    Lte,
    Gt,
    Gte,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Neg,
    Not,
    StringAdd,
    StringEq,
    StringNotEq,
    EnumEq,
    EnumNotEq,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexTarget {
    String,
    Array,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForIterableKind {
    Range,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    Direct,
    PrimitiveNumericConversion,
    InlineLambda,
    RuntimeIntrinsic,
    CollectionOperation,
    /// A direct call whose callee declares a `bulk` array parameter.
    ///
    /// The call site packs N scalar arguments into a fresh rooted array (reusing the
    /// `emit_array_literal` allocation sequence) and direct-calls the callee with that array as
    /// its sole argument. The callee signature still has one array parameter, so this is a
    /// distinct lowering path from [`CallKind::Direct`], which requires argument/signature arity
    /// to match.
    Bulk,
    /// Canonical Foundation `Array.Empty<T>` resolved with its enclosing concrete generic
    /// specialization. This reuses the descriptor-backed array-literal allocation sequence and
    /// never imports the legacy element-size `array_new` ABI.
    TypedArrayAllocation,
    Dynamic,
}
