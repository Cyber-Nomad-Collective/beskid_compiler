//! Enum layout, variant, payload, and match-arm facts.

use super::super::*;

impl SyntaxNodeFacts<'_> {
    pub(super) fn enum_layout_impl(&self, key: AstNodeKey) -> Option<EnumLayout> {
        self.enum_layout_for(key)
    }

    pub(super) fn binary_enum_layout_impl(&self, key: AstNodeKey) -> Option<EnumLayout> {
        let left_key = self.child(key, 0)?;
        let right_key = self.child(key, 1)?;
        // Try to get the enum layout from either operand. Enum-literal operands
        // (constructors) have a registered static plan and produce the full layout.
        // Path-expression operands that name an enum variable don't have a static
        // plan; we use the literal's layout and verify type compatibility.
        let layout = self.enum_layout_for(left_key).or_else(|| self.enum_layout_for(right_key))?;
        // When both operands are literals, verify tag layouts agree.
        if let Some(right_layout) = self.enum_layout_for(right_key).or_else(|| self.enum_layout_for(left_key))
            && (layout.tag.offset != right_layout.tag.offset || layout.tag.value_type != right_layout.tag.value_type)
        {
            return None;
        }
        // When one operand is a path expression, verify its semantic type matches
        // the literal's type (both must be the same enum).
        let left_type = self.scalar_semantic_type(left_key)?;
        let right_type = self.scalar_semantic_type(right_key)?;
        // Only nominal (non-primitive) types can be enums; primitive equality is
        // handled by the integer/float comparison path.
        if left_type != right_type
            || matches!(
                left_type,
                SemanticTypeId::I32
                    | SemanticTypeId::I64
                    | SemanticTypeId::U32
                    | SemanticTypeId::U8
                    | SemanticTypeId::F64
                    | SemanticTypeId::BOOL
                    | SemanticTypeId::CHAR
                    | SemanticTypeId::UNIT
                    | SemanticTypeId::NEVER
            )
        {
            return None;
        }
        Some(layout)
    }

    pub(super) fn enum_variant_index_impl(&self, key: AstNodeKey) -> Option<u32> {
        self.query(enum_constructor(self.db, key))
            .or_else(|| self.specialized_enum_constructor(key).map(|fact| fact.constructor))
            .map(|constructor| constructor.variant_index)
    }

    pub(super) fn enum_payloads_impl(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        Some(
            self.query(enum_constructor(self.db, key))
                .or_else(|| self.specialized_enum_constructor(key).map(|fact| fact.constructor))?
                .payloads
                .to_vec(),
        )
    }

    pub(super) fn match_arms_impl(&self, key: AstNodeKey) -> Option<Vec<MatchArmFact>> {
        let fact = self.enum_match_in_context(key)?;
        fact.arms
            .iter()
            .map(|arm| match &arm.pattern {
                beskid_queries::EnumMatchPatternFact::Wildcard => Some(MatchArmFact::wildcard(arm.body)),
                beskid_queries::EnumMatchPatternFact::Enum(pattern)
                    if pattern.declaration == fact.declaration && pattern.layout == fact.layout =>
                {
                    let payload = MatchPayloadPatternFact::Fields(
                        pattern
                            .items
                            .iter()
                            .map(|payload| self.match_payload_pattern(payload))
                            .collect::<Option<Vec<_>>>()?,
                    );
                    Some(MatchArmFact::variant_with_payload(u64::from(pattern.variant_index), arm.body, payload))
                }
                _ => None,
            })
            .collect()
    }
}
