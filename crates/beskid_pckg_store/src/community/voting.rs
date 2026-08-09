use sqlx::PgPool;

use super::model::{CommunityStoreError, CommunityVote};
use super::validation::{community_database_error, community_timestamp};

#[derive(Clone, Copy)]
pub(super) enum VoteTarget {
    Post,
    Comment,
}

impl VoteTarget {
    const fn content_table(self) -> &'static str {
        match self {
            Self::Post => "pckg_community_posts",
            Self::Comment => "pckg_community_comments",
        }
    }

    const fn vote_table(self) -> &'static str {
        match self {
            Self::Post => "pckg_community_post_votes",
            Self::Comment => "pckg_community_comment_votes",
        }
    }

    const fn key_column(self) -> &'static str {
        match self {
            Self::Post => "post_id",
            Self::Comment => "comment_id",
        }
    }

    const fn missing_error(self) -> CommunityStoreError {
        match self {
            Self::Post => CommunityStoreError::PostNotFound,
            Self::Comment => CommunityStoreError::CommentNotFound,
        }
    }
}

pub(super) async fn vote_for(
    pool: &PgPool,
    target: VoteTarget,
    content_id: i64,
    voter: &str,
    vote: CommunityVote,
    now: i64,
) -> Result<i32, CommunityStoreError> {
    let content_table = target.content_table();
    let vote_table = target.vote_table();
    let key_column = target.key_column();
    validate_community_subject(voter)?;
    let at = community_timestamp(now)?;
    let mut tx = pool.begin().await.map_err(community_database_error)?;
    // Identifiers are private constants supplied by this module, never HTTP input.
    let author_sql = format!("SELECT author_subject FROM {content_table} WHERE id=$1 FOR UPDATE");
    let author: Option<String> = sqlx::query_scalar(&author_sql)
        .bind(content_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(community_database_error)?;
    let author = author.ok_or(target.missing_error())?;
    if author == voter {
        return Err(CommunityStoreError::SelfVote);
    }
    let existing_sql = format!("SELECT value FROM {vote_table} WHERE {key_column}=$1 AND voter_subject=$2 FOR UPDATE");
    let old: Option<i16> = sqlx::query_scalar(&existing_sql)
        .bind(content_id)
        .bind(voter)
        .fetch_optional(&mut *tx)
        .await
        .map_err(community_database_error)?;
    let new = vote.value();
    if new == 0 {
        let delete_sql = format!("DELETE FROM {vote_table} WHERE {key_column}=$1 AND voter_subject=$2");
        sqlx::query(&delete_sql)
            .bind(content_id)
            .bind(voter)
            .execute(&mut *tx)
            .await
            .map_err(community_database_error)?;
    } else {
        let upsert_sql = format!(
            "INSERT INTO {vote_table} ({key_column},voter_subject,value,updated_at_utc) VALUES ($1,$2,$3,$4) ON CONFLICT ({key_column},voter_subject) DO UPDATE SET value=EXCLUDED.value,updated_at_utc=EXCLUDED.updated_at_utc"
        );
        sqlx::query(&upsert_sql)
            .bind(content_id)
            .bind(voter)
            .bind(new)
            .bind(at)
            .execute(&mut *tx)
            .await
            .map_err(community_database_error)?;
    }
    let delta = i32::from(new) - i32::from(old.unwrap_or(0));
    let update_sql = format!("UPDATE {content_table} SET score=score+$2,updated_at_utc=$3 WHERE id=$1 RETURNING score");
    let score: i32 = sqlx::query_scalar(&update_sql)
        .bind(content_id)
        .bind(delta)
        .bind(at)
        .fetch_one(&mut *tx)
        .await
        .map_err(community_database_error)?;
    tx.commit().await.map_err(community_database_error)?;
    Ok(score)
}

