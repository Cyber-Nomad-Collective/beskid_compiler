//! Pure operations-domain rules for the pckg registry.

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
        Self {
            subject: subject.into(),
            roles: roles.into_iter().collect(),
        }
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
    if principal.has_role(Role::SuperAdmin) {
        AuthorizationDecision::Allowed
    } else {
        AuthorizationDecision::Denied
    }
}

pub const BLOCKED_LINK_REASON: &str =
    "This content contains a link that is not allowed on this registry.";
const MAX_BLOCKED_LINK_PATTERN_LENGTH: usize = 512;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockedLinkPattern(String);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockedLinkPatternError {
    Empty,
    TooLong,
}

impl BlockedLinkPattern {
    pub fn new(pattern: impl AsRef<str>) -> Result<Self, BlockedLinkPatternError> {
        let pattern = pattern.as_ref().trim();
        if pattern.is_empty() {
            return Err(BlockedLinkPatternError::Empty);
        }
        if pattern.len() > MAX_BLOCKED_LINK_PATTERN_LENGTH {
            return Err(BlockedLinkPatternError::TooLong);
        }
        Ok(Self(pattern.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockedLinkPatterns(Vec<BlockedLinkPattern>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockedLinkPatternsError {
    Invalid(BlockedLinkPatternError),
    Duplicate,
}

impl BlockedLinkPatterns {
    pub fn from_patterns(
        patterns: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<Self, BlockedLinkPatternsError> {
        let mut seen = BTreeSet::new();
        let mut normalized = Vec::new();
        for value in patterns {
            let pattern =
                BlockedLinkPattern::new(value).map_err(BlockedLinkPatternsError::Invalid)?;
            if !seen.insert(pattern.0.to_ascii_lowercase()) {
                return Err(BlockedLinkPatternsError::Duplicate);
            }
            normalized.push(pattern);
        }
        Ok(Self(normalized))
    }

    pub fn patterns(&self) -> &[BlockedLinkPattern] {
        &self.0
    }

    /// Returns the legacy public reason when a URL-like segment contains a blocked pattern.
    pub fn block_reason(&self, text: impl AsRef<str>) -> Option<&'static str> {
        let text = text.as_ref();
        for segment in url_like_segments(text) {
            if self
                .0
                .iter()
                .any(|pattern| contains_ascii_case_insensitive(segment, pattern.as_str()))
            {
                return Some(BLOCKED_LINK_REASON);
            }
        }
        None
    }
}

fn url_like_segments(text: &str) -> impl Iterator<Item = &str> {
    text.split(|character: char| {
        character.is_whitespace() || matches!(character, '"' | '\'' | '<' | '>' | '(' | ')')
    })
    .filter(|segment| {
        let lower = segment.to_ascii_lowercase();
        lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("www.")
    })
}

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

/// Activity data is intentionally storage-neutral; adapters supply stable sequence IDs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryActivityEntry {
    sequence: u64,
    occurred_at_unix_seconds: i64,
    action: String,
    message: String,
}

impl RegistryActivityEntry {
    pub fn new(
        sequence: u64,
        occurred_at_unix_seconds: i64,
        action: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            sequence,
            occurred_at_unix_seconds,
            action: action.into(),
            message: message.into(),
        }
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryActivityLog {
    entries: Vec<RegistryActivityEntry>,
    capacity: usize,
}

impl RegistryActivityLog {
    pub const LEGACY_CAPACITY: usize = 500;

    pub fn legacy_compatible() -> Self {
        Self::with_capacity(Self::LEGACY_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "registry activity capacity must be positive");
        Self {
            entries: Vec::new(),
            capacity,
        }
    }

    pub fn append(&mut self, entry: RegistryActivityEntry) {
        self.entries.push(entry);
        self.entries.sort_by(|left, right| {
            right
                .occurred_at_unix_seconds
                .cmp(&left.occurred_at_unix_seconds)
                .then_with(|| right.sequence.cmp(&left.sequence))
        });
        self.entries.truncate(self.capacity);
    }

    pub fn entries(&self) -> &[RegistryActivityEntry] {
        &self.entries
    }

    pub fn recent(&self, take: usize) -> &[RegistryActivityEntry] {
        &self.entries[..take.min(self.capacity).min(self.entries.len())]
    }
}

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
        Self {
            user_id: user_id.into(),
            notification_type,
            scope: scope.normalized(),
            send_email,
            include_in_spotlight,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationDeliveryDecision {
    pub send_email: bool,
    pub include_in_spotlight: bool,
}

impl NotificationDeliveryDecision {
    pub const fn new(send_email: bool, include_in_spotlight: bool) -> Self {
        Self {
            send_email,
            include_in_spotlight,
        }
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
        preference.user_id == user_id
            && preference.notification_type == notification_type
            && preference.scope == scope
    });
    let global = preferences.iter().find(|preference| {
        preference.user_id == user_id
            && preference.notification_type == notification_type
            && preference.scope == NotificationScope::Global
    });
    exact.or(global).map_or(
        NotificationDeliveryDecision::new(false, false),
        |preference| {
            NotificationDeliveryDecision::new(
                preference.send_email,
                preference.include_in_spotlight,
            )
        },
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Notification {
    id: String,
    user_id: String,
    notification_type: NotificationType,
    is_read: bool,
}

impl Notification {
    pub fn unread(
        id: impl Into<String>,
        user_id: impl Into<String>,
        notification_type: NotificationType,
    ) -> Self {
        Self {
            id: id.into(),
            user_id: user_id.into(),
            notification_type,
            is_read: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationReadDecision {
    NotFound,
    AlreadyRead,
    MarkedRead,
}

pub fn mark_notification_read(
    notification: &Notification,
    requester_id: &str,
) -> NotificationReadDecision {
    if notification.user_id != requester_id {
        NotificationReadDecision::NotFound
    } else if notification.is_read {
        NotificationReadDecision::AlreadyRead
    } else {
        NotificationReadDecision::MarkedRead
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct CaptchaConfig {
    pub site_key: Option<String>,
    pub project_id: Option<String>,
    pub api_key: Option<String>,
    pub minimum_score_milli: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct AuthHubConfig {
    pub hub_public_url: Option<String>,
    pub public_url: Option<String>,
    pub pairing_approver_login: Option<String>,
    pub github_sync_token: Option<String>,
    pub service_token: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct OperationsConfig {
    pub captcha: CaptchaConfig,
    pub auth_hub: AuthHubConfig,
    pub session_secret: Option<String>,
    pub require_structured_api_doc: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigValidationError {
    IncompleteCaptcha,
    InvalidCaptchaMinimumScore,
    IncompleteAuthHubSession,
    InvalidHubPublicUrl,
    InvalidPublicUrl,
}

impl OperationsConfig {
    pub fn for_test() -> Self {
        Self {
            require_structured_api_doc: true,
            ..Self::default()
        }
    }

    /// Validates the operational configuration before adapters bind network or secret clients.
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        let captcha_values = [
            &self.captcha.site_key,
            &self.captcha.project_id,
            &self.captcha.api_key,
        ];
        let configured_captcha_values = captcha_values
            .iter()
            .filter(|value| is_present(value))
            .count();
        if configured_captcha_values != 0 && configured_captcha_values != captcha_values.len() {
            return Err(ConfigValidationError::IncompleteCaptcha);
        }
        if self.captcha.minimum_score_milli > 1000 {
            return Err(ConfigValidationError::InvalidCaptchaMinimumScore);
        }
        if is_present(&self.auth_hub.service_token) != is_present(&self.session_secret) {
            return Err(ConfigValidationError::IncompleteAuthHubSession);
        }
        if is_present(&self.auth_hub.hub_public_url)
            && !is_http_url(self.auth_hub.hub_public_url.as_deref().unwrap())
        {
            return Err(ConfigValidationError::InvalidHubPublicUrl);
        }
        if is_present(&self.auth_hub.public_url)
            && !is_http_url(self.auth_hub.public_url.as_deref().unwrap())
        {
            return Err(ConfigValidationError::InvalidPublicUrl);
        }
        Ok(())
    }
}

fn is_present(value: &Option<String>) -> bool {
    value
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
}

fn is_http_url(value: &str) -> bool {
    let value = value.trim();
    value.starts_with("https://") || value.starts_with("http://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_super_admins_are_authorized_for_administration() {
        assert_eq!(
            authorize_administration(&Principal::new("admin", [Role::SuperAdmin])),
            AuthorizationDecision::Allowed
        );
        assert_eq!(
            authorize_administration(&Principal::new("moderator", [Role::Moderator])),
            AuthorizationDecision::Denied
        );
    }

    #[test]
    fn blocked_links_match_url_segments_case_insensitively() {
        let patterns =
            BlockedLinkPatterns::from_patterns(["spam.example"]).expect("a usable blocked pattern");

        assert_eq!(
            patterns.block_reason("See HTTPS://SPAM.EXAMPLE/offer."),
            Some(BLOCKED_LINK_REASON)
        );
        assert_eq!(patterns.block_reason("spam.example without a URL"), None);
    }

    #[test]
    fn blocked_link_patterns_are_trimmed_and_reject_duplicates() {
        assert_eq!(
            BlockedLinkPattern::new("   ").unwrap_err(),
            BlockedLinkPatternError::Empty
        );
        assert_eq!(
            BlockedLinkPatterns::from_patterns(["spam.example", " SPAM.EXAMPLE "]).unwrap_err(),
            BlockedLinkPatternsError::Duplicate
        );
    }

    #[test]
    fn activity_log_retains_the_newest_500_entries() {
        let mut log = RegistryActivityLog::legacy_compatible();
        for sequence in 0..501 {
            log.append(RegistryActivityEntry::new(
                sequence,
                sequence as i64,
                "publish",
                "done",
            ));
        }

        assert_eq!(log.entries().len(), 500);
        assert_eq!(log.entries().first().unwrap().sequence(), 500);
        assert_eq!(log.entries().last().unwrap().sequence(), 1);
        assert_eq!(log.recent(501).len(), 500);
    }

    #[test]
    fn a_scoped_notification_preference_overrides_the_global_preference() {
        let preferences = vec![
            NotificationPreference::new(
                "user-1",
                NotificationType::PackagePublished,
                NotificationScope::Global,
                true,
                false,
            ),
            NotificationPreference::new(
                "user-1",
                NotificationType::PackagePublished,
                NotificationScope::Package("beskid.core".into()),
                false,
                true,
            ),
        ];

        assert_eq!(
            notification_delivery_decision(
                &preferences,
                "user-1",
                NotificationType::PackagePublished,
                &NotificationScope::Package("  beskid.core ".into()),
            ),
            NotificationDeliveryDecision::new(false, true)
        );
    }

    #[test]
    fn marking_a_notification_read_requires_recipient_ownership() {
        let notification = Notification::unread("notice-1", "owner", NotificationType::System);

        assert_eq!(
            mark_notification_read(&notification, "other"),
            NotificationReadDecision::NotFound
        );
        assert_eq!(
            mark_notification_read(&notification, "owner"),
            NotificationReadDecision::MarkedRead
        );
    }

    #[test]
    fn startup_configuration_requires_complete_captcha_and_auth_secrets() {
        let mut config = OperationsConfig::for_test();
        config.captcha.site_key = Some("site".into());
        assert_eq!(
            config.validate().unwrap_err(),
            ConfigValidationError::IncompleteCaptcha
        );

        config.captcha.project_id = Some("project".into());
        config.captcha.api_key = Some("api".into());
        config.auth_hub.service_token = Some("service".into());
        config.session_secret = Some("session".into());
        assert_eq!(config.validate(), Ok(()));
    }
}
