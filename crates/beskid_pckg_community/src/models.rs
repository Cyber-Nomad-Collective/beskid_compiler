use serde::{Deserialize, Serialize};

use crate::{errors::CommunityError, identity::Subject};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BoardId(String);

impl BoardId {
    pub fn new(value: impl Into<String>) -> Result<Self, CommunityError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CommunityError::InvalidBoardId);
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ResourceId {
    Board(BoardId),
    Package(String),
}

impl ResourceId {
    pub fn board(board: BoardId) -> Self {
        Self::Board(board)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub subject: Subject,
    pub display_name: String,
    pub bio: String,
    pub social_links: Vec<String>,
    pub is_publisher_verified: bool,
}

impl Profile {
    pub fn new(subject: Subject, display_name: impl Into<String>) -> Self {
        Self {
            subject,
            display_name: display_name.into(),
            bio: String::new(),
            social_links: Vec::new(),
            is_publisher_verified: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    pub id: BoardId,
    pub title: String,
    pub locked: bool,
}

impl Board {
    pub fn new(id: BoardId, title: impl Into<String>) -> Self {
        Self { id, title: title.into(), locked: false }
    }
}
