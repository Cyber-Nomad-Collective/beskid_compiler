use beskid_pckg_community::{
    ApiKeyScope, Board, BoardId, CommunityError, CommunityService, NotificationPreference,
    Permission, Principal, Profile, ResourceId, Role, Subject, VoteValue,
};

fn subject(value: &str) -> Subject {
    Subject::new(value).unwrap()
}

fn user(value: &str) -> Principal {
    Principal::auth_hub(subject(value), [Role::User])
}

#[test]
fn blank_auth_hub_subject_is_rejected() {
    assert_eq!(Subject::new("  "), Err(CommunityError::InvalidSubject));
}

#[test]
fn publisher_verification_requires_a_super_admin() {
    let mut community = CommunityService::new();
    let publisher = subject("hub:publisher");
    community.upsert_profile(Profile::new(publisher.clone(), "Beskid Labs"));

    assert_eq!(
        community.verify_publisher(&user("hub:member"), &publisher),
        Err(CommunityError::Forbidden)
    );

    community
        .verify_publisher(
            &Principal::auth_hub(subject("hub:admin"), [Role::SuperAdmin]),
            &publisher,
        )
        .unwrap();
    assert!(community.profile(&publisher).unwrap().is_publisher_verified);
}

#[test]
fn api_keys_enforce_read_and_publish_scopes() {
    let reader = Principal::api_key(subject("hub:bot"), [ApiKeyScope::Read]);
    let publisher = Principal::api_key(subject("hub:bot"), [ApiKeyScope::Publish]);

    assert!(reader.allows(Permission::Read));
    assert!(!reader.allows(Permission::Publish));
    assert!(publisher.allows(Permission::Publish));
}

#[test]
fn publisher_self_follow_is_visible_without_creating_a_follow() {
    let mut community = CommunityService::new();
    let publisher = subject("hub:publisher");
    let result = community
        .toggle_publisher_follow(&user("hub:publisher"), &publisher)
        .unwrap();

    assert!(result.is_following);
    assert!(!result.changed);
    assert_eq!(community.publisher_follow_count(&publisher), 0);
}

#[test]
fn members_can_publish_to_unlocked_boards_and_followers_are_notified() {
    let mut community = CommunityService::new();
    let board = Board::new(BoardId::new("announcements").unwrap(), "Announcements");
    community.add_board(board);
    let author = user("hub:author");
    let follower = user("hub:follower");
    community
        .toggle_publisher_follow(&follower, author.subject().unwrap())
        .unwrap();

    let post = community
        .create_post(
            &author,
            &BoardId::new("announcements").unwrap(),
            "Hello",
            "World",
        )
        .unwrap();

    assert_eq!(post.author, subject("hub:author"));
    assert_eq!(
        community
            .notifications_for(follower.subject().unwrap())
            .len(),
        1
    );
}

#[test]
fn locked_board_requires_moderator_or_resource_permission_to_publish() {
    let mut community = CommunityService::new();
    let board_id = BoardId::new("moderated").unwrap();
    let mut board = Board::new(board_id.clone(), "Moderated");
    board.locked = true;
    community.add_board(board);
    let member = user("hub:member");

    assert_eq!(
        community.create_post(&member, &board_id, "Nope", "Nope"),
        Err(CommunityError::BoardLocked)
    );

    community.grant_permission(
        subject("hub:member"),
        ResourceId::board(board_id.clone()),
        Permission::Moderate,
    );
    assert!(
        community
            .create_post(&member, &board_id, "Allowed", "With moderation permission")
            .is_ok()
    );
}

#[test]
fn author_cannot_vote_on_own_post_but_other_member_can_change_vote() {
    let mut community = CommunityService::new();
    let board_id = BoardId::new("general").unwrap();
    community.add_board(Board::new(board_id.clone(), "General"));
    let author = user("hub:author");
    let voter = user("hub:voter");
    let post = community
        .create_post(&author, &board_id, "Title", "Body")
        .unwrap();

    assert_eq!(
        community.vote_on_post(&author, post.id, VoteValue::Up),
        Err(CommunityError::SelfVote)
    );
    assert_eq!(
        community
            .vote_on_post(&voter, post.id, VoteValue::Up)
            .unwrap()
            .score,
        1
    );
    assert_eq!(
        community
            .vote_on_post(&voter, post.id, VoteValue::Down)
            .unwrap()
            .score,
        -1
    );
}

#[test]
fn reply_notifies_post_author_but_never_notifies_the_actor() {
    let mut community = CommunityService::new();
    let board_id = BoardId::new("general").unwrap();
    community.add_board(Board::new(board_id.clone(), "General"));
    let author = user("hub:author");
    let commenter = user("hub:commenter");
    let post = community
        .create_post(&author, &board_id, "Title", "Body")
        .unwrap();

    community
        .create_comment(&commenter, post.id, "Question", None)
        .unwrap();

    assert_eq!(
        community.notifications_for(author.subject().unwrap()).len(),
        1
    );
    assert!(
        community
            .notifications_for(commenter.subject().unwrap())
            .is_empty()
    );
}

#[test]
fn notification_preferences_filter_delivery_by_scope() {
    let mut community = CommunityService::new();
    let member = subject("hub:member");
    community.set_notification_preference(member.clone(), NotificationPreference::mentions_only());

    assert!(community.should_notify(&member, beskid_pckg_community::NotificationScope::Mention));
    assert!(!community.should_notify(
        &member,
        beskid_pckg_community::NotificationScope::FollowedPublisherPost
    ));
}
