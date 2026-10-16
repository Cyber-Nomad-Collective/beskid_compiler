//! Facts for `move` expressions and the `exchange(place, move value)` builtin
//! (`language-meta--memory-model--memory-and-references`, "Destructive read of a field").

use cranelift_codegen::ir::Type;

use super::*;
use crate::layout::{EnumLayout, ManagedArrayAllocation, ManagedStructAllocation};

/// How one `move` expression evaluates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MoveExpressionPlan {
    /// `move x`: the owned value of `operand`, transferred as is. Ownership is a static fact, so
    /// nothing is stored.
    Transfer { operand: AstNodeKey },
    /// `move x.f`: read the field `place` and store `empty` into it in the same step, through
    /// the barriered store of an existing object's pointer slot.
    DestructiveRead { place: AstNodeKey, empty: CanonicalEmptyPlan },
}

/// Construction of the canonical empty value a destructive read leaves in its field. Every
/// allocation is an initialization of a new object; only the store into the field is barriered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonicalEmptyPlan {
    /// The payload-free `variant` of the `Option<T>` `layout`, allocated through `allocation`.
    OptionNone { layout: EnumLayout, allocation: ManagedStructAllocation, variant: u32 },
    /// A descriptor-backed array of length zero.
    EmptyArray { allocation: ManagedArrayAllocation },
    /// A `List<T>` object whose storage field (at `storage_offset`) holds an empty array of
    /// `storage` and whose count field (`count_type` at `count_offset`) holds zero.
    EmptyList {
        storage: ManagedArrayAllocation,
        list: ManagedStructAllocation,
        storage_offset: i32,
        count_offset: i32,
        count_type: Type,
    },
}

/// The operands of one `exchange(place, move value)` call: `place` is a field or element
/// expression, read before `value` is stored into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExchangePlan {
    pub place: AstNodeKey,
    pub value: AstNodeKey,
}
