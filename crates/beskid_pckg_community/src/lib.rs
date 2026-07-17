//! Storage-independent community rules for the Auth-Hub-backed pckg service.
//!
//! All ownership is keyed by [`Subject`], the stable Auth Hub `sub` claim.
//! Adapters are responsible for authentication, persistence, and transport; this
//! crate makes the authorization and state-transition rules explicit.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Subject(String);

impl Subject {
    pub fn new(value: impl Into<String>) -> Result<Self, CommunityError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CommunityError::InvalidSubject);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Role {
    User,
    Moderator,
    SuperAdmin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ApiKeyScope {
    Read,
    Publish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Permission {
    Read,
    Publish,
    Moderate,
    VerifyPublisher,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    Anonymous,
    AuthHub {
        subject: Subject,
        roles: BTreeSet<Role>,
    },
    ApiKey {
        subject: Subject,
        scopes: BTreeSet<ApiKeyScope>,
    },
}

impl Principal {
    pub fn auth_hub(subject: Subject, roles: impl IntoIterator<Item = Role>) -> Self {
        Self::AuthHub {
            subject,
            roles: roles.into_iter().collect(),
        }
    }

    pub fn api_key(subject: Subject, scopes: impl IntoIterator<Item = ApiKeyScope>) -> Self {
        Self::ApiKey {
            subject,
            scopes: scopes.into_iter().collect(),
        }
    }

    pub fn subject(&self) -> Option<&Subject> {
        match self {
            Self::Anonymous => None,
            Self::AuthHub { subject, .. } | Self::ApiKey { subject, .. } => Some(subject),
        }
    }

    pub fn allows(&self, permission: Permission) -> bool {
        match self {
            Self::Anonymous => false,
            Self::AuthHub { roles, .. } => match permission {
                Permission::Read => true,
                Permission::Publish => {
                    roles.contains(&Role::User)
                        || roles.contains(&Role::Moderator)
                        || roles.contains(&Role::SuperAdmin)
                }
                Permission::Moderate => {
                    roles.contains(&Role::Moderator) || roles.contains(&Role::SuperAdmin)
                }
                Permission::VerifyPublisher => roles.contains(&Role::SuperAdmin),
            },
            Self::ApiKey { scopes, .. } => match permission {
                Permission::Read => {
                    scopes.contains(&ApiKeyScope::Read) || scopes.contains(&ApiKeyScope::Publish)
                }
                Permission::Publish => scopes.contains(&ApiKeyScope::Publish),
                Permission::Moderate | Permission::VerifyPublisher => false,
            },
        }
    }
}

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
        Self {
            id,
            title: title.into(),
            locked: false,
        }
    }
}

pub type PostId = u64;
pub type CommentId = u64;
pub type NotificationId = u64;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoteValue {
    Up,
    Down,
    Clear,
}

impl VoteValue {
    fn score(self) -> i8 {
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
        Self {
            enabled: BTreeSet::from([NotificationScope::Mention]),
        }
    }
    pub fn from_enabled(enabled: impl IntoIterator<Item = NotificationScope>) -> Self {
        Self {
            enabled: enabled.into_iter().collect(),
        }
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

#[derive(Debug, Default)]
pub struct CommunityService {
    profiles: BTreeMap<Subject, Profile>,
    boards: BTreeMap<BoardId, Board>,
    permissions: BTreeSet<(Subject, ResourceId, Permission)>,
    publisher_follows: BTreeSet<(Subject, Subject)>,
    package_follows: BTreeSet<(Subject, String)>,
    posts: BTreeMap<PostId, Post>,
    comments: BTreeMap<CommentId, Comment>,
    post_votes: BTreeMap<(PostId, Subject), i8>,
    comment_votes: BTreeMap<(CommentId, Subject), i8>,
    preferences: BTreeMap<Subject, NotificationPreference>,
    notifications: Vec<Notification>,
    next_post_id: PostId,
    next_comment_id: CommentId,
    next_notification_id: NotificationId,
}

impl CommunityService {
    pub fn new() -> Self {
        Self {
            next_post_id: 1,
            next_comment_id: 1,
            next_notification_id: 1,
            ..Self::default()
        }
    }
    pub fn upsert_profile(&mut self, profile: Profile) {
        self.profiles.insert(profile.subject.clone(), profile);
    }
    pub fn profile(&self, subject: &Subject) -> Option<&Profile> {
        self.profiles.get(subject)
    }
    pub fn add_board(&mut self, board: Board) {
        self.boards.insert(board.id.clone(), board);
    }
    pub fn boards(&self) -> Vec<&Board> {
        self.boards.values().collect()
    }
    pub fn board(&self, board_id: &BoardId) -> Option<&Board> {
        self.boards.get(board_id)
    }
    pub fn set_board_locked(
        &mut self,
        board_id: &BoardId,
        locked: bool,
    ) -> Result<(), CommunityError> {
        let board = self
            .boards
            .get_mut(board_id)
            .ok_or(CommunityError::BoardNotFound)?;
        board.locked = locked;
        Ok(())
    }
    pub fn posts_for_board(&self, board_id: &BoardId) -> Vec<&Post> {
        self.posts
            .values()
            .filter(|post| &post.board_id == board_id)
            .collect()
    }
    pub fn post(&self, post_id: PostId) -> Option<&Post> {
        self.posts.get(&post_id)
    }
    pub fn comments_for_post(&self, post_id: PostId) -> Vec<&Comment> {
        self.comments
            .values()
            .filter(|comment| comment.post_id == post_id)
            .collect()
    }
    pub fn comment(&self, comment_id: CommentId) -> Option<&Comment> {
        self.comments.get(&comment_id)
    }
    pub fn grant_permission(
        &mut self,
        subject: Subject,
        resource: ResourceId,
        permission: Permission,
    ) {
        self.permissions.insert((subject, resource, permission));
    }

    pub fn verify_publisher(
        &mut self,
        actor: &Principal,
        publisher: &Subject,
    ) -> Result<(), CommunityError> {
        Self::require(actor, Permission::VerifyPublisher)?;
        let profile = self
            .profiles
            .get_mut(publisher)
            .ok_or(CommunityError::Forbidden)?;
        profile.is_publisher_verified = true;
        Ok(())
    }

    pub fn toggle_publisher_follow(
        &mut self,
        actor: &Principal,
        publisher: &Subject,
    ) -> Result<FollowResult, CommunityError> {
        Self::require(actor, Permission::Publish)?;
        let follower = actor
            .subject()
            .expect("authorized principal has a subject")
            .clone();
        if follower == *publisher {
            return Ok(FollowResult {
                is_following: true,
                changed: false,
            });
        }
        let key = (follower, publisher.clone());
        if self.publisher_follows.remove(&key) {
            Ok(FollowResult {
                is_following: false,
                changed: true,
            })
        } else {
            self.publisher_follows.insert(key);
            Ok(FollowResult {
                is_following: true,
                changed: true,
            })
        }
    }

    pub fn publisher_follow_count(&self, publisher: &Subject) -> usize {
        self.publisher_follows
            .iter()
            .filter(|(_, target)| target == publisher)
            .count()
    }
    pub fn is_following_publisher(&self, follower: &Subject, publisher: &Subject) -> bool {
        follower == publisher
            || self
                .publisher_follows
                .contains(&(follower.clone(), publisher.clone()))
    }
    pub fn toggle_package_follow(
        &mut self,
        actor: &Principal,
        package: impl Into<String>,
    ) -> Result<FollowResult, CommunityError> {
        Self::require(actor, Permission::Publish)?;
        let key = (
            actor
                .subject()
                .expect("authorized principal has a subject")
                .clone(),
            package.into(),
        );
        if self.package_follows.remove(&key) {
            Ok(FollowResult {
                is_following: false,
                changed: true,
            })
        } else {
            self.package_follows.insert(key);
            Ok(FollowResult {
                is_following: true,
                changed: true,
            })
        }
    }
    pub fn is_following_package(&self, follower: &Subject, package: &str) -> bool {
        self.package_follows
            .contains(&(follower.clone(), package.to_owned()))
    }
    pub fn package_follow_count(&self, package: &str) -> usize {
        self.package_follows
            .iter()
            .filter(|(_, followed_package)| followed_package == package)
            .count()
    }

    pub fn create_post(
        &mut self,
        actor: &Principal,
        board_id: &BoardId,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<Post, CommunityError> {
        Self::require(actor, Permission::Publish)?;
        let board = self
            .boards
            .get(board_id)
            .ok_or(CommunityError::BoardNotFound)?;
        if board.locked && !self.can_moderate(actor, &ResourceId::board(board_id.clone())) {
            return Err(CommunityError::BoardLocked);
        }
        let post = Post {
            id: self.take_post_id(),
            board_id: board_id.clone(),
            author: actor
                .subject()
                .expect("authorized principal has a subject")
                .clone(),
            title: title.into(),
            content: content.into(),
            score: 0,
        };
        self.notify_publisher_followers(&post);
        self.posts.insert(post.id, post.clone());
        Ok(post)
    }

    pub fn create_comment(
        &mut self,
        actor: &Principal,
        post_id: PostId,
        content: impl Into<String>,
        parent_comment_id: Option<CommentId>,
    ) -> Result<Comment, CommunityError> {
        Self::require(actor, Permission::Publish)?;
        let post = self
            .posts
            .get(&post_id)
            .cloned()
            .ok_or(CommunityError::PostNotFound)?;
        if let Some(parent) = parent_comment_id
            && !self.comments.contains_key(&parent)
        {
            return Err(CommunityError::CommentNotFound);
        }
        let comment = Comment {
            id: self.take_comment_id(),
            post_id,
            author: actor
                .subject()
                .expect("authorized principal has a subject")
                .clone(),
            content: content.into(),
            parent_comment_id,
            score: 0,
        };
        self.notify(
            &post.author,
            NotificationScope::Reply,
            &comment.author,
            Some(post_id),
            Some(comment.id),
        );
        self.comments.insert(comment.id, comment.clone());
        Ok(comment)
    }

    pub fn vote_on_post(
        &mut self,
        actor: &Principal,
        post_id: PostId,
        vote: VoteValue,
    ) -> Result<VoteResult, CommunityError> {
        Self::require(actor, Permission::Publish)?;
        let post = self
            .posts
            .get_mut(&post_id)
            .ok_or(CommunityError::PostNotFound)?;
        let voter = actor
            .subject()
            .expect("authorized principal has a subject")
            .clone();
        if post.author == voter {
            return Err(CommunityError::SelfVote);
        }
        Self::apply_vote(
            &mut self.post_votes,
            (post_id, voter),
            vote,
            &mut post.score,
        )
    }

    pub fn vote_on_comment(
        &mut self,
        actor: &Principal,
        comment_id: CommentId,
        vote: VoteValue,
    ) -> Result<VoteResult, CommunityError> {
        Self::require(actor, Permission::Publish)?;
        let comment = self
            .comments
            .get_mut(&comment_id)
            .ok_or(CommunityError::CommentNotFound)?;
        let voter = actor
            .subject()
            .expect("authorized principal has a subject")
            .clone();
        if comment.author == voter {
            return Err(CommunityError::SelfVote);
        }
        Self::apply_vote(
            &mut self.comment_votes,
            (comment_id, voter),
            vote,
            &mut comment.score,
        )
    }

    pub fn set_notification_preference(
        &mut self,
        subject: Subject,
        preference: NotificationPreference,
    ) {
        self.preferences.insert(subject, preference);
    }
    pub fn notification_preference(&self, subject: &Subject) -> NotificationPreference {
        self.preferences.get(subject).cloned().unwrap_or_default()
    }
    pub fn should_notify(&self, subject: &Subject, scope: NotificationScope) -> bool {
        self.preferences
            .get(subject)
            .cloned()
            .unwrap_or_default()
            .allows(scope)
    }
    pub fn notifications_for(&self, subject: &Subject) -> Vec<&Notification> {
        self.notifications
            .iter()
            .filter(|notification| &notification.recipient == subject)
            .collect()
    }
    pub fn mark_notification_read(
        &mut self,
        actor: &Principal,
        notification_id: NotificationId,
    ) -> Result<(), CommunityError> {
        let actor_subject = actor
            .subject()
            .ok_or(CommunityError::NotificationNotFound)?;
        let notification = self
            .notifications
            .iter_mut()
            .find(|notification| {
                notification.id == notification_id && &notification.recipient == actor_subject
            })
            .ok_or(CommunityError::NotificationNotFound)?;
        notification.is_read = true;
        Ok(())
    }

    /// Marks only the authenticated recipient's unread notifications.
    pub fn mark_all_notifications_read(
        &mut self,
        actor: &Principal,
    ) -> Result<usize, CommunityError> {
        let subject = actor
            .subject()
            .ok_or(CommunityError::NotificationNotFound)?;
        let mut changed = 0;
        for notification in self
            .notifications
            .iter_mut()
            .filter(|notification| &notification.recipient == subject && !notification.is_read)
        {
            notification.is_read = true;
            changed += 1;
        }
        Ok(changed)
    }

    /// Creates the one permitted self-addressed notification: a delivery
    /// check for the current Auth Hub subject.
    pub fn create_test_notification(
        &mut self,
        actor: &Principal,
    ) -> Result<NotificationId, CommunityError> {
        let subject = actor
            .subject()
            .ok_or(CommunityError::NotificationNotFound)?
            .clone();
        let id = self.take_notification_id();
        self.notifications.push(Notification {
            id,
            recipient: subject.clone(),
            scope: NotificationScope::System,
            actor: subject,
            post_id: None,
            comment_id: None,
            is_read: false,
        });
        Ok(id)
    }

    fn require(actor: &Principal, permission: Permission) -> Result<(), CommunityError> {
        if actor.allows(permission) {
            Ok(())
        } else {
            Err(CommunityError::Forbidden)
        }
    }
    fn can_moderate(&self, actor: &Principal, resource: &ResourceId) -> bool {
        actor.allows(Permission::Moderate)
            || actor.subject().is_some_and(|subject| {
                self.permissions.contains(&(
                    subject.clone(),
                    resource.clone(),
                    Permission::Moderate,
                ))
            })
    }
    fn take_post_id(&mut self) -> PostId {
        let id = self.next_post_id;
        self.next_post_id += 1;
        id
    }
    fn take_comment_id(&mut self) -> CommentId {
        let id = self.next_comment_id;
        self.next_comment_id += 1;
        id
    }
    fn take_notification_id(&mut self) -> NotificationId {
        let id = self.next_notification_id;
        self.next_notification_id += 1;
        id
    }
    fn apply_vote(
        votes: &mut BTreeMap<(u64, Subject), i8>,
        key: (u64, Subject),
        vote: VoteValue,
        score: &mut i32,
    ) -> Result<VoteResult, CommunityError> {
        let old = votes.remove(&key).unwrap_or(0);
        let new = vote.score();
        if new != 0 {
            votes.insert(key, new);
        }
        *score += i32::from(new - old);
        Ok(VoteResult { score: *score })
    }
    fn notify_publisher_followers(&mut self, post: &Post) {
        let followers: Vec<_> = self
            .publisher_follows
            .iter()
            .filter(|(_, publisher)| publisher == &post.author)
            .map(|(follower, _)| follower.clone())
            .collect();
        for follower in followers {
            self.notify(
                &follower,
                NotificationScope::FollowedPublisherPost,
                &post.author,
                Some(post.id),
                None,
            );
        }
    }
    fn notify(
        &mut self,
        recipient: &Subject,
        scope: NotificationScope,
        actor: &Subject,
        post_id: Option<PostId>,
        comment_id: Option<CommentId>,
    ) {
        if recipient != actor && self.should_notify(recipient, scope) {
            let id = self.take_notification_id();
            self.notifications.push(Notification {
                id,
                recipient: recipient.clone(),
                scope,
                actor: actor.clone(),
                post_id,
                comment_id,
                is_read: false,
            });
        }
    }
}
