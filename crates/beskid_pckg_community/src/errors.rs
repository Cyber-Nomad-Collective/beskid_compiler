use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CommunityError {
    #[error("Auth Hub subject must not be blank")]
    InvalidSubject,
    #[error("board id must not be blank")]
    InvalidBoardId,
    #[error("the current principal is not permitted to perform this action")]
    Forbidden,
    #[error("board is locked")]
    BoardLocked,
    #[error("board was not found")]
    BoardNotFound,
    #[error("post was not found")]
    PostNotFound,
    #[error("comment was not found")]
    CommentNotFound,
    #[error("notification was not found")]
    NotificationNotFound,
    #[error("an author cannot vote on their own content")]
    SelfVote,
}
