use std::collections::{BTreeMap, BTreeSet};

use crate::{
    content::{Comment, CommentId, Post, PostId},
    errors::CommunityError,
    follows_votes::{FollowResult, VoteResult, VoteValue},
    identity::{Permission, Principal, Subject},
    models::{Board, BoardId, Profile, ResourceId},
    notifications::{Notification, NotificationId, NotificationPreference, NotificationScope},
};

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
        Self { next_post_id: 1, next_comment_id: 1, next_notification_id: 1, ..Self::default() }
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
    pub fn set_board_locked(&mut self, board_id: &BoardId, locked: bool) -> Result<(), CommunityError> {
        let board = self.boards.get_mut(board_id).ok_or(CommunityError::BoardNotFound)?;
        board.locked = locked;
        Ok(())
    }
    pub fn posts_for_board(&self, board_id: &BoardId) -> Vec<&Post> {
        self.posts.values().filter(|post| &post.board_id == board_id).collect()
    }
    pub fn post(&self, post_id: PostId) -> Option<&Post> {
        self.posts.get(&post_id)
    }
    pub fn comments_for_post(&self, post_id: PostId) -> Vec<&Comment> {
        self.comments.values().filter(|comment| comment.post_id == post_id).collect()
    }
    pub fn comment(&self, comment_id: CommentId) -> Option<&Comment> {
        self.comments.get(&comment_id)
    }
    pub fn grant_permission(&mut self, subject: Subject, resource: ResourceId, permission: Permission) {
        self.permissions.insert((subject, resource, permission));
    }

    pub fn verify_publisher(&mut self, actor: &Principal, publisher: &Subject) -> Result<(), CommunityError> {
        Self::require(actor, Permission::VerifyPublisher)?;
        let profile = self.profiles.get_mut(publisher).ok_or(CommunityError::Forbidden)?;
        profile.is_publisher_verified = true;
        Ok(())
    }

    pub fn toggle_publisher_follow(
        &mut self,
        actor: &Principal,
        publisher: &Subject,
    ) -> Result<FollowResult, CommunityError> {
        Self::require(actor, Permission::Publish)?;
        let follower = actor.subject().expect("authorized principal has a subject").clone();
        if follower == *publisher {
            return Ok(FollowResult { is_following: true, changed: false });
        }
        let key = (follower, publisher.clone());
        if self.publisher_follows.remove(&key) {
            Ok(FollowResult { is_following: false, changed: true })
        } else {
            self.publisher_follows.insert(key);
            Ok(FollowResult { is_following: true, changed: true })
        }
    }

    pub fn publisher_follow_count(&self, publisher: &Subject) -> usize {
        self.publisher_follows.iter().filter(|(_, target)| target == publisher).count()
    }
    pub fn is_following_publisher(&self, follower: &Subject, publisher: &Subject) -> bool {
        follower == publisher || self.publisher_follows.contains(&(follower.clone(), publisher.clone()))
    }
    pub fn toggle_package_follow(
        &mut self,
        actor: &Principal,
        package: impl Into<String>,
    ) -> Result<FollowResult, CommunityError> {
        Self::require(actor, Permission::Publish)?;
        let key = (actor.subject().expect("authorized principal has a subject").clone(), package.into());
        if self.package_follows.remove(&key) {
            Ok(FollowResult { is_following: false, changed: true })
        } else {
            self.package_follows.insert(key);
            Ok(FollowResult { is_following: true, changed: true })
        }
    }
    pub fn is_following_package(&self, follower: &Subject, package: &str) -> bool {
        self.package_follows.contains(&(follower.clone(), package.to_owned()))
    }
    pub fn package_follow_count(&self, package: &str) -> usize {
        self.package_follows.iter().filter(|(_, followed_package)| followed_package == package).count()
    }

    pub fn create_post(
        &mut self,
        actor: &Principal,
        board_id: &BoardId,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<Post, CommunityError> {
        Self::require(actor, Permission::Publish)?;
        let board = self.boards.get(board_id).ok_or(CommunityError::BoardNotFound)?;
        if board.locked && !self.can_moderate(actor, &ResourceId::board(board_id.clone())) {
            return Err(CommunityError::BoardLocked);
        }
        let post = Post {
            id: self.take_post_id(),
            board_id: board_id.clone(),
            author: actor.subject().expect("authorized principal has a subject").clone(),
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
        let post = self.posts.get(&post_id).cloned().ok_or(CommunityError::PostNotFound)?;
        if let Some(parent) = parent_comment_id
            && !self.comments.contains_key(&parent)
        {
            return Err(CommunityError::CommentNotFound);
        }
        let comment = Comment {
            id: self.take_comment_id(),
            post_id,
            author: actor.subject().expect("authorized principal has a subject").clone(),
            content: content.into(),
            parent_comment_id,
            score: 0,
        };
        self.notify(&post.author, NotificationScope::Reply, &comment.author, Some(post_id), Some(comment.id));
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
        let post = self.posts.get_mut(&post_id).ok_or(CommunityError::PostNotFound)?;
        let voter = actor.subject().expect("authorized principal has a subject").clone();
        if post.author == voter {
            return Err(CommunityError::SelfVote);
        }
        Self::apply_vote(&mut self.post_votes, (post_id, voter), vote, &mut post.score)
    }

    pub fn vote_on_comment(
        &mut self,
        actor: &Principal,
        comment_id: CommentId,
        vote: VoteValue,
    ) -> Result<VoteResult, CommunityError> {
        Self::require(actor, Permission::Publish)?;
        let comment = self.comments.get_mut(&comment_id).ok_or(CommunityError::CommentNotFound)?;
        let voter = actor.subject().expect("authorized principal has a subject").clone();
        if comment.author == voter {
            return Err(CommunityError::SelfVote);
        }
        Self::apply_vote(&mut self.comment_votes, (comment_id, voter), vote, &mut comment.score)
    }

    pub fn set_notification_preference(&mut self, subject: Subject, preference: NotificationPreference) {
        self.preferences.insert(subject, preference);
    }
    pub fn notification_preference(&self, subject: &Subject) -> NotificationPreference {
        self.preferences.get(subject).cloned().unwrap_or_default()
    }
    pub fn should_notify(&self, subject: &Subject, scope: NotificationScope) -> bool {
        self.preferences.get(subject).cloned().unwrap_or_default().allows(scope)
    }
    pub fn notifications_for(&self, subject: &Subject) -> Vec<&Notification> {
        self.notifications.iter().filter(|notification| &notification.recipient == subject).collect()
    }
    pub fn mark_notification_read(
        &mut self,
        actor: &Principal,
        notification_id: NotificationId,
    ) -> Result<(), CommunityError> {
        let actor_subject = actor.subject().ok_or(CommunityError::NotificationNotFound)?;
        let notification = self
            .notifications
            .iter_mut()
            .find(|notification| notification.id == notification_id && &notification.recipient == actor_subject)
            .ok_or(CommunityError::NotificationNotFound)?;
        notification.is_read = true;
        Ok(())
    }

    /// Marks only the authenticated recipient's unread notifications.
    pub fn mark_all_notifications_read(&mut self, actor: &Principal) -> Result<usize, CommunityError> {
        let subject = actor.subject().ok_or(CommunityError::NotificationNotFound)?;
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
    pub fn create_test_notification(&mut self, actor: &Principal) -> Result<NotificationId, CommunityError> {
        let subject = actor.subject().ok_or(CommunityError::NotificationNotFound)?.clone();
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
        if actor.allows(permission) { Ok(()) } else { Err(CommunityError::Forbidden) }
    }
    fn can_moderate(&self, actor: &Principal, resource: &ResourceId) -> bool {
        actor.allows(Permission::Moderate)
            || actor.subject().is_some_and(|subject| {
                self.permissions.contains(&(subject.clone(), resource.clone(), Permission::Moderate))
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
            self.notify(&follower, NotificationScope::FollowedPublisherPost, &post.author, Some(post.id), None);
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
