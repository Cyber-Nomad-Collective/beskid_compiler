use super::authorization::{AuthorizationDecision, Principal, authorize_administration};

/// Publisher state owned by pckg, keyed solely by the Auth Hub subject.
///
/// The HTTP adapter must only construct this from a verified Auth Hub session
/// (for GitHub-only pckg this is a `github:<numeric-id>` subject).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublisherProfile {
    subject: String,
    is_verified: bool,
}

impl PublisherProfile {
    pub fn unverified(subject: impl Into<String>) -> Self {
        Self { subject: subject.into(), is_verified: false }
    }

    pub fn verified(subject: impl Into<String>) -> Self {
        Self { subject: subject.into(), is_verified: true }
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn is_verified(&self) -> bool {
        self.is_verified
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublisherVerificationDecision {
    Denied,
    Unchanged(PublisherProfile),
    Updated(PublisherProfile),
}

/// Only registry administrators may change publisher verification.
pub fn decide_publisher_verification(
    administrator: &Principal,
    publisher: &PublisherProfile,
    verified: bool,
) -> PublisherVerificationDecision {
    if authorize_administration(administrator) == AuthorizationDecision::Denied {
        return PublisherVerificationDecision::Denied;
    }

    if publisher.is_verified == verified {
        return PublisherVerificationDecision::Unchanged(publisher.clone());
    }

    let updated = if verified {
        PublisherProfile::verified(publisher.subject())
    } else {
        PublisherProfile::unverified(publisher.subject())
    };
    PublisherVerificationDecision::Updated(updated)
}
