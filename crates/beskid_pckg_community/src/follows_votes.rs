use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoteValue {
    Up,
    Down,
    Clear,
}

impl VoteValue {
    pub(super) fn score(self) -> i8 {
        match self {
            Self::Up => 1,
            Self::Down => -1,
            Self::Clear => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoteResult {
    pub score: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowResult {
    pub is_following: bool,
    pub changed: bool,
}
