use serde::{Deserialize, Serialize};

use crate::{identity::Subject, models::BoardId};

pub type PostId = u64;
pub type CommentId = u64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Post {
    pub id: PostId,
    pub board_id: BoardId,
    pub author: Subject,
    pub title: String,
    pub content: String,
    pub score: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: CommentId,
    pub post_id: PostId,
    pub author: Subject,
    pub content: String,
    pub parent_comment_id: Option<CommentId>,
    pub score: i32,
}
