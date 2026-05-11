//! Structural parse failures while walking a Pest pair tree into the syntax AST.

use pest::iterators::Pair;

use crate::parser::Rule;
use crate::syntax::SpanInfo;

/// Rule mismatch, missing child rule, or grammar-specific constraint (for example `self` in `impl`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedRule {
        expected: Option<Rule>,
        found: Rule,
        span: SpanInfo,
    },
    MissingPair {
        expected: Rule,
    },
    ForbiddenImplSelfParameter {
        span: SpanInfo,
    },
}

impl ParseError {
    /// Record an unexpected rule at `pair`'s span; `expected` is `None` when any child was invalid.
    pub fn unexpected_rule(pair: Pair<Rule>, expected: Option<Rule>) -> Self {
        Self::UnexpectedRule {
            expected,
            found: pair.as_rule(),
            span: SpanInfo::from_span(&pair.as_span()),
        }
    }

    /// Expected a child rule `expected` but the pair iterator ended.
    pub fn missing(expected: Rule) -> Self {
        Self::MissingPair { expected }
    }

    /// `self` used where the grammar forbids it (impl receiver contract).
    pub fn forbidden_impl_self_parameter(span: SpanInfo) -> Self {
        Self::ForbiddenImplSelfParameter { span }
    }
}
