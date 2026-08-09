use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    content::{CommentId, PostId},
    identity::Subject,
};

pub type NotificationId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationScope {
    System,
    Mention,
    Reply,
    FollowedPublisherPost,
    Moderation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationPreference {
    enabled: BTreeSet<NotificationScope>,
}

impl NotificationPreference {
    pub fn all() -> Self {
        Self {
            enabled: BTreeSet::from([
                NotificationScope::System,
                NotificationScope::Mention,
                NotificationScope::Reply,
                NotificationScope::FollowedPublisherPost,
                NotificationScope::Moderation,
            ]),
        }
    }
    pub fn mentions_only() -> Self {
        Self { enabled: BTreeSet::from([NotificationScope::Mention]) }
    }
    pub fn from_enabled(enabled: impl IntoIterator<Item = NotificationScope>) -> Self {
        Self { enabled: enabled.into_iter().collect() }
    }
    pub fn allows(&self, scope: NotificationScope) -> bool {
        self.enabled.contains(&scope)
    }
}

impl Default for NotificationPreference {
    fn default() -> Self {
        Self::all()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub id: NotificationId,
    pub recipient: Subject,
    pub scope: NotificationScope,
    pub actor: Subject,
    pub post_id: Option<PostId>,
    pub comment_id: Option<CommentId>,
    pub is_read: bool,
}
