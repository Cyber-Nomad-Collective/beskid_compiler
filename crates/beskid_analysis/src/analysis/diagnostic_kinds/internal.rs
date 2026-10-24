//! The one shape every internal compiler error takes (E2101-E2105): a message that starts with
//! "internal compiler error:", names the construct in words, and ends with "please report this";
//! the internal reason goes to the help line, never debug text in the message.

use super::SemanticIssueKind;

/// Where an internal compiler error is reported.
pub const INTERNAL_ERROR_REPORT: &str = "https://github.com/beskid-lang/beskid/issues";

/// A syntax node kind name in words: `AssignExpression` -> `assign expression`. An empty or
/// unknown kind is "construct".
pub fn humanize_construct(kind: &str) -> String {
    let kind = kind.trim();
    if kind.is_empty() || kind == "Unknown" {
        return "construct".to_string();
    }
    let mut words = String::with_capacity(kind.len() + 4);
    for (index, character) in kind.chars().enumerate() {
        if character.is_uppercase() && index > 0 {
            words.push(' ');
        }
        words.extend(character.to_lowercase());
    }
    words
}

impl SemanticIssueKind {
    /// Whether this kind is an internal compiler error (E2101-E2105).
    pub fn is_internal(&self) -> bool {
        matches!(
            self,
            Self::InternalSemanticFactUnavailable { .. }
                | Self::InternalLoweringRuleMissing { .. }
                | Self::InternalLoweringInvariant { .. }
                | Self::InternalModSemanticFailure { .. }
                | Self::InternalBuildFailure { .. }
        )
    }

    pub(super) fn internal_message(&self) -> Option<String> {
        let summary = match self {
            Self::InternalSemanticFactUnavailable { construct, .. } => {
                format!("a compiler fact this {} needs is missing", humanize_construct(construct))
            }
            Self::InternalLoweringRuleMissing { construct } => {
                format!("the compiler cannot lower this {} yet", humanize_construct(construct))
            }
            Self::InternalLoweringInvariant { construct, .. } => {
                format!("the compiler hit an inconsistency while lowering this {}", humanize_construct(construct))
            }
            Self::InternalModSemanticFailure { .. } => {
                "the compiler could not answer a Mod's semantic request".to_string()
            }
            Self::InternalBuildFailure { .. } => "the build stopped on an inconsistency in the compiler".to_string(),
            _ => return None,
        };
        Some(format!("internal compiler error: {summary}; please report this"))
    }

    pub(super) fn internal_help(&self) -> Option<String> {
        let reason = match self {
            Self::InternalSemanticFactUnavailable { query, .. } => format!("semantic fact `{query}` is unavailable"),
            Self::InternalLoweringRuleMissing { .. } => "no lowering rule or fact matches this construct".to_string(),
            Self::InternalLoweringInvariant { reason, .. }
            | Self::InternalModSemanticFailure { reason }
            | Self::InternalBuildFailure { reason } => reason.clone(),
            _ => return None,
        };
        Some(format!("report it at {INTERNAL_ERROR_REPORT} with the source that triggers it; internal reason: {reason}"))
    }

    pub(super) fn internal_label(&self) -> Option<String> {
        self.is_internal().then(|| "the compiler failed here".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construct_kinds_read_as_words() {
        assert_eq!(humanize_construct("AssignExpression"), "assign expression");
        assert_eq!(humanize_construct("Unknown"), "construct");
        assert_eq!(humanize_construct(""), "construct");
    }

    #[test]
    fn every_internal_kind_has_the_one_shape() {
        let kinds = [
            SemanticIssueKind::InternalSemanticFactUnavailable {
                query: "callable_signature".into(),
                construct: "CallExpression".into(),
            },
            SemanticIssueKind::InternalLoweringRuleMissing { construct: "ExpressionStatement".into() },
            SemanticIssueKind::InternalLoweringInvariant {
                construct: "MatchExpression".into(),
                reason: "invalid enum layout".into(),
            },
            SemanticIssueKind::InternalModSemanticFailure { reason: "unknown semantic unit".into() },
            SemanticIssueKind::InternalBuildFailure { reason: "object module already finalized".into() },
        ];
        for (offset, kind) in kinds.iter().enumerate() {
            assert_eq!(kind.code(), format!("E{}", 2101 + offset));
            let message = kind.message();
            assert!(message.starts_with("internal compiler error: "), "{message}");
            assert!(message.ends_with("; please report this"), "{message}");
            assert!(!message.contains("MissingRuleOrFact") && !message.contains('#'), "{message}");
            let help = kind.help().expect("internal help");
            assert!(help.contains(INTERNAL_ERROR_REPORT) && help.contains("internal reason: "), "{help}");
            assert_eq!(kind.label(), "the compiler failed here");
            assert!(kind.is_internal());
        }
        assert_eq!(
            kinds[2].message(),
            "internal compiler error: the compiler hit an inconsistency while lowering this match expression; \
             please report this"
        );
    }
}
