//! Process-wide entry session registry keyed by [`SessionFingerprint`].

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use crate::composition::CompositionSnapshot;
use crate::projects::ProgramAssembly;

use super::front_end::FrontEndTypedResult;
use super::session::{CompilationSession, SemanticSnapshot, SessionFingerprint};

static REGISTRY: OnceLock<Mutex<EntrySessionRegistry>> = OnceLock::new();

struct EntrySessionRegistry {
    sessions: HashMap<SessionFingerprint, Arc<CompilationSession>>,
    syntax_generation: HashMap<SessionFingerprint, u64>,
}

fn registry() -> &'static Mutex<EntrySessionRegistry> {
    REGISTRY.get_or_init(|| {
        Mutex::new(EntrySessionRegistry {
            sessions: HashMap::new(),
            syntax_generation: HashMap::new(),
        })
    })
}

/// Monotonic syntax generation id for an entry (bumps after mod-host re-parse).
pub fn next_syntax_generation_id(fingerprint: &SessionFingerprint) -> u64 {
    let mut guard = registry().lock().expect("entry session registry");
    let next = guard
        .syntax_generation
        .entry(fingerprint.clone())
        .and_modify(|id| *id = id.saturating_add(1))
        .or_insert(1);
    *next
}

/// Current syntax generation without bumping.
pub fn current_syntax_generation_id(fingerprint: &SessionFingerprint) -> u64 {
    let guard = registry().lock().expect("entry session registry");
    guard.syntax_generation.get(fingerprint).copied().unwrap_or(1)
}

pub fn get_or_insert_assembly(
    fingerprint: SessionFingerprint,
    assembly: ProgramAssembly,
) -> Arc<CompilationSession> {
    let mut guard = registry().lock().expect("entry session registry");
    if let Some(session) = guard.sessions.get(&fingerprint) {
        return Arc::clone(session);
    }
    guard
        .syntax_generation
        .entry(fingerprint.clone())
        .or_insert(1);
    let session = Arc::new(CompilationSession {
        fingerprint: fingerprint.clone(),
        assembly: Arc::new(assembly),
        prepared_executable: None,
        semantic_snapshot: None,
    });
    guard
        .sessions
        .insert(fingerprint, Arc::clone(&session));
    session
}

pub fn update_semantic_snapshot(fingerprint: &SessionFingerprint, snapshot: SemanticSnapshot) {
    let mut guard = registry().lock().expect("entry session registry");
    let Some(session) = guard.sessions.get_mut(fingerprint) else {
        return;
    };
    let updated = Arc::new(CompilationSession {
        fingerprint: session.fingerprint.clone(),
        assembly: Arc::clone(&session.assembly),
        prepared_executable: session.prepared_executable.clone(),
        semantic_snapshot: Some(snapshot),
    });
    guard.sessions.insert(fingerprint.clone(), updated);
}

pub fn store_executable_and_snapshot(
    fingerprint: &SessionFingerprint,
    executable: Option<FrontEndTypedResult>,
    snapshot: SemanticSnapshot,
) {
    let mut guard = registry().lock().expect("entry session registry");
    let Some(session) = guard.sessions.get(fingerprint) else {
        return;
    };
    let updated = Arc::new(CompilationSession {
        fingerprint: session.fingerprint.clone(),
        assembly: Arc::clone(&session.assembly),
        prepared_executable: executable.map(Arc::new),
        semantic_snapshot: Some(snapshot),
    });
    guard.sessions.insert(fingerprint.clone(), updated);
}

pub fn cached_compilation_session(
    fingerprint: &SessionFingerprint,
) -> Option<Arc<CompilationSession>> {
    let guard = registry().lock().expect("entry session registry");
    guard.sessions.get(fingerprint).cloned()
}

pub fn cached_semantic_snapshot(fingerprint: &SessionFingerprint) -> Option<SemanticSnapshot> {
    cached_compilation_session(fingerprint).and_then(|s| s.semantic_snapshot.clone())
}

pub fn cached_executable(fingerprint: &SessionFingerprint) -> Option<Arc<FrontEndTypedResult>> {
    cached_compilation_session(fingerprint).and_then(|s| s.prepared_executable.clone())
}

fn canonical_path(path: &Path) -> std::path::PathBuf {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
}

pub fn invalidate_project(project_root: &Path) {
    let canonical = canonical_path(project_root);
    let mut guard = registry().lock().expect("entry session registry");
    guard
        .sessions
        .retain(|fp, _| canonical_path(&fp.project_root) != canonical);
    guard
        .syntax_generation
        .retain(|fp, _| canonical_path(&fp.project_root) != canonical);
}

pub fn invalidate_all() {
    let mut guard = registry().lock().expect("entry session registry");
    guard.sessions.clear();
    guard.syntax_generation.clear();
}

pub fn composition_fingerprint(snapshot: &CompositionSnapshot) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    snapshot.version.hash(&mut hasher);
    snapshot.launched_host.hash(&mut hasher);
    snapshot.registrations.len().hash(&mut hasher);
    for registration in &snapshot.registrations {
        registration.id.hash(&mut hasher);
        registration.scope_id.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composition::CompositionSnapshot;

    #[test]
    fn composition_fingerprint_is_stable_for_same_snapshot() {
        let snap = CompositionSnapshot::default();
        assert_eq!(
            composition_fingerprint(&snap),
            composition_fingerprint(&snap)
        );
    }
}
