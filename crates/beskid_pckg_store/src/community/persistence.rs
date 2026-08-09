use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use super::model::{
    AsyncCommunityRepository, CommunityBoard, CommunityComment, CommunityNotification, CommunityNotificationPreference,
    CommunityPost, CommunityProfile, CommunityStoreError, CommunityVote, NewCommunityNotification,
};
use super::rows::{
    CommunityBoardRow, CommunityCommentRow, CommunityNotificationRow, CommunityPostRow, CommunityPreferenceRow,
    CommunityProfileRow,
};
use super::validation::{
    community_database_error, community_timestamp, validate_community_subject, validate_nonblank,
    validate_notification_scope,
};
use super::voting::{VoteTarget, vote_for};

// Kept as a named contract because this insert must stay aligned with the
// independently migrated profile table. It creates no synthetic identity data.
pub(crate) const CREATE_TEST_NOTIFICATION_PROFILE_SQL: &str = "INSERT INTO pckg_community_profiles (subject,display_name,bio,social_links,is_publisher_verified,updated_at_utc) VALUES ($1,$1,'','[]'::JSONB,FALSE,$2) ON CONFLICT (subject) DO NOTHING";

/// PostgreSQL community repository. The mutation methods use transactions and
/// row locks for parent ownership, self-vote and score invariants.
#[derive(Clone, Debug)]
pub struct SqlxCommunityRepository {
    pool: PgPool,
}

impl SqlxCommunityRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
    pub async fn migrate(&self) -> Result<(), CommunityStoreError> {
        sqlx::raw_sql(crate::migrations::CREATE_COMMUNITY)
            .execute(self.pool())
            .await
            .map_err(community_database_error)?;
        sqlx::raw_sql(crate::migrations::EXTEND_COMMUNITY_NOTIFICATIONS)
            .execute(self.pool())
            .await
            .map_err(community_database_error)?;
        Ok(())
    }
}

#[async_trait]
impl AsyncCommunityRepository for SqlxCommunityRepository {
    async fn upsert_profile(&self, profile: CommunityProfile) -> Result<CommunityProfile, CommunityStoreError> {
        validate_community_subject(&profile.subject)?;
        validate_nonblank(&profile.display_name)?;
        let at = community_timestamp(profile.updated_at_unix_seconds)?;
        sqlx::query("INSERT INTO pckg_community_profiles (subject, display_name, bio, social_links, is_publisher_verified, updated_at_utc) VALUES ($1,$2,$3,$4::jsonb,$5,$6) ON CONFLICT (subject) DO UPDATE SET display_name=EXCLUDED.display_name,bio=EXCLUDED.bio,social_links=EXCLUDED.social_links,updated_at_utc=EXCLUDED.updated_at_utc")
            .bind(&profile.subject).bind(&profile.display_name).bind(&profile.bio).bind(&profile.social_links_json).bind(profile.is_publisher_verified).bind(at).execute(self.pool()).await.map_err(community_database_error)?;
        Ok(profile)
    }

    async fn profile(&self, subject: &str) -> Result<Option<CommunityProfile>, CommunityStoreError> {
        validate_community_subject(subject)?;
        let row = sqlx::query_as::<_, CommunityProfileRow>("SELECT subject,display_name,bio,social_links::text AS social_links_json,is_publisher_verified,updated_at_utc FROM pckg_community_profiles WHERE subject=$1").bind(subject).fetch_optional(self.pool()).await.map_err(community_database_error)?;
        Ok(row.map(CommunityProfileRow::into_domain))
    }
    async fn boards(&self) -> Result<Vec<CommunityBoard>, CommunityStoreError> {
        let rows = sqlx::query_as::<_, CommunityBoardRow>(
            "SELECT id,title,locked,created_at_utc,updated_at_utc FROM pckg_community_boards ORDER BY id",
        )
        .fetch_all(self.pool())
        .await
        .map_err(community_database_error)?;
        Ok(rows.into_iter().map(CommunityBoardRow::into_domain).collect())
    }
    async fn board(&self, board_id: &str) -> Result<Option<CommunityBoard>, CommunityStoreError> {
        validate_nonblank(board_id).map_err(|_| CommunityStoreError::InvalidBoardId)?;
        let row = sqlx::query_as::<_, CommunityBoardRow>(
            "SELECT id,title,locked,created_at_utc,updated_at_utc FROM pckg_community_boards WHERE id=$1",
        )
        .bind(board_id)
        .fetch_optional(self.pool())
        .await
        .map_err(community_database_error)?;
        Ok(row.map(CommunityBoardRow::into_domain))
    }
    async fn posts_for_board(&self, board_id: &str) -> Result<Vec<CommunityPost>, CommunityStoreError> {
        validate_nonblank(board_id).map_err(|_| CommunityStoreError::InvalidBoardId)?;
        let rows=sqlx::query_as::<_,CommunityPostRow>("SELECT id,board_id,author_subject,title,content,score,created_at_utc,updated_at_utc FROM pckg_community_posts WHERE board_id=$1 ORDER BY created_at_utc DESC").bind(board_id).fetch_all(self.pool()).await.map_err(community_database_error)?;
        Ok(rows.into_iter().map(CommunityPostRow::into_domain).collect())
    }
    async fn post(&self, post_id: i64) -> Result<Option<CommunityPost>, CommunityStoreError> {
        let row=sqlx::query_as::<_,CommunityPostRow>("SELECT id,board_id,author_subject,title,content,score,created_at_utc,updated_at_utc FROM pckg_community_posts WHERE id=$1").bind(post_id).fetch_optional(self.pool()).await.map_err(community_database_error)?;
        Ok(row.map(CommunityPostRow::into_domain))
    }
    async fn comments_for_post(&self, post_id: i64) -> Result<Vec<CommunityComment>, CommunityStoreError> {
        let rows=sqlx::query_as::<_,CommunityCommentRow>("SELECT id,post_id,author_subject,content,parent_comment_id,score,created_at_utc,updated_at_utc FROM pckg_community_comments WHERE post_id=$1 ORDER BY created_at_utc ASC").bind(post_id).fetch_all(self.pool()).await.map_err(community_database_error)?;
        Ok(rows.into_iter().map(CommunityCommentRow::into_domain).collect())
    }

    async fn create_board(&self, board: CommunityBoard) -> Result<CommunityBoard, CommunityStoreError> {
        validate_nonblank(&board.id).map_err(|_| CommunityStoreError::InvalidBoardId)?;
        validate_nonblank(&board.title)?;
        let created = community_timestamp(board.created_at_unix_seconds)?;
        let updated = community_timestamp(board.updated_at_unix_seconds)?;
        sqlx::query("INSERT INTO pckg_community_boards (id,title,locked,created_at_utc,updated_at_utc) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (id) DO UPDATE SET title=EXCLUDED.title,locked=EXCLUDED.locked,updated_at_utc=EXCLUDED.updated_at_utc")
            .bind(&board.id).bind(&board.title).bind(board.locked).bind(created).bind(updated).execute(self.pool()).await.map_err(community_database_error)?;
        Ok(board)
    }

    async fn create_post(
        &self,
        board_id: &str,
        author_subject: &str,
        title: &str,
        content: &str,
        now: i64,
    ) -> Result<CommunityPost, CommunityStoreError> {
        validate_nonblank(board_id).map_err(|_| CommunityStoreError::InvalidBoardId)?;
        validate_community_subject(author_subject)?;
        validate_nonblank(title)?;
        let at = community_timestamp(now)?;
        let row = sqlx::query_as::<_, CommunityPostRow>("INSERT INTO pckg_community_posts (board_id,author_subject,title,content,score,created_at_utc,updated_at_utc) SELECT id,$2,$3,$4,0,$5,$5 FROM pckg_community_boards WHERE id=$1 AND locked=FALSE RETURNING id,board_id,author_subject,title,content,score,created_at_utc,updated_at_utc")
            .bind(board_id).bind(author_subject).bind(title).bind(content).bind(at).fetch_optional(self.pool()).await.map_err(community_database_error)?;
        row.map(CommunityPostRow::into_domain).ok_or(CommunityStoreError::BoardNotFound)
    }

    async fn create_comment(
        &self,
        post_id: i64,
        author_subject: &str,
        content: &str,
        parent_comment_id: Option<i64>,
        now: i64,
    ) -> Result<CommunityComment, CommunityStoreError> {
        validate_community_subject(author_subject)?;
        validate_nonblank(content)?;
        let at = community_timestamp(now)?;
        let mut tx = self.pool().begin().await.map_err(community_database_error)?;
        let post_exists: Option<i64> =
            sqlx::query_scalar("SELECT id FROM pckg_community_posts WHERE id=$1 FOR KEY SHARE")
                .bind(post_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(community_database_error)?;
        if post_exists.is_none() {
            return Err(CommunityStoreError::PostNotFound);
        }
        if let Some(parent) = parent_comment_id {
            let parent_post: Option<i64> =
                sqlx::query_scalar("SELECT post_id FROM pckg_community_comments WHERE id=$1 FOR KEY SHARE")
                    .bind(parent)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(community_database_error)?;
            match parent_post {
                None => return Err(CommunityStoreError::CommentNotFound),
                Some(found) if found != post_id => {
                    return Err(CommunityStoreError::ParentCommentOutsidePost);
                }
                _ => {}
            }
        }
        let row=sqlx::query_as::<_,CommunityCommentRow>("INSERT INTO pckg_community_comments (post_id,author_subject,content,parent_comment_id,score,created_at_utc,updated_at_utc) VALUES ($1,$2,$3,$4,0,$5,$5) RETURNING id,post_id,author_subject,content,parent_comment_id,score,created_at_utc,updated_at_utc").bind(post_id).bind(author_subject).bind(content).bind(parent_comment_id).bind(at).fetch_one(&mut *tx).await.map_err(community_database_error)?;
        tx.commit().await.map_err(community_database_error)?;
        Ok(row.into_domain())
    }

    async fn vote_on_post(
        &self,
        post_id: i64,
        voter: &str,
        vote: CommunityVote,
        now: i64,
    ) -> Result<i32, CommunityStoreError> {
        vote_for(self.pool(), VoteTarget::Post, post_id, voter, vote, now).await
    }
    async fn vote_on_comment(
        &self,
        comment_id: i64,
        voter: &str,
        vote: CommunityVote,
        now: i64,
    ) -> Result<i32, CommunityStoreError> {
        vote_for(self.pool(), VoteTarget::Comment, comment_id, voter, vote, now).await
    }

    async fn toggle_publisher_follow(
        &self,
        follower: &str,
        publisher: &str,
        now: i64,
    ) -> Result<bool, CommunityStoreError> {
        validate_community_subject(follower)?;
        validate_community_subject(publisher)?;
        if follower == publisher {
            return Ok(true);
        }
        let at = community_timestamp(now)?;
        let removed = sqlx::query(
            "DELETE FROM pckg_community_publisher_follows WHERE follower_subject=$1 AND publisher_subject=$2",
        )
        .bind(follower)
        .bind(publisher)
        .execute(self.pool())
        .await
        .map_err(community_database_error)?;
        if removed.rows_affected() > 0 {
            return Ok(false);
        }
        sqlx::query("INSERT INTO pckg_community_publisher_follows (follower_subject,publisher_subject,created_at_utc) VALUES ($1,$2,$3)").bind(follower).bind(publisher).bind(at).execute(self.pool()).await.map_err(community_database_error)?;
        Ok(true)
    }

    async fn toggle_package_follow(
        &self,
        follower: &str,
        package_id: &str,
        now: i64,
    ) -> Result<bool, CommunityStoreError> {
        validate_community_subject(follower)?;
        let package = Uuid::parse_str(package_id).map_err(|_| CommunityStoreError::InvalidPackageId)?;
        let at = community_timestamp(now)?;
        let removed =
            sqlx::query("DELETE FROM pckg_community_package_follows WHERE follower_subject=$1 AND package_id=$2")
                .bind(follower)
                .bind(package)
                .execute(self.pool())
                .await
                .map_err(community_database_error)?;
        if removed.rows_affected() > 0 {
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO pckg_community_package_follows (follower_subject,package_id,created_at_utc) VALUES ($1,$2,$3)",
        )
        .bind(follower)
        .bind(package)
        .bind(at)
        .execute(self.pool())
        .await
        .map_err(community_database_error)?;
        Ok(true)
    }

    async fn is_following_publisher(&self, follower: &str, publisher: &str) -> Result<bool, CommunityStoreError> {
        validate_community_subject(follower)?;
        validate_community_subject(publisher)?;
        if follower == publisher {
            return Ok(true);
        }
        let value:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pckg_community_publisher_follows WHERE follower_subject=$1 AND publisher_subject=$2)").bind(follower).bind(publisher).fetch_one(self.pool()).await.map_err(community_database_error)?;
        Ok(value)
    }
    async fn publisher_follow_count(&self, publisher: &str) -> Result<i64, CommunityStoreError> {
        validate_community_subject(publisher)?;
        sqlx::query_scalar("SELECT COUNT(*) FROM pckg_community_publisher_follows WHERE publisher_subject=$1")
            .bind(publisher)
            .fetch_one(self.pool())
            .await
            .map_err(community_database_error)
    }
    async fn is_following_package(&self, follower: &str, package_id: &str) -> Result<bool, CommunityStoreError> {
        validate_community_subject(follower)?;
        let package = Uuid::parse_str(package_id).map_err(|_| CommunityStoreError::InvalidPackageId)?;
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pckg_community_package_follows WHERE follower_subject=$1 AND package_id=$2)",
        )
        .bind(follower)
        .bind(package)
        .fetch_one(self.pool())
        .await
        .map_err(community_database_error)
    }
    async fn package_follow_count(&self, package_id: &str) -> Result<i64, CommunityStoreError> {
        let package = Uuid::parse_str(package_id).map_err(|_| CommunityStoreError::InvalidPackageId)?;
        sqlx::query_scalar("SELECT COUNT(*) FROM pckg_community_package_follows WHERE package_id=$1")
            .bind(package)
            .fetch_one(self.pool())
            .await
            .map_err(community_database_error)
    }

    async fn set_notification_preference(
        &self,
        subject: &str,
        preference: CommunityNotificationPreference,
        now: i64,
    ) -> Result<(), CommunityStoreError> {
        validate_community_subject(subject)?;
        let at = community_timestamp(now)?;
        sqlx::query("INSERT INTO pckg_community_notification_preferences (subject,system_enabled,mention_enabled,reply_enabled,followed_publisher_post_enabled,moderation_enabled,updated_at_utc) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (subject) DO UPDATE SET system_enabled=EXCLUDED.system_enabled,mention_enabled=EXCLUDED.mention_enabled,reply_enabled=EXCLUDED.reply_enabled,followed_publisher_post_enabled=EXCLUDED.followed_publisher_post_enabled,moderation_enabled=EXCLUDED.moderation_enabled,updated_at_utc=EXCLUDED.updated_at_utc").bind(subject).bind(preference.system_enabled).bind(preference.mention_enabled).bind(preference.reply_enabled).bind(preference.followed_publisher_post_enabled).bind(preference.moderation_enabled).bind(at).execute(self.pool()).await.map_err(community_database_error)?;
        Ok(())
    }

    async fn notification_preference(
        &self,
        subject: &str,
    ) -> Result<CommunityNotificationPreference, CommunityStoreError> {
        validate_community_subject(subject)?;
        let row:Option<CommunityPreferenceRow>=sqlx::query_as("SELECT system_enabled,mention_enabled,reply_enabled,followed_publisher_post_enabled,moderation_enabled FROM pckg_community_notification_preferences WHERE subject=$1").bind(subject).fetch_optional(self.pool()).await.map_err(community_database_error)?;
        Ok(row.map(CommunityPreferenceRow::into_domain).unwrap_or_default())
    }

    async fn create_notification(
        &self,
        notification: NewCommunityNotification,
    ) -> Result<CommunityNotification, CommunityStoreError> {
        validate_community_subject(&notification.recipient_subject)?;
        validate_community_subject(&notification.actor_subject)?;
        validate_notification_scope(&notification.scope)?;
        let at = community_timestamp(notification.now_unix_seconds)?;
        let row = sqlx::query_as::<_, CommunityNotificationRow>(
            "INSERT INTO pckg_community_notifications (recipient_subject,scope,actor_subject,post_id,comment_id,created_at_utc,read_at_utc) \
             VALUES ($1,$2,$3,$4,$5,$6,NULL) \
             RETURNING id,recipient_subject,scope,actor_subject,post_id,comment_id,created_at_utc,read_at_utc",
        )
        .bind(&notification.recipient_subject)
        .bind(&notification.scope)
        .bind(&notification.actor_subject)
        .bind(notification.post_id)
        .bind(notification.comment_id)
        .bind(at)
        .fetch_one(self.pool())
        .await
        .map_err(community_database_error)?;
        Ok(row.into_domain())
    }

    async fn list_notifications(&self, subject: &str) -> Result<Vec<CommunityNotification>, CommunityStoreError> {
        validate_community_subject(subject)?;
        let rows=sqlx::query_as::<_,CommunityNotificationRow>("SELECT id,recipient_subject,scope,actor_subject,post_id,comment_id,created_at_utc,read_at_utc FROM pckg_community_notifications WHERE recipient_subject=$1 ORDER BY created_at_utc DESC").bind(subject).fetch_all(self.pool()).await.map_err(community_database_error)?;
        Ok(rows.into_iter().map(CommunityNotificationRow::into_domain).collect())
    }

    async fn mark_notification_read(
        &self,
        notification_id: i64,
        recipient: &str,
        now: i64,
    ) -> Result<(), CommunityStoreError> {
        validate_community_subject(recipient)?;
        let at = community_timestamp(now)?;
        let result=sqlx::query("UPDATE pckg_community_notifications SET read_at_utc=COALESCE(read_at_utc,$3) WHERE id=$1 AND recipient_subject=$2").bind(notification_id).bind(recipient).bind(at).execute(self.pool()).await.map_err(community_database_error)?;
        if result.rows_affected() == 0 { Err(CommunityStoreError::NotificationNotFound) } else { Ok(()) }
    }

    async fn mark_all_notifications_read(&self, recipient: &str, now: i64) -> Result<u64, CommunityStoreError> {
        validate_community_subject(recipient)?;
        let at = community_timestamp(now)?;
        sqlx::query(
            "UPDATE pckg_community_notifications SET read_at_utc=$2 WHERE recipient_subject=$1 AND read_at_utc IS NULL",
        )
        .bind(recipient)
        .bind(at)
        .execute(self.pool())
        .await
        .map(|result| result.rows_affected())
        .map_err(community_database_error)
    }

    async fn create_test_notification(
        &self,
        recipient: &str,
        now: i64,
    ) -> Result<CommunityNotification, CommunityStoreError> {
        validate_community_subject(recipient)?;
        let at = community_timestamp(now)?;
        // A delivery check must work before an account edits its profile. The
        // fallback stores only the stable subject, never a login or email.
        sqlx::query(CREATE_TEST_NOTIFICATION_PROFILE_SQL)
            .bind(recipient)
            .bind(at)
            .execute(self.pool())
            .await
            .map_err(community_database_error)?;
        self.create_notification(NewCommunityNotification {
            recipient_subject: recipient.to_owned(),
            scope: "system".to_owned(),
            actor_subject: recipient.to_owned(),
            post_id: None,
            comment_id: None,
            now_unix_seconds: now,
        })
        .await
    }
}
