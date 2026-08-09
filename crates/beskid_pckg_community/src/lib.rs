//! Storage-independent community rules for the Auth-Hub-backed pckg service.
//!
//! All ownership is keyed by [`Subject`], the stable Auth Hub `sub` claim.
//! Adapters are responsible for authentication, persistence, and transport; this
//! crate makes the authorization and state-transition rules explicit.

mod content;
mod errors;
mod follows_votes;
mod identity;
mod models;
mod notifications;
mod service;

pub use content::{Comment, CommentId, Post, PostId};
pub use errors::CommunityError;
pub use follows_votes::{FollowResult, VoteResult, VoteValue};
pub use identity::{ApiKeyScope, Permission, Principal, Role, Subject};
pub use models::{Board, BoardId, Profile, ResourceId};
pub use notifications::{Notification, NotificationId, NotificationPreference, NotificationScope};
pub use service::CommunityService;
