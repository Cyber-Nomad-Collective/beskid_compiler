//! Pure operations-domain rules for the pckg registry.

mod activity;
mod authorization;
mod config;
mod link_policy;
mod notifications;
mod permissions;
mod publishers;

pub use activity::{RegistryActivityEntry, RegistryActivityLog};
pub use authorization::{AuthorizationDecision, Principal, Role, authorize_administration};
pub use config::{AuthHubConfig, CaptchaConfig, ConfigValidationError, OperationsConfig};
pub use link_policy::{
    BLOCKED_LINK_REASON, BlockedLinkPattern, BlockedLinkPatternError, BlockedLinkPatterns, BlockedLinkPatternsError,
};
pub use notifications::{
    AdministrativeNotificationPreferenceDecision, Notification, NotificationDeliveryDecision, NotificationPreference,
    NotificationReadDecision, NotificationScope, NotificationType, decide_administrative_notification_preference,
    mark_notification_read, notification_delivery_decision,
};
pub use permissions::{
    Capability, PermissionGrantDecision, Resource, ResourceKind, ResourcePermission, authorize_package_moderation,
    decide_permission_grant,
};
pub use publishers::{PublisherProfile, PublisherVerificationDecision, decide_publisher_verification};

#[cfg(test)]
mod tests;
