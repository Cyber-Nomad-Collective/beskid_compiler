//! Match-arm and range facts.

use super::*;
use crate::layout::EnumLayout;
use cranelift_codegen::ir::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchArmBindingFact {
    pub slot: LocalSlotId,
    pub value_type: Type,
    pub managed_reference: ManagedReferenceFact,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchPayloadPatternFact {
    Ignore,
    Unit,
    Binding(MatchArmBindingFact),
    ScalarLiteral { expression: AstNodeKey, value_type: Type },
    /// A string literal compared to the matched `string` by content (`str_eq`).
    StringLiteral { expression: AstNodeKey },
    Fields(Vec<MatchPayloadPatternFact>),
    Enum { layout: EnumLayout, discriminant: u64, payload: Box<MatchPayloadPatternFact> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchArmFact {
    /// The enum variant the arm selects; `None` for an arm whose top-level pattern tests the
    /// whole scrutinee value (wildcard, binding, or scalar/string literal).
    pub(crate) discriminant: Option<u64>,
    pub(crate) body: AstNodeKey,
    pub(crate) payload: MatchPayloadPatternFact,
    /// A `when` guard evaluated after the pattern matched, with its bindings in scope.
    pub(crate) guard: Option<AstNodeKey>,
}

impl MatchArmFact {
    pub const fn variant(discriminant: u64, body: AstNodeKey) -> Self {
        Self { discriminant: Some(discriminant), body, payload: MatchPayloadPatternFact::Fields(Vec::new()), guard: None }
    }

    pub fn variant_with_binding(discriminant: u64, body: AstNodeKey, binding: Option<MatchArmBindingFact>) -> Self {
        let payload = match binding {
            Some(binding) => MatchPayloadPatternFact::Binding(binding),
            None => MatchPayloadPatternFact::Ignore,
        };
        Self {
            discriminant: Some(discriminant),
            body,
            payload: MatchPayloadPatternFact::Fields(vec![payload]),
            guard: None,
        }
    }

    pub fn variant_with_payload(discriminant: u64, body: AstNodeKey, payload: MatchPayloadPatternFact) -> Self {
        Self { discriminant: Some(discriminant), body, payload, guard: None }
    }

    pub const fn wildcard(body: AstNodeKey) -> Self {
        Self { discriminant: None, body, payload: MatchPayloadPatternFact::Ignore, guard: None }
    }

    /// An arm whose top-level pattern tests the whole scrutinee value: `Ignore`, `Binding`,
    /// `ScalarLiteral`, or `StringLiteral`.
    pub fn value(body: AstNodeKey, pattern: MatchPayloadPatternFact) -> Self {
        Self { discriminant: None, body, payload: pattern, guard: None }
    }

    /// The same arm guarded by a `when` condition.
    pub fn with_guard(self, guard: Option<AstNodeKey>) -> Self {
        Self { guard, ..self }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RangeFact {
    pub(crate) start: AstNodeKey,
    pub(crate) end: AstNodeKey,
    pub(crate) step: i64,
    pub(crate) inclusive: bool,
}

impl RangeFact {
    pub const fn new(start: AstNodeKey, end: AstNodeKey, step: i64, inclusive: bool) -> Self {
        Self { start, end, step, inclusive }
    }
}

pub type Unit = ();
