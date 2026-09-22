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
