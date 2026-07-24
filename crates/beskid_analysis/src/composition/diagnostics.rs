use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::syntax::SpanInfo;

use super::model::ScopeId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositionIssue {
    MissingLaunchHost { span: Option<SpanInfo> },
    MultipleLaunchHosts { first_span: Option<SpanInfo>, second_span: Option<SpanInfo> },
    UnknownLaunchHost { host_name: String, span: SpanInfo },
    HostInheritanceCycle { host_name: String, span: SpanInfo },
    DuplicateScopeName { scope_name: String, span: SpanInfo, first_scope: ScopeId, second_scope: ScopeId },
    UnknownParentScope { scope_name: String, parent_scope: ScopeId, span: SpanInfo },
    UnknownRegistrationId { registration_id: u32, span: Option<SpanInfo> },
    DependencyCycle { from_id: u32, to_id: u32, span: Option<SpanInfo> },
    AmbiguousInject { requested_type: String, span: SpanInfo },
    UnresolvedInject { requested_type: String, span: SpanInfo },
    EmptyPluralInject { requested_type: String, span: SpanInfo },
    WithArgsMismatch { scope_name: String, span: SpanInfo },
    ScopedOutsideWith { span: SpanInfo },
    ChildScopeWithoutParent { scope_name: String, span: SpanInfo },
    InvalidScopeQualifier { qualifier: String, span: SpanInfo },
    OverrideLifetimeMismatch { binding: String, span: SpanInfo },
    InjectOnConstructor { span: SpanInfo },
    HostInModProject { span: SpanInfo },
}

/// Map a composition issue to semantic diagnostic kind and span when possible.
pub fn to_semantic_issue(issue: &CompositionIssue) -> Option<(SpanInfo, SemanticIssueKind)> {
    match issue {
        CompositionIssue::MissingLaunchHost { span } => {
            Some((span.unwrap_or_default(), SemanticIssueKind::CompositionMissingLaunchHost))
        }
        CompositionIssue::MultipleLaunchHosts { first_span, second_span } => Some((
            (*second_span).or(*first_span).unwrap_or_default(),
            SemanticIssueKind::CompositionMultipleLaunchHosts,
        )),
        CompositionIssue::UnknownLaunchHost { host_name, span } => {
            Some((*span, SemanticIssueKind::CompositionLaunchTargetNotHost { target_name: host_name.clone() }))
        }
        CompositionIssue::HostInheritanceCycle { host_name, span } => {
            Some((*span, SemanticIssueKind::CompositionHostInheritanceCycle { host_name: host_name.clone() }))
        }
        CompositionIssue::DuplicateScopeName { scope_name, span, .. } => {
            Some((*span, SemanticIssueKind::CompositionDuplicateScopeName { scope_name: scope_name.clone() }))
        }
        CompositionIssue::UnknownParentScope { scope_name, span, .. } => {
            Some((*span, SemanticIssueKind::CompositionChildScopeWithoutParent { scope_name: scope_name.clone() }))
        }
        CompositionIssue::UnknownRegistrationId { .. } => None,
        CompositionIssue::DependencyCycle { from_id, to_id, span } => Some((
            span.unwrap_or_default(),
            SemanticIssueKind::CompositionDependencyCycle { from_id: *from_id, to_id: *to_id },
        )),
        CompositionIssue::AmbiguousInject { requested_type, span } => {
            Some((*span, SemanticIssueKind::CompositionAmbiguousInject { requested_type: requested_type.clone() }))
        }
        CompositionIssue::UnresolvedInject { requested_type, span } => {
            Some((*span, SemanticIssueKind::CompositionUnresolvedInject { requested_type: requested_type.clone() }))
        }
        CompositionIssue::EmptyPluralInject { requested_type, span } => {
            Some((*span, SemanticIssueKind::CompositionUnresolvedInject { requested_type: requested_type.clone() }))
        }
        CompositionIssue::WithArgsMismatch { scope_name, span } => {
            Some((*span, SemanticIssueKind::CompositionWithArgsMismatch { scope_name: scope_name.clone() }))
        }
        CompositionIssue::ScopedOutsideWith { span } => Some((*span, SemanticIssueKind::CompositionScopedOutsideWith)),
        CompositionIssue::ChildScopeWithoutParent { scope_name, span } => {
            Some((*span, SemanticIssueKind::CompositionChildScopeWithoutParent { scope_name: scope_name.clone() }))
        }
        CompositionIssue::InvalidScopeQualifier { qualifier, span } => {
            Some((*span, SemanticIssueKind::CompositionInvalidScopeQualifier { qualifier: qualifier.clone() }))
        }
        CompositionIssue::OverrideLifetimeMismatch { binding, span } => {
            Some((*span, SemanticIssueKind::CompositionOverrideLifetimeMismatch { binding: binding.clone() }))
        }
        CompositionIssue::InjectOnConstructor { span } => {
            Some((*span, SemanticIssueKind::CompositionInjectOnConstructor))
        }
        CompositionIssue::HostInModProject { span } => Some((*span, SemanticIssueKind::CompositionHostInModProject)),
    }
}

pub fn composition_issue_code(issue: &CompositionIssue) -> &'static str {
    match issue {
        CompositionIssue::MissingLaunchHost { .. } => "E1701",
        CompositionIssue::MultipleLaunchHosts { .. } => "E1702",
        CompositionIssue::DependencyCycle { .. } => "E1703",
        CompositionIssue::UnresolvedInject { .. } => "E1704",
        CompositionIssue::AmbiguousInject { .. } => "E1705",
        CompositionIssue::ScopedOutsideWith { .. } => "E1706",
        CompositionIssue::ChildScopeWithoutParent { .. } => "E1707",
        CompositionIssue::WithArgsMismatch { .. } => "E1708",
        CompositionIssue::UnknownLaunchHost { .. } => "E1709",
        CompositionIssue::HostInModProject { .. } => "E1710",
        CompositionIssue::HostInheritanceCycle { .. } => "E1715",
        CompositionIssue::InjectOnConstructor { .. } => "E1712",
        CompositionIssue::OverrideLifetimeMismatch { .. } => "E1713",
        CompositionIssue::InvalidScopeQualifier { .. } => "E1714",
        CompositionIssue::DuplicateScopeName { .. } => "E1716",
        CompositionIssue::UnknownParentScope { .. } => "E1707",
        CompositionIssue::UnknownRegistrationId { .. } => "E1704",
        CompositionIssue::EmptyPluralInject { .. } => "E1704",
    }
}
