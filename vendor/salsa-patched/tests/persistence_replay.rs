#![cfg(all(feature = "persistence", feature = "inventory"))]

mod common;

use common::LogDatabase;
use salsa::{Database, Setter};

#[salsa::input(persist)]
struct ReplayInput {
    field: usize,
}

#[salsa::tracked(persist)]
fn replay_child(db: &dyn LogDatabase, input: ReplayInput) -> usize {
    input.field(db) + 1
}

#[salsa::tracked(persist)]
fn replay_parent(db: &dyn LogDatabase, input: ReplayInput) -> usize {
    replay_child(db, input) * 2
}

#[test]
fn fresh_snapshot_revalidates_child_before_its_first_direct_call() {
    let mut db = common::LoggerDatabase::default();
    let input = ReplayInput::new(&db, 3);
    assert_eq!(replay_parent(&db, input), 8);
    let serialized = serde_json::to_string(&<dyn Database>::as_serialize(&mut db)).unwrap();
    let mut restored = common::LoggerDatabase::default();
    <dyn Database>::deserialize(
        &mut restored,
        &mut serde_json::Deserializer::from_str(&serialized),
    )
    .unwrap();
    // Force the parent to verify its persisted child without entering that
    // child's generated query accessor first. LogDatabase is a custom Db view.
    input.set_field(&mut restored).to(4);
    assert_eq!(replay_parent(&restored, input), 10);
}

#[salsa::db]
trait ParentDatabase: LogDatabase {}

#[salsa::db]
impl ParentDatabase for common::LoggerDatabase {}

#[salsa::tracked(persist)]
fn distinct_view_parent(db: &dyn ParentDatabase, input: ReplayInput) -> usize {
    replay_child(db, input) * 2
}

#[test]
#[should_panic(expected = "No downcaster registered for type")]
fn fresh_snapshot_does_not_infer_an_unregistered_dependency_view() {
    let mut db = common::LoggerDatabase::default();
    let input = ReplayInput::new(&db, 3);
    assert_eq!(distinct_view_parent(&db, input), 8);
    let serialized = serde_json::to_string(&<dyn Database>::as_serialize(&mut db)).unwrap();
    let mut restored = common::LoggerDatabase::default();
    <dyn Database>::deserialize(
        &mut restored,
        &mut serde_json::Deserializer::from_str(&serialized),
    )
    .unwrap();
    input.set_field(&mut restored).to(4);
    // ParentDatabase does not grant an implicit view cast to LogDatabase.
    // The caller must explicitly register every required typed view on reload.
    distinct_view_parent(&restored, input);
}

#[salsa::tracked(persist)]
fn replay_keyed(db: &dyn LogDatabase, input: ReplayInput, key: u32) -> usize {
    input.field(db) + key as usize
}

/// Interned and memo entry counts of a serialized database.
fn snapshot_counts(serialized: &str) -> (usize, usize) {
    let value: serde_json::Value = serde_json::from_str(serialized).unwrap();
    let mut interned = 0;
    let mut memos = 0;
    for entries in value["ingredients"].as_object().unwrap().values() {
        for (key, entry) in entries.as_object().unwrap() {
            if key.contains(':') {
                memos += 1;
            } else if entry.get("last_interned_at").is_some() {
                interned += 1;
            }
        }
    }
    (interned, memos)
}

fn restore(serialized: &str) -> common::LoggerDatabase {
    let mut restored = common::LoggerDatabase::default();
    <dyn Database>::deserialize(
        &mut restored,
        &mut serde_json::Deserializer::from_str(serialized),
    )
    .unwrap();
    restored
}

/// Every session interns a fresh set of keys outside of any query, like a compiler run that
/// numbers its syntax trees anew. The keys of earlier sessions are pinned only for the process
/// that interned them: restored, they are reused by the next session's keys, and their memos are
/// not written again, so the snapshot does not grow from one session to the next.
#[test]
fn repeated_sessions_with_fresh_top_level_keys_keep_the_snapshot_bounded() {
    const KEYS: u32 = 64;
    let mut db = common::LoggerDatabase::default();
    let input = ReplayInput::new(&db, 1);
    // Start past the first revision, as any real session does after setting its inputs.
    input.set_field(&mut db).to(0);
    for key in 0..KEYS {
        assert_eq!(replay_keyed(&db, input, key), key as usize);
    }
    let mut serialized = serde_json::to_string(&<dyn Database>::as_serialize(&mut db)).unwrap();
    assert_eq!(snapshot_counts(&serialized), (KEYS as usize, KEYS as usize));
    for session in 1..=4_u32 {
        let mut db = restore(&serialized);
        input.set_field(&mut db).to(session as usize);
        for key in 0..KEYS {
            let key = session * 1000 + key;
            assert_eq!(replay_keyed(&db, input, key), (session + key) as usize);
        }
        serialized = serde_json::to_string(&<dyn Database>::as_serialize(&mut db)).unwrap();
        let (interned, memos) = snapshot_counts(&serialized);
        assert_eq!(memos, KEYS as usize, "session {session} writes only its own memos");
        // Slots are reused per hash shard: a shard with fewer dead slots than new keys allocates
        // the difference. Without reuse every session would add `KEYS` slots.
        assert!(
            interned < 2 * KEYS as usize,
            "session {session} must reuse the earlier keys' slots: {interned}"
        );
    }
}

/// A restored session that keys its queries exactly like the previous one pins and reuses the
/// same values, and a session that does not advance the revision writes what it restored.
#[test]
fn restored_top_level_keys_are_pinned_again_when_interned_again() {
    let mut db = common::LoggerDatabase::default();
    let input = ReplayInput::new(&db, 0);
    input.set_field(&mut db).to(1);
    let first: Vec<_> = (0..8).map(|key| replay_keyed(&db, input, key)).collect();
    let serialized = serde_json::to_string(&<dyn Database>::as_serialize(&mut db)).unwrap();

    let mut unchanged = restore(&serialized);
    let idle = serde_json::to_string(&<dyn Database>::as_serialize(&mut unchanged)).unwrap();
    assert_eq!(
        snapshot_counts(&idle),
        (8, 8),
        "an idle session writes everything it restored"
    );

    let mut db = restore(&serialized);
    input.set_field(&mut db).to(2);
    let again: Vec<_> = (0..8).map(|key| replay_keyed(&db, input, key)).collect();
    assert_eq!(again, first.iter().map(|value| value + 1).collect::<Vec<_>>());
    // New keys must not take the slots of keys this session interned again.
    for key in 100..108 {
        replay_keyed(&db, input, key);
    }
    for key in 0..8 {
        assert_eq!(replay_keyed(&db, input, key), key as usize + 2);
    }
    let serialized = serde_json::to_string(&<dyn Database>::as_serialize(&mut db)).unwrap();
    assert_eq!(snapshot_counts(&serialized), (16, 16));
}
