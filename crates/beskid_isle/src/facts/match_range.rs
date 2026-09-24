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
    Fields(Vec<MatchPayloadPatternFact>),
    Enum { layout: EnumLayout, discriminant: u64, payload: Box<MatchPayloadPatternFact> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchArmFact {
    pub(crate) discriminant: Option<u64>,
    pub(crate) body: AstNodeKey,
    pub(crate) payload: MatchPayloadPatternFact,
}

impl MatchArmFact {
    pub const fn variant(discriminant: u64, body: AstNodeKey) -> Self {
        Self { discriminant: Some(discriminant), body, payload: MatchPayloadPatternFact::Fields(Vec::new()) }
    }

    pub fn variant_with_binding(discriminant: u64, body: AstNodeKey, binding: Option<MatchArmBindingFact>) -> Self {
        let payload = match binding {
            Some(binding) => MatchPayloadPatternFact::Binding(binding),
            None => MatchPayloadPatternFact::Ignore,
        };
        Self { discriminant: Some(discriminant), body, payload: MatchPayloadPatternFact::Fields(vec![payload]) }
    }

    pub fn variant_with_payload(discriminant: u64, body: AstNodeKey, payload: MatchPayloadPatternFact) -> Self {
        Self { discriminant: Some(discriminant), body, payload }
    }

    pub const fn wildcard(body: AstNodeKey) -> Self {
        Self { discriminant: None, body, payload: MatchPayloadPatternFact::Ignore }
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
