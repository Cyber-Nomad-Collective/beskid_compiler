use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::errors::CommunityError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Subject(String);

impl Subject {
    pub fn new(value: impl Into<String>) -> Result<Self, CommunityError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CommunityError::InvalidSubject);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Role {
    User,
    Moderator,
    SuperAdmin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ApiKeyScope {
    Read,
    Publish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Permission {
    Read,
    Publish,
    Moderate,
    VerifyPublisher,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    Anonymous,
    AuthHub { subject: Subject, roles: BTreeSet<Role> },
    ApiKey { subject: Subject, scopes: BTreeSet<ApiKeyScope> },
}

impl Principal {
    pub fn auth_hub(subject: Subject, roles: impl IntoIterator<Item = Role>) -> Self {
        Self::AuthHub { subject, roles: roles.into_iter().collect() }
    }

    pub fn api_key(subject: Subject, scopes: impl IntoIterator<Item = ApiKeyScope>) -> Self {
        Self::ApiKey { subject, scopes: scopes.into_iter().collect() }
    }

    pub fn subject(&self) -> Option<&Subject> {
        match self {
            Self::Anonymous => None,
            Self::AuthHub { subject, .. } | Self::ApiKey { subject, .. } => Some(subject),
        }
    }

    pub fn allows(&self, permission: Permission) -> bool {
        match self {
            Self::Anonymous => false,
            Self::AuthHub { roles, .. } => match permission {
                Permission::Read => true,
                Permission::Publish => {
                    roles.contains(&Role::User) || roles.contains(&Role::Moderator) || roles.contains(&Role::SuperAdmin)
                }
                Permission::Moderate => roles.contains(&Role::Moderator) || roles.contains(&Role::SuperAdmin),
                Permission::VerifyPublisher => roles.contains(&Role::SuperAdmin),
            },
            Self::ApiKey { scopes, .. } => match permission {
                Permission::Read => scopes.contains(&ApiKeyScope::Read) || scopes.contains(&ApiKeyScope::Publish),
                Permission::Publish => scopes.contains(&ApiKeyScope::Publish),
                Permission::Moderate | Permission::VerifyPublisher => false,
            },
        }
    }
}
