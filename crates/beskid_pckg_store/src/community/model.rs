use async_trait::async_trait;

/// Community persistence failures deliberately distinguish authorization-like
/// ownership violations from missing resources so HTTP adapters can preserve
/// the legacy registry's non-disclosure policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunityStoreError {
    InvalidAuthHubSubject,
    InvalidBoardId,
    InvalidContent,
    InvalidPackageId,
    ProfileNotFound,
    BoardNotFound,
    PostNotFound,
    CommentNotFound,
    NotificationNotFound,
    SelfVote,
    ParentCommentOutsidePost,
    Database(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityProfile {
    pub subject: String,
    pub display_name: String,
    pub bio: String,
    pub social_links_json: String,
    pub is_publisher_verified: bool,
    pub updated_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityBoard {
    pub id: String,
    pub title: String,
    pub locked: bool,
    pub created_at_unix_seconds: i64,
    pub updated_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityPost {
    pub id: i64,
    pub board_id: String,
    pub author_subject: String,
    pub title: String,
    pub content: String,
    pub score: i32,
    pub created_at_unix_seconds: i64,
    pub updated_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityComment {
    pub id: i64,
    pub post_id: i64,
    pub author_subject: String,
    pub content: String,
    pub parent_comment_id: Option<i64>,
    pub score: i32,
    pub created_at_unix_seconds: i64,
    pub updated_at_unix_seconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommunityVote {
    Up,
    Down,
    Clear,
}

impl CommunityVote {
    fn value(self) -> i16 {
        match self {
            Self::Up => 1,
            Self::Down => -1,
            Self::Clear => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityNotificationPreference {
    pub system_enabled: bool,
    pub mention_enabled: bool,
    pub reply_enabled: bool,
    pub followed_publisher_post_enabled: bool,
    pub moderation_enabled: bool,
}

impl Default for CommunityNotificationPreference {
    fn default() -> Self {
        Self {
            system_enabled: true,
            mention_enabled: true,
            reply_enabled: true,
            followed_publisher_post_enabled: true,
            moderation_enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityNotification {
    pub id: i64,
    pub recipient_subject: String,
    pub scope: String,
    pub actor_subject: String,
    pub post_id: Option<i64>,
    pub comment_id: Option<i64>,
    pub created_at_unix_seconds: i64,
    pub read_at_unix_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCommunityReview {
    pub id: String,
    pub package_id: String,
    pub author_subject: String,
    pub rating: i16,
    pub comment: String,
    pub created_at_unix_seconds: i64,
    pub updated_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageCommunityReviewError {
    InvalidAuthHubSubject,
    InvalidPackageId,
    InvalidRating,
    InvalidComment,
    Database(String),
}

#[async_trait]
pub trait AsyncPackageCommunityReviewRepository: Send + Sync {
    async fn upsert_package_community_review(
        &self,
        review: PackageCommunityReview,
    ) -> Result<PackageCommunityReview, PackageCommunityReviewError>;
    async fn list_package_community_reviews(
        &self,
        package_id: &str,
    ) -> Result<Vec<PackageCommunityReview>, PackageCommunityReviewError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCommunityNotification {
    pub recipient_subject: String,
    pub scope: String,
    pub actor_subject: String,
    pub post_id: Option<i64>,
    pub comment_id: Option<i64>,
    pub now_unix_seconds: i64,
}

/// Async boundary consumed by a future Axum state adapter. Actor subjects are
/// explicit parameters: a repository never accepts a legacy Identity id or
/// derives identity from display data.
#[async_trait]
pub trait AsyncCommunityRepository: Send + Sync {
    async fn upsert_profile(&self, profile: CommunityProfile) -> Result<CommunityProfile, CommunityStoreError>;
    async fn profile(&self, subject: &str) -> Result<Option<CommunityProfile>, CommunityStoreError>;
    async fn boards(&self) -> Result<Vec<CommunityBoard>, CommunityStoreError>;
    async fn board(&self, board_id: &str) -> Result<Option<CommunityBoard>, CommunityStoreError>;
    async fn posts_for_board(&self, board_id: &str) -> Result<Vec<CommunityPost>, CommunityStoreError>;
    async fn post(&self, post_id: i64) -> Result<Option<CommunityPost>, CommunityStoreError>;
    async fn comments_for_post(&self, post_id: i64) -> Result<Vec<CommunityComment>, CommunityStoreError>;
    async fn create_board(&self, board: CommunityBoard) -> Result<CommunityBoard, CommunityStoreError>;
    async fn create_post(
        &self,
        board_id: &str,
        author_subject: &str,
        title: &str,
        content: &str,
        now_unix_seconds: i64,
    ) -> Result<CommunityPost, CommunityStoreError>;
    async fn create_comment(
        &self,
        post_id: i64,
        author_subject: &str,
        content: &str,
        parent_comment_id: Option<i64>,
        now_unix_seconds: i64,
    ) -> Result<CommunityComment, CommunityStoreError>;
    async fn vote_on_post(
        &self,
        post_id: i64,
        voter_subject: &str,
        vote: CommunityVote,
        now_unix_seconds: i64,
    ) -> Result<i32, CommunityStoreError>;
    async fn vote_on_comment(
        &self,
        comment_id: i64,
        voter_subject: &str,
        vote: CommunityVote,
        now_unix_seconds: i64,
    ) -> Result<i32, CommunityStoreError>;
    async fn toggle_publisher_follow(
        &self,
        follower_subject: &str,
        publisher_subject: &str,
        now_unix_seconds: i64,
    ) -> Result<bool, CommunityStoreError>;
    async fn toggle_package_follow(
        &self,
        follower_subject: &str,
        package_id: &str,
        now_unix_seconds: i64,
    ) -> Result<bool, CommunityStoreError>;
    async fn is_following_publisher(
        &self,
        follower_subject: &str,
        publisher_subject: &str,
    ) -> Result<bool, CommunityStoreError>;
    async fn publisher_follow_count(&self, publisher_subject: &str) -> Result<i64, CommunityStoreError>;
    async fn is_following_package(&self, follower_subject: &str, package_id: &str)
    -> Result<bool, CommunityStoreError>;
    async fn package_follow_count(&self, package_id: &str) -> Result<i64, CommunityStoreError>;
    async fn set_notification_preference(
        &self,
        subject: &str,
        preference: CommunityNotificationPreference,
        now_unix_seconds: i64,
    ) -> Result<(), CommunityStoreError>;
    async fn notification_preference(
        &self,
        subject: &str,
    ) -> Result<CommunityNotificationPreference, CommunityStoreError>;
    async fn create_notification(
        &self,
        notification: NewCommunityNotification,
    ) -> Result<CommunityNotification, CommunityStoreError>;
    async fn list_notifications(&self, subject: &str) -> Result<Vec<CommunityNotification>, CommunityStoreError>;
    async fn mark_notification_read(
        &self,
        notification_id: i64,
        recipient_subject: &str,
        now_unix_seconds: i64,
    ) -> Result<(), CommunityStoreError>;
    async fn mark_all_notifications_read(
        &self,
        recipient_subject: &str,
        now_unix_seconds: i64,
    ) -> Result<u64, CommunityStoreError>;
    async fn create_test_notification(
        &self,
        recipient_subject: &str,
        now_unix_seconds: i64,
    ) -> Result<CommunityNotification, CommunityStoreError>;
}

