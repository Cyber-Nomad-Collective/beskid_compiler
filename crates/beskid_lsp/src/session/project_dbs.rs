//! One in-memory Salsa database per project, kept across requests.
//!
//! A workspace often holds many projects (Corelib holds more than fifty). Each project is
//! prepared in its own database, as the CLI does, but the server keeps the database of each
//! project it works on: switching between two open documents of different projects must not
//! throw away the typed closure of either. Work on one project never waits for work on
//! another, because each database has its own mutation gate.
//!
//! The databases live only in memory. A restored Salsa snapshot is verified and recomputed in
//! full on every prepare (see `configure_db_in_place` in `beskid_queries::session`), so loading
//! and saving one only adds time.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use beskid_queries::{BeskidDatabase, reset_compilation_database};
use tokio::sync::Mutex as AsyncMutex;

/// Most project databases kept at once. Each holds the typed closure of its project, Corelib
/// included (about 0.5-1 GB for a Corelib package), so the pool stays small. Databases of
/// projects that only the background scan touched are dropped first; among the others the least
/// recently edited one goes.
pub(crate) const MAX_PROJECT_DATABASES: usize = 4;

/// Analyses a database serves before it starts afresh. Every analysis registers a new syntax
/// generation whose typed facts stay in the storage (measured at roughly 50-100 MB for a Corelib
/// package), so a database kept for a whole session would grow without bound.
pub(crate) const ANALYSES_PER_DATABASE: usize = 8;

/// The database of one project and the gate that serializes its mutation.
#[derive(Clone)]
pub(crate) struct ProjectDatabase {
    db: Arc<Mutex<BeskidDatabase>>,
    gate: Arc<AsyncMutex<()>>,
    analyses: Arc<AtomicUsize>,
}

impl ProjectDatabase {
    fn new() -> Self {
        Self {
            db: Arc::new(Mutex::new(BeskidDatabase::default())),
            gate: Arc::new(AsyncMutex::new(())),
            analyses: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Lock the database for one analysis (hold [`Self::gate`] first). After
    /// [`ANALYSES_PER_DATABASE`] analyses the storage starts afresh; query handles cloned from
    /// the old storage keep it alive until they are dropped.
    pub(crate) fn lock_for_analysis(&self) -> MutexGuard<'_, BeskidDatabase> {
        let mut guard = self.lock();
        if self.analyses.fetch_add(1, Ordering::AcqRel) >= ANALYSES_PER_DATABASE {
            reset_compilation_database(&mut guard);
            self.analyses.store(1, Ordering::Release);
        }
        guard
    }

    /// The mutation gate. Hold it while the database is locked or a query handle is cloned.
    pub(crate) fn gate(&self) -> &Arc<AsyncMutex<()>> {
        &self.gate
    }

    /// Lock the database. A panic in an earlier holder leaves memos of unknown state behind,
    /// so a poisoned database is replaced by a fresh one.
    pub(crate) fn lock(&self) -> MutexGuard<'_, BeskidDatabase> {
        match self.db.lock() {
            Ok(guard) => guard,
            Err(poison) => {
                tracing::error!("project database lock poisoned; starting a fresh database");
                let mut guard = poison.into_inner();
                reset_compilation_database(&mut guard);
                self.db.clear_poison();
                guard
            }
        }
    }
}

struct Entry {
    database: ProjectDatabase,
    /// Clock value of the last use by any work.
    last_used: u64,
    /// Clock value of the last use for an open document (0: background work only).
    last_interactive: u64,
}

/// The databases of the projects the server works on, keyed by canonical project root.
/// Work outside any project (manifests, standalone BSOL, explorer graphs) shares one
/// detached database.
pub(crate) struct ProjectDatabases {
    detached: ProjectDatabase,
    projects: HashMap<PathBuf, Entry>,
    clock: u64,
    capacity: usize,
}

impl Default for ProjectDatabases {
    fn default() -> Self {
        Self::with_capacity(MAX_PROJECT_DATABASES)
    }
}

impl ProjectDatabases {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self { detached: ProjectDatabase::new(), projects: HashMap::new(), clock: 0, capacity: capacity.max(1) }
    }

    /// The database of `project_root` (canonicalized here), created on first use. `interactive`
    /// marks use for an open document; such databases outlive those of scanned-only projects.
    /// Evicted databases are returned so the caller can drop them outside its locks (dropping
    /// a large storage takes a while).
    pub(crate) fn get(
        &mut self,
        project_root: Option<&Path>,
        interactive: bool,
    ) -> (ProjectDatabase, Vec<ProjectDatabase>) {
        let Some(root) = project_root else {
            return (self.detached.clone(), Vec::new());
        };
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        self.clock += 1;
        let clock = self.clock;
        let entry = self.projects.entry(root.clone()).or_insert_with(|| Entry {
            database: ProjectDatabase::new(),
            last_used: clock,
            last_interactive: 0,
        });
        entry.last_used = clock;
        if interactive {
            entry.last_interactive = clock;
        }
        let database = entry.database.clone();
        let evicted = self.evict_beyond_capacity(&root);
        (database, evicted)
    }

    fn evict_beyond_capacity(&mut self, keep: &Path) -> Vec<ProjectDatabase> {
        let mut evicted = Vec::new();
        while self.projects.len() > self.capacity {
            let Some(victim) = self
                .projects
                .iter()
                .filter(|(root, _)| root.as_path() != keep)
                .min_by_key(|(_, entry)| (entry.last_interactive, entry.last_used))
                .map(|(root, _)| root.clone())
            else {
                break;
            };
            evicted.extend(self.projects.remove(&victim).map(|entry| entry.database));
        }
        evicted
    }

    /// Take every database out (compilation authority changed: manifests, locks, focus). The
    /// caller drops them outside its locks.
    pub(crate) fn clear(&mut self) -> Vec<ProjectDatabase> {
        let mut dropped: Vec<ProjectDatabase> = self.projects.drain().map(|(_, entry)| entry.database).collect();
        dropped.push(std::mem::replace(&mut self.detached, ProjectDatabase::new()));
        dropped
    }

    /// Whether no project database exists yet.
    pub(crate) fn is_empty(&self) -> bool {
        self.projects.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn holds_count(&self) -> usize {
        self.projects.len()
    }

    #[cfg(test)]
    pub(crate) fn holds(&self, project_root: &Path) -> bool {
        let root = project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf());
        self.projects.contains_key(&root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(left: &ProjectDatabase, right: &ProjectDatabase) -> bool {
        Arc::ptr_eq(&left.db, &right.db)
    }

    #[test]
    fn a_project_keeps_its_database_while_other_projects_are_used() {
        let mut databases = ProjectDatabases::with_capacity(4);
        let first = databases.get(Some(Path::new("/w/first")), true).0;
        let second = databases.get(Some(Path::new("/w/second")), true).0;
        assert!(!same(&first, &second), "projects never share a database");
        assert!(same(&first, &databases.get(Some(Path::new("/w/first")), true).0));
        assert!(same(&second, &databases.get(Some(Path::new("/w/second")), false).0));
        assert!(same(&databases.get(None, false).0, &databases.get(None, true).0), "one detached database");
    }

    #[test]
    fn scanned_only_projects_are_evicted_before_edited_ones() {
        let mut databases = ProjectDatabases::with_capacity(3);
        let edited = databases.get(Some(Path::new("/w/edited")), true).0;
        let other_edited = databases.get(Some(Path::new("/w/other")), true).0;
        for index in 0..10 {
            databases.get(Some(Path::new(&format!("/w/scanned{index}"))), false);
        }
        assert!(databases.holds(Path::new("/w/edited")) && databases.holds(Path::new("/w/other")));
        assert!(databases.holds(Path::new("/w/scanned9")), "the project being scanned stays");
        assert!(!databases.holds(Path::new("/w/scanned8")));
        assert!(same(&edited, &databases.get(Some(Path::new("/w/edited")), true).0));
        assert!(same(&other_edited, &databases.get(Some(Path::new("/w/other")), true).0));
    }

    #[test]
    fn the_least_recently_edited_project_goes_when_edited_projects_exceed_capacity() {
        let mut databases = ProjectDatabases::with_capacity(2);
        databases.get(Some(Path::new("/w/a")), true).0;
        databases.get(Some(Path::new("/w/b")), true);
        databases.get(Some(Path::new("/w/a")), true).0;
        databases.get(Some(Path::new("/w/c")), true);
        assert!(databases.holds(Path::new("/w/a")) && databases.holds(Path::new("/w/c")));
        assert!(!databases.holds(Path::new("/w/b")));
    }

    #[test]
    fn a_poisoned_database_is_replaced_by_a_fresh_one() {
        let mut databases = ProjectDatabases::with_capacity(2);
        let database = databases.get(Some(Path::new("/w/a")), true).0;
        let poisoner = database.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.db.lock().expect("lock");
            panic!("handler bug while holding the database");
        })
        .join();
        assert!(database.db.is_poisoned());
        drop(database.lock());
        assert!(!database.db.is_poisoned());
    }

    #[test]
    fn evicted_and_cleared_databases_are_handed_back() {
        let mut databases = ProjectDatabases::with_capacity(1);
        let first = databases.get(Some(Path::new("/w/a")), true).0;
        let (_, evicted) = databases.get(Some(Path::new("/w/b")), true);
        assert_eq!(evicted.len(), 1);
        assert!(same(&first, &evicted[0]));
        assert_eq!(databases.holds_count(), 1);
    }

    #[test]
    fn a_database_starts_afresh_after_its_analysis_budget() {
        use beskid_queries::{bump_file_revision, file_revision_for};

        let mut databases = ProjectDatabases::with_capacity(1);
        let database = databases.get(Some(Path::new("/w/a")), true).0;
        bump_file_revision(&mut database.lock_for_analysis(), "entry");
        for _ in 1..ANALYSES_PER_DATABASE {
            let db = database.lock_for_analysis();
            assert_eq!(file_revision_for(&*db, "entry"), 1, "the storage is kept within the budget");
        }
        let db = database.lock_for_analysis();
        assert_eq!(file_revision_for(&*db, "entry"), 0, "the storage started afresh");
    }

    #[test]
    fn clearing_drops_every_database() {
        let mut databases = ProjectDatabases::with_capacity(2);
        let before = databases.get(Some(Path::new("/w/a")), true).0;
        let detached = databases.get(None, false).0;
        assert_eq!(databases.clear().len(), 2);
        assert!(databases.is_empty());
        assert!(!same(&before, &databases.get(Some(Path::new("/w/a")), true).0));
        assert!(!same(&detached, &databases.get(None, false).0));
    }
}
