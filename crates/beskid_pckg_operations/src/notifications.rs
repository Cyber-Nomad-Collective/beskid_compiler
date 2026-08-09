use super::authorization::{AuthorizationDecision, Principal, authorize_administration};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum NotificationType {
    Unknown,
    PackageUpdated,
    PackagePublished,
    PackageFollowedPublisherUpdate,
    BoardThreadActivity,
    System,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NotificationScope {
    Global,
    Package(String),
    Thread(String),
}

impl NotificationScope {
    fn normalized(&self) -> Self {
        match self {
            Self::Global => Self::Global,
            Self::Package(id) => Self::Package(id.trim().to_owned()),
            Self::Thread(id) => Self::Thread(id.trim().to_owned()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationPreference {
    user_id: String,
    notification_type: NotificationType,
    scope: NotificationScope,
    send_email: bool,
    include_in_spotlight: bool,
}

impl NotificationPreference {
    pub fn new(
        user_id: impl Into<String>,
        notification_type: NotificationType,
        scope: NotificationScope,
        send_email: bool,
        include_in_spotlight: bool,
    ) -> Self {
        Self { user_id: user_id.into(), notification_type, scope: scope.normalized(), send_email, include_in_spotlight }
    }

    pub fn user_subject(&self) -> &str {
        &self.user_id
    }

    pub fn notification_type(&self) -> NotificationType {
        self.notification_type
    }

    pub fn scope(&self) -> &NotificationScope {
        &self.scope
    }

    pub fn send_email(&self) -> bool {
        self.send_email
    }

    pub fn include_in_spotlight(&self) -> bool {
        self.include_in_spotlight
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdministrativeNotificationPreferenceDecision {
    Denied,
    Upsert(NotificationPreference),
}

/// Administrative notification preference changes are a separate operation
/// from recipient-owned preference edits, and require the SuperAdmin role.
pub fn decide_administrative_notification_preference(
    administrator: &Principal,
    requested: NotificationPreference,
) -> AdministrativeNotificationPreferenceDecision {
    if authorize_administration(administrator) == AuthorizationDecision::Allowed {
        AdministrativeNotificationPreferenceDecision::Upsert(requested)
    } else {
        AdministrativeNotificationPreferenceDecision::Denied
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationDeliveryDecision {
    pub send_email: bool,
    pub include_in_spotlight: bool,
}

impl NotificationDeliveryDecision {
    pub const fn new(send_email: bool, include_in_spotlight: bool) -> Self {
        Self { send_email, include_in_spotlight }
    }
}

/// Scoped settings override the global setting; absence means no optional delivery.
pub fn notification_delivery_decision(
    preferences: &[NotificationPreference],
    user_id: &str,
    notification_type: NotificationType,
    scope: &NotificationScope,
) -> NotificationDeliveryDecision {
    let scope = scope.normalized();
    let exact = preferences.iter().find(|preference| {
        preference.user_id == user_id && preference.notification_type == notification_type && preference.scope == scope
    });
    let global = preferences.iter().find(|preference| {
        preference.user_id == user_id
            && preference.notification_type == notification_type
            && preference.scope == NotificationScope::Global
    });
    exact.or(global).map_or(NotificationDeliveryDecision::new(false, false), |preference| {
        NotificationDeliveryDecision::new(preference.send_email, preference.include_in_spotlight)
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Notification {
    id: String,
    user_id: String,
    notification_type: NotificationType,
    is_read: bool,
}

impl Notification {
    pub fn unread(id: impl Into<String>, user_id: impl Into<String>, notification_type: NotificationType) -> Self {
        Self { id: id.into(), user_id: user_id.into(), notification_type, is_read: false }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationReadDecision {
    NotFound,
    AlreadyRead,
    MarkedRead,
}

pub fn mark_notification_read(notification: &Notification, requester_id: &str) -> NotificationReadDecision {
    if notification.user_id != requester_id {
        NotificationReadDecision::NotFound
    } else if notification.is_read {
        NotificationReadDecision::AlreadyRead
    } else {
        NotificationReadDecision::MarkedRead
    }
}
