//! Constant and literal facts.

use super::super::*;

impl SyntaxNodeFacts<'_> {
    pub(super) fn constant_integer_impl(&self, key: AstNodeKey) -> Option<i64> {
        self.query(constant_integer(self.db, key))
    }

    pub(super) fn canonical_runtime_constant_integer_impl(&self, key: AstNodeKey) -> Option<i64> {
        if self.node_kind(key) != Some(NodeKind::PathExpression) || self.input.runtime_intrinsic_capability().is_none()
        {
            return None;
        }
        self.query(constant_integer(self.db, key))
    }

    pub(super) fn integer_literal_impl(&self, key: AstNodeKey) -> Option<i64> {
        let LiteralFact::Integer(text) = self.literal(key)? else {
            return None;
        };
        let clean = beskid_analysis::syntax::integer_literal_magnitude(&text).replace('_', "");
        let value = clean.as_str();
        match value.strip_prefix("0x") {
            Some(hexadecimal) => u64::from_str_radix(hexadecimal, 16).ok().map(|number| number as i64),
            None => value.parse::<i64>().ok().or_else(|| value.parse::<u64>().ok().map(|bits| bits as i64)),
        }
    }

    /// A `bool` literal at `key`, looking through grouping parentheses: `beskid fmt` writes a
    /// loop condition as `while (true)`, which must stay a statically-true condition.
    pub(super) fn boolean_literal_impl(&self, key: AstNodeKey) -> Option<bool> {
        let mut node = key;
        loop {
            if let Some(LiteralFact::Bool(value)) = self.literal(node) {
                return Some(value);
            }
            match self.query(node_kind(self.db, node))? {
                beskid_queries::IndexedNodeKind::Expression | beskid_queries::IndexedNodeKind::GroupedExpression => {
                    node = *self.raw_children(node).first()?;
                }
                _ => return None,
            }
        }
    }

    pub(super) fn float_literal_impl(&self, key: AstNodeKey) -> Option<f64> {
        let LiteralFact::Float(text) = self.literal(key)? else {
            return None;
        };
        let magnitude = beskid_analysis::syntax::float_literal_magnitude(&text);
        if text.ends_with("_f32") || self.scalar_semantic_type(key) == Some(SemanticTypeId::F32) {
            magnitude.parse::<f32>().ok().map(f64::from)
        } else {
            magnitude.parse().ok()
        }
    }

    pub(super) fn char_literal_impl(&self, key: AstNodeKey) -> Option<char> {
        let LiteralFact::Char(text) = self.literal(key)? else {
            return None;
        };
        beskid_analysis::syntax::decode_char_literal_token(&text).ok()
    }

    pub(super) fn string_literal_impl(&self, key: AstNodeKey) -> Option<std::sync::Arc<str>> {
        let LiteralFact::String(text) = self.literal(key)? else {
            return None;
        };
        try_decode_string_literal_token(&text).map(Into::into)
    }
}
