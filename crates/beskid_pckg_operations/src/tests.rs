use super::{
    AdministrativeNotificationPreferenceDecision, AuthorizationDecision, BLOCKED_LINK_REASON, BlockedLinkPattern,
    BlockedLinkPatternError, BlockedLinkPatterns, BlockedLinkPatternsError, ConfigValidationError, Notification,
    NotificationDeliveryDecision, NotificationPreference, NotificationReadDecision, NotificationScope,
    NotificationType, OperationsConfig, PermissionGrantDecision, Principal, PublisherProfile,
    PublisherVerificationDecision, RegistryActivityEntry, RegistryActivityLog, Resource, ResourcePermission, Role,
    authorize_administration, authorize_package_moderation, decide_administrative_notification_preference,
    decide_permission_grant, decide_publisher_verification, mark_notification_read, notification_delivery_decision,
};

#[test]
fn only_super_admins_are_authorized_for_administration() {
    assert_eq!(authorize_administration(&Principal::new("admin", [Role::SuperAdmin])), AuthorizationDecision::Allowed);
    assert_eq!(
        authorize_administration(&Principal::new("moderator", [Role::Moderator])),
        AuthorizationDecision::Denied
    );
}

#[test]
fn blocked_links_match_url_segments_case_insensitively() {
    let patterns = BlockedLinkPatterns::from_patterns(["spam.example"]).expect("a usable blocked pattern");

    assert_eq!(patterns.block_reason("See HTTPS://SPAM.EXAMPLE/offer."), Some(BLOCKED_LINK_REASON));
    assert_eq!(patterns.block_reason("spam.example without a URL"), None);
}

#[test]
fn blocked_link_patterns_are_trimmed_and_reject_duplicates() {
    assert_eq!(BlockedLinkPattern::new("   ").unwrap_err(), BlockedLinkPatternError::Empty);
    assert_eq!(
        BlockedLinkPatterns::from_patterns(["spam.example", " SPAM.EXAMPLE "]).unwrap_err(),
        BlockedLinkPatternsError::Duplicate
    );
}

#[test]
fn activity_log_retains_the_newest_500_entries() {
    let mut log = RegistryActivityLog::legacy_compatible();
    for sequence in 0..501 {
        log.append(RegistryActivityEntry::new(sequence, sequence as i64, "publish", "done"));
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

    assert_eq!(mark_notification_read(&notification, "other"), NotificationReadDecision::NotFound);
    assert_eq!(mark_notification_read(&notification, "owner"), NotificationReadDecision::MarkedRead);
}

#[test]
fn startup_configuration_requires_complete_captcha_and_auth_secrets() {
    let mut config = OperationsConfig::for_test();
    config.captcha.site_key = Some("site".into());
    assert_eq!(config.validate().unwrap_err(), ConfigValidationError::IncompleteCaptcha);

    config.captcha.project_id = Some("project".into());
    config.captcha.api_key = Some("api".into());
    config.auth_hub.service_token = Some("service".into());
    config.session_secret = Some("session".into());
    assert_eq!(config.validate(), Ok(()));
}

#[test]
fn publisher_verification_is_admin_controlled_and_subject_keyed() {
    let publisher = PublisherProfile::unverified("github:42");
    let member = Principal::new("github:42", [Role::User]);
    let administrator = Principal::new("github:1", [Role::SuperAdmin]);

    assert_eq!(decide_publisher_verification(&member, &publisher, true), PublisherVerificationDecision::Denied);
    assert_eq!(
        decide_publisher_verification(&administrator, &publisher, true),
        PublisherVerificationDecision::Updated(PublisherProfile::verified("github:42"))
    );
}

#[test]
fn package_owner_or_granted_moderator_can_moderate_a_package() {
    let owner = Principal::new("github:42", [Role::User]);
    let collaborator = Principal::new("github:43", [Role::User]);
    let permission = ResourcePermission::moderate("github:43", Resource::package("pkg-1"), "github:1", 1);

    assert_eq!(authorize_package_moderation(&owner, "github:42", "pkg-1", &[]), AuthorizationDecision::Allowed);
    assert_eq!(
        authorize_package_moderation(&collaborator, "github:42", "pkg-1", &[permission]),
        AuthorizationDecision::Allowed
    );
}

#[test]
fn permission_grants_are_idempotent_by_subject_resource_and_capability() {
    let resource = Resource::board("board-9");
    let grant = ResourcePermission::moderate("github:42", resource.clone(), "github:1", 1);

    assert_eq!(
        decide_permission_grant(std::slice::from_ref(&grant), grant.clone()),
        PermissionGrantDecision::AlreadyGranted
    );
    assert!(matches!(
        decide_permission_grant(&[], ResourcePermission::moderate("github:42", resource, "github:1", 1)),
        PermissionGrantDecision::Granted(_)
    ));
}

#[test]
fn administrative_notification_preference_changes_only_affect_the_target_subject() {
    let administrator = Principal::new("github:1", [Role::SuperAdmin]);
    let member = Principal::new("github:42", [Role::User]);
    let requested =
        NotificationPreference::new("github:42", NotificationType::System, NotificationScope::Global, true, true);

    assert_eq!(
        decide_administrative_notification_preference(&member, requested.clone()),
        AdministrativeNotificationPreferenceDecision::Denied
    );
    assert_eq!(
        decide_administrative_notification_preference(&administrator, requested.clone()),
        AdministrativeNotificationPreferenceDecision::Upsert(requested)
    );
}
