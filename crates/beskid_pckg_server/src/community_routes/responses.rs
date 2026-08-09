use beskid_pckg_community::{Comment, CommentId, Post, PostId};
use serde::Serialize;

#[derive(Serialize)]
pub(super) struct SetBoardLockedResponse {
    pub(super) success: bool,
    pub(super) message: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FollowResponse {
    pub(super) is_following: bool,
    pub(super) changed: bool,
}

#[derive(Serialize)]
pub(super) struct FollowCountResponse {
    pub(super) count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct VoteResponse {
    pub(super) score: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PostResponse {
    pub(super) id: PostId,
    pub(super) board_id: String,
    pub(super) author: String,
    pub(super) title: String,
    pub(super) content: String,
    pub(super) score: i32,
}

impl PostResponse {
    pub(super) fn from_post(post: Post, board_id: String) -> Self {
        Self {
            id: post.id,
            board_id,
            author: post.author.as_str().to_owned(),
            title: post.title,
            content: post.content,
            score: post.score,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CommentResponse {
    pub(super) id: CommentId,
    pub(super) post_id: PostId,
    pub(super) author: String,
    pub(super) content: String,
    pub(super) parent_comment_id: Option<CommentId>,
    pub(super) score: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BoardResponse {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) locked: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NotificationPreferenceResponse {
    pub(super) system_enabled: bool,
    pub(super) mention_enabled: bool,
    pub(super) reply_enabled: bool,
    pub(super) followed_publisher_post_enabled: bool,
    pub(super) moderation_enabled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NotificationResponse {
    pub(super) id: i64,
    pub(super) recipient: String,
    pub(super) scope: String,
    pub(super) actor: String,
    pub(super) post_id: Option<i64>,
    pub(super) comment_id: Option<i64>,
    pub(super) is_read: bool,
}
impl From<Comment> for CommentResponse {
    fn from(comment: Comment) -> Self {
        Self {
            id: comment.id,
            post_id: comment.post_id,
            author: comment.author.as_str().to_owned(),
            content: comment.content,
            parent_comment_id: comment.parent_comment_id,
            score: comment.score,
        }
    }
}
pub(super) fn post_response_from_store(post: beskid_pckg_store::CommunityPost) -> PostResponse {
    PostResponse {
        id: post.id as PostId,
        board_id: post.board_id,
        author: post.author_subject,
        title: post.title,
        content: post.content,
        score: post.score,
    }
}

pub(super) fn comment_response_from_store(comment: beskid_pckg_store::CommunityComment) -> CommentResponse {
    CommentResponse {
        id: comment.id as CommentId,
        post_id: comment.post_id as PostId,
        author: comment.author_subject,
        content: comment.content,
        parent_comment_id: comment.parent_comment_id.map(|id| id as CommentId),
        score: comment.score,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StoreProfileResponse {
    pub(super) subject: String,
    pub(super) display_name: String,
    pub(super) bio: String,
    pub(super) social_links: Vec<String>,
    pub(super) is_publisher_verified: bool,
}
pub(super) fn profile_response_from_store(profile: beskid_pckg_store::CommunityProfile) -> StoreProfileResponse {
    StoreProfileResponse {
        subject: profile.subject,
        display_name: profile.display_name,
        bio: profile.bio,
        social_links: serde_json::from_str(&profile.social_links_json).unwrap_or_default(),
        is_publisher_verified: profile.is_publisher_verified,
    }
}
