use std::collections::BTreeSet;

/// Roles emitted by the identity boundary that matter to registry operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Role {
    User,
    Moderator,
    SuperAdmin,
}

/// Authenticated actor passed from the HTTP/auth adapter into domain rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Principal {
    subject: String,
    roles: BTreeSet<Role>,
}

impl Principal {
    pub fn new(subject: impl Into<String>, roles: impl IntoIterator<Item = Role>) -> Self {
        Self { subject: subject.into(), roles: roles.into_iter().collect() }
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn has_role(&self, role: Role) -> bool {
        self.roles.contains(&role)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorizationDecision {
    Allowed,
    Denied,
}

/// C# administration endpoints uniformly require the SuperAdmin role.
pub fn authorize_administration(principal: &Principal) -> AuthorizationDecision {
    if principal.has_role(Role::SuperAdmin) { AuthorizationDecision::Allowed } else { AuthorizationDecision::Denied }
}
