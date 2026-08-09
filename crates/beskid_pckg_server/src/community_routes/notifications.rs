use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use beskid_pckg_community::{NotificationPreference, NotificationScope};
use beskid_pckg_store::AsyncCommunityRepository;

use super::{
    auth::authenticated_principal,
    contracts::{
        NotificationAction, NotificationActionRequest, NotificationPreferenceMode, NotificationPreferenceRequest,
    },
    error::{community_error, store_error},
    responses::{NotificationPreferenceResponse, NotificationResponse},
    state::{CommunityBackend, CommunityState, now_unix_seconds},
};

pub(super) async fn list_notifications(State(state): State<CommunityState>, headers: HeaderMap) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let subject = principal.subject().expect("authenticated principal has subject");
    match &state.backend {
        CommunityBackend::InMemory(service) => Json(
            service
                .lock()
                .expect("community service lock is not poisoned")
                .notifications_for(subject)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>(),
        )
        .into_response(),
        CommunityBackend::Sqlx(repository) => match repository.list_notifications(subject.as_str()).await {
            Ok(notifications) => Json(
                notifications
                    .into_iter()
                    .map(|notification| NotificationResponse {
                        id: notification.id,
                        recipient: notification.recipient_subject,
                        scope: notification.scope,
                        actor: notification.actor_subject,
                        post_id: notification.post_id,
                        comment_id: notification.comment_id,
                        is_read: notification.read_at_unix_seconds.is_some(),
                    })
                    .collect::<Vec<_>>(),
            )
            .into_response(),
            Err(error) => store_error(error),
        },
    }
}

pub(super) async fn update_notification_preferences(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Json(request): Json<NotificationPreferenceRequest>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let (memory_preference, store_preference) = match notification_preferences(request) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match &state.backend {
        CommunityBackend::InMemory(service) => {
            service.lock().expect("community service lock is not poisoned").set_notification_preference(
                principal.subject().expect("authenticated principal has subject").clone(),
                memory_preference,
            );
            StatusCode::NO_CONTENT.into_response()
        }
        CommunityBackend::Sqlx(repository) => {
            match repository
                .set_notification_preference(
                    principal.subject().expect("authenticated principal has subject").as_str(),
                    store_preference,
                    now_unix_seconds(),
                )
                .await
            {
                Ok(()) => StatusCode::NO_CONTENT.into_response(),
                Err(error) => store_error(error),
            }
        }
    }
}

// This route-level helper intentionally returns Axum's complete rejection so
// the public handler preserves the standard error body without allocation.
#[allow(clippy::result_large_err)]
fn notification_preferences(
    request: NotificationPreferenceRequest,
) -> Result<(NotificationPreference, beskid_pckg_store::CommunityNotificationPreference), Response> {
    let store_preference = if let Some(preferences) = request.preferences {
        beskid_pckg_store::CommunityNotificationPreference {
            system_enabled: preferences.system_enabled,
            mention_enabled: preferences.mention_enabled,
            reply_enabled: preferences.reply_enabled,
            followed_publisher_post_enabled: preferences.followed_publisher_post_enabled,
            moderation_enabled: preferences.moderation_enabled,
        }
    } else {
        match request.mode {
            Some(NotificationPreferenceMode::All) => beskid_pckg_store::CommunityNotificationPreference::default(),
            Some(NotificationPreferenceMode::MentionsOnly) => beskid_pckg_store::CommunityNotificationPreference {
                system_enabled: true,
                mention_enabled: true,
                reply_enabled: false,
                followed_publisher_post_enabled: false,
                moderation_enabled: false,
            },
            None => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"message":"notification preferences are required"})),
                )
                    .into_response());
            }
        }
    };
    let mut enabled = Vec::new();
    if store_preference.system_enabled {
        enabled.push(NotificationScope::System);
    }
    if store_preference.mention_enabled {
        enabled.push(NotificationScope::Mention);
    }
    if store_preference.reply_enabled {
        enabled.push(NotificationScope::Reply);
    }
    if store_preference.followed_publisher_post_enabled {
        enabled.push(NotificationScope::FollowedPublisherPost);
    }
    if store_preference.moderation_enabled {
        enabled.push(NotificationScope::Moderation);
    }
    Ok((NotificationPreference::from_enabled(enabled), store_preference))
}

// Axum responses intentionally carry the complete HTTP rejection payload here;
// boxing it would only add allocation and dereferencing at every route boundary.
#[allow(clippy::result_large_err)]
pub(super) async fn get_notification_preferences(State(state): State<CommunityState>, headers: HeaderMap) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let subject = principal.subject().expect("authenticated principal has subject");
    match &state.backend {
        CommunityBackend::InMemory(service) => {
            let preference =
                service.lock().expect("community service lock is not poisoned").notification_preference(subject);
            Json(NotificationPreferenceResponse {
                system_enabled: preference.allows(NotificationScope::System),
                mention_enabled: preference.allows(NotificationScope::Mention),
                reply_enabled: preference.allows(NotificationScope::Reply),
                followed_publisher_post_enabled: preference.allows(NotificationScope::FollowedPublisherPost),
                moderation_enabled: preference.allows(NotificationScope::Moderation),
            })
            .into_response()
        }
        CommunityBackend::Sqlx(repository) => match repository.notification_preference(subject.as_str()).await {
            Ok(value) => Json(NotificationPreferenceResponse {
                system_enabled: value.system_enabled,
                mention_enabled: value.mention_enabled,
                reply_enabled: value.reply_enabled,
                followed_publisher_post_enabled: value.followed_publisher_post_enabled,
                moderation_enabled: value.moderation_enabled,
            })
            .into_response(),
            Err(error) => store_error(error),
        },
    }
}
pub(super) async fn mark_notification_read(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Path(notification_id): Path<u64>,
) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .mark_notification_read(&principal, notification_id)
        {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => match repository
            .mark_notification_read(
                notification_id as i64,
                principal.subject().expect("authenticated principal has subject").as_str(),
                now_unix_seconds(),
            )
            .await
        {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(error) => store_error(error),
        },
    }
}

pub(super) async fn mark_all_notifications_read(State(state): State<CommunityState>, headers: HeaderMap) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match &state.backend {
        CommunityBackend::InMemory(service) => match service
            .lock()
            .expect("community service lock is not poisoned")
            .mark_all_notifications_read(&principal)
        {
            Ok(updated) => Json(serde_json::json!({"updated": updated})).into_response(),
            Err(error) => community_error(error),
        },
        CommunityBackend::Sqlx(repository) => match repository
            .mark_all_notifications_read(
                principal.subject().expect("authenticated principal has subject").as_str(),
                now_unix_seconds(),
            )
            .await
        {
            Ok(updated) => Json(serde_json::json!({"updated": updated})).into_response(),
            Err(error) => store_error(error),
        },
    }
}

pub(super) async fn send_test_notification(State(state): State<CommunityState>, headers: HeaderMap) -> Response {
    let principal = match authenticated_principal(&state, &headers) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match &state.backend {
        CommunityBackend::InMemory(service) => {
            match service.lock().expect("community service lock is not poisoned").create_test_notification(&principal) {
                Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({"id": id}))).into_response(),
                Err(error) => community_error(error),
            }
        }
        CommunityBackend::Sqlx(repository) => match repository
            .create_test_notification(
                principal.subject().expect("authenticated principal has subject").as_str(),
                now_unix_seconds(),
            )
            .await
        {
            Ok(notification) => (StatusCode::CREATED, Json(serde_json::json!({"id": notification.id}))).into_response(),
            Err(error) => store_error(error),
        },
    }
}

pub(super) async fn execute_notification_action(
    State(state): State<CommunityState>,
    headers: HeaderMap,
    Path(notification_id): Path<u64>,
    Json(request): Json<NotificationActionRequest>,
) -> Response {
    // The legacy endpoint accepted arbitrary handler names but its registered
    // handler always rejected them.  The Rust boundary exposes only the two
    // safe, deterministic actions, both equivalent to recipient-owned read.
    match request.action {
        NotificationAction::MarkRead | NotificationAction::Dismiss => {}
    }
    let response = mark_notification_read(State(state), headers, Path(notification_id)).await;
    if response.status().is_success() { Json(serde_json::json!({"handled": true})).into_response() } else { response }
}
