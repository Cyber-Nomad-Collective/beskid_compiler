use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::syntax::SpanInfo;

use super::model::ScopeId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositionIssue {
    MissingLaunchHost {
        span: Option<SpanInfo>,
    },
    MultipleLaunchHosts {
        first_span: Option<SpanInfo>,
        second_span: Option<SpanInfo>,
    },
    UnknownLaunchHost {
        host_name: String,
        span: SpanInfo,
    },
    HostInheritanceCycle {
        host_name: String,
        span: SpanInfo,
    },
    DuplicateScopeName {
        scope_name: String,
        span: SpanInfo,
        first_scope: ScopeId,
        second_scope: ScopeId,
    },
    UnknownParentScope {
        scope_name: String,
        parent_scope: ScopeId,
        span: SpanInfo,
    },
    UnknownRegistrationId {
        registration_id: u32,
        span: Option<SpanInfo>,
    },
    DependencyCycle {
        from_id: u32,
        to_id: u32,
        span: Option<SpanInfo>,
    },
    /// A singular inject matched several registrations at one container level.
    AmbiguousInject {
        requested_type: String,
        span: SpanInfo,
        field_name: String,
        name_span: SpanInfo,
        type_span: SpanInfo,
        candidates: Vec<SpanInfo>,
    },
    UnresolvedInject {
        requested_type: String,
        span: SpanInfo,
    },
    EmptyPluralInject {
        requested_type: String,
        span: SpanInfo,
    },
    WithArgsMismatch {
        scope_name: String,
        span: SpanInfo,
    },
    ScopedOutsideWith {
        span: SpanInfo,
    },
    ChildScopeWithoutParent {
        scope_name: String,
        span: SpanInfo,
    },
    InvalidScopeQualifier {
        qualifier: String,
        span: SpanInfo,
    },
    /// A derived host re-registers `binding` with another lifetime than its base host.
    OverrideLifetimeMismatch {
        binding: String,
        span: SpanInfo,
        lifetime: &'static str,
        /// The overridden base registration, when it is in the unit under analysis.
        overridden_span: Option<SpanInfo>,
        overridden_lifetime: &'static str,
    },
    /// `startup` runs once per launch at global scope; it cannot be declared inside a scope.
    StartupInNamedScope {
        span: SpanInfo,
    },
    /// A registry line names a contract as the implementation: a contract cannot be created.
    RegisteredContract {
        contract: String,
        /// The contract key of a `for Target` line; `None` for a self-registration.
        target: Option<String>,
        span: SpanInfo,
        implementation_span: SpanInfo,
        /// Types of the unit that conform to the contract and can be registered.
        candidates: Vec<String>,
    },
    /// The registered implementation is a type the container cannot create.
    UnconstructibleRegistration {
        implementation: String,
        reason: UnconstructibleReason,
        span: SpanInfo,
    },
    /// `single Impl for Contract;` where `Impl` does not conform to `Contract`.
    NonConformingRegistration {
        implementation: String,
        contract: String,
        span: SpanInfo,
    },
    /// A `with` that no earlier `launch` of the same function body encloses.
    WithWithoutLaunch {
        scope_name: String,
        span: SpanInfo,
    },
    InjectOnConstructor {
        span: SpanInfo,
    },
    HostInModProject {
        span: SpanInfo,
    },
    /// `launch Host(...)` passes fewer arguments than the host's required parameters or more than
    /// all its parameters (`required..=total`).
    /// `cross_unit`: the host is declared in another unit, which cannot take `launch` arguments.
    LaunchArgumentCount {
        host_name: String,
        required: usize,
        total: usize,
        found: usize,
        cross_unit: bool,
        span: SpanInfo,
    },
    /// `with Scope(...)` passes a number of arguments outside the scope's `required..=total`.
    WithArgumentCount {
        scope_name: String,
        required: usize,
        total: usize,
        found: usize,
        span: SpanInfo,
    },
    /// A host used as a base declares parameters (on itself or on one of its scopes); only the
    /// launched host receives `launch` and `with` arguments.
    BaseHostParameters {
        host_name: String,
        span: SpanInfo,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnconstructibleReason {
    /// The type has this value field, which the container cannot initialize.
    ValueField(String),
    Generic,
    Enum,
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
        CompositionIssue::AmbiguousInject { requested_type, span, .. } => {
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
        CompositionIssue::OverrideLifetimeMismatch { binding, span, .. } => {
            Some((*span, SemanticIssueKind::CompositionOverrideLifetimeMismatch { binding: binding.clone() }))
        }
        CompositionIssue::InjectOnConstructor { span } => {
            Some((*span, SemanticIssueKind::CompositionInjectOnConstructor))
        }
        CompositionIssue::HostInModProject { span } => Some((*span, SemanticIssueKind::CompositionHostInModProject)),
        CompositionIssue::StartupInNamedScope { .. }
        | CompositionIssue::RegisteredContract { .. }
        | CompositionIssue::UnconstructibleRegistration { .. }
        | CompositionIssue::NonConformingRegistration { .. }
        | CompositionIssue::WithWithoutLaunch { .. }
        | CompositionIssue::LaunchArgumentCount { .. }
        | CompositionIssue::WithArgumentCount { .. }
        | CompositionIssue::BaseHostParameters { .. } => None,
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
        CompositionIssue::StartupInNamedScope { .. } => "E1717",
        CompositionIssue::RegisteredContract { .. } => "E1718",
        CompositionIssue::UnconstructibleRegistration { .. } => "E1719",
        CompositionIssue::NonConformingRegistration { .. } => "E1720",
        CompositionIssue::WithWithoutLaunch { .. } => "E1721",
        CompositionIssue::LaunchArgumentCount { .. } => "E1722",
        CompositionIssue::WithArgumentCount { .. } => "E1708",
        CompositionIssue::BaseHostParameters { .. } => "E1723",
    }
}
