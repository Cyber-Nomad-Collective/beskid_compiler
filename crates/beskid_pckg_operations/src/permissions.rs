use super::authorization::{AuthorizationDecision, Principal, Role};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ResourceKind {
    Package,
    Board,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Resource {
    kind: ResourceKind,
    id: String,
}

impl Resource {
    pub fn package(id: impl Into<String>) -> Self {
        Self { kind: ResourceKind::Package, id: id.into() }
    }

    pub fn board(id: impl Into<String>) -> Self {
        Self { kind: ResourceKind::Board, id: id.into() }
    }

    pub fn kind(&self) -> ResourceKind {
        self.kind
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Capability {
    Moderate,
}

/// A durable permission row: the storage adapter enforces uniqueness on
/// `(subject, resource_kind, resource_id, capability)`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourcePermission {
    subject: String,
    resource: Resource,
    capability: Capability,
    granted_by_subject: String,
    granted_at_unix_seconds: i64,
}

impl ResourcePermission {
    pub fn moderate(
        subject: impl Into<String>,
        resource: Resource,
        granted_by_subject: impl Into<String>,
        granted_at_unix_seconds: i64,
    ) -> Self {
        Self {
            subject: subject.into(),
            resource,
            capability: Capability::Moderate,
            granted_by_subject: granted_by_subject.into(),
            granted_at_unix_seconds,
        }
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn resource(&self) -> &Resource {
        &self.resource
    }

    pub fn capability(&self) -> Capability {
        self.capability
    }

    pub fn granted_by_subject(&self) -> &str {
        &self.granted_by_subject
    }

    pub fn granted_at_unix_seconds(&self) -> i64 {
        self.granted_at_unix_seconds
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PermissionGrantDecision {
    AlreadyGranted,
    Granted(ResourcePermission),
}

pub fn decide_permission_grant(
    existing: &[ResourcePermission],
    requested: ResourcePermission,
) -> PermissionGrantDecision {
    if existing.iter().any(|permission| {
        permission.subject == requested.subject
            && permission.resource == requested.resource
            && permission.capability == requested.capability
    }) {
        PermissionGrantDecision::AlreadyGranted
    } else {
        PermissionGrantDecision::Granted(requested)
    }
}

/// Package owners, global moderators, SuperAdmins, and explicitly granted
/// package moderators may moderate package-owned community content.
pub fn authorize_package_moderation(
    principal: &Principal,
    owner_subject: &str,
    package_id: &str,
    permissions: &[ResourcePermission],
) -> AuthorizationDecision {
    if principal.has_role(Role::SuperAdmin)
        || principal.has_role(Role::Moderator)
        || principal.subject() == owner_subject
    {
        return AuthorizationDecision::Allowed;
    }

    let resource = Resource::package(package_id);
    if permissions.iter().any(|permission| {
        permission.subject == principal.subject()
            && permission.resource == resource
            && permission.capability == Capability::Moderate
    }) {
        AuthorizationDecision::Allowed
    } else {
        AuthorizationDecision::Denied
    }
}
