//! Process-wide entry session registry keyed by [`SessionFingerprint`].

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use crate::composition::CompositionSnapshot;
use crate::projects::ProgramAssembly;
use crate::syntax::SyntaxGenerationId;

use super::front_end::FrontEndTypedResult;
use super::session::{CompilationSession, SemanticSnapshot, SessionFingerprint};

static REGISTRY: OnceLock<Mutex<EntrySessionRegistry>> = OnceLock::new();

struct EntrySessionRegistry {
    sessions: HashMap<SessionFingerprint, Arc<CompilationSession>>,
    syntax_generation: HashMap<SessionFingerprint, u64>,
    executable_weak: HashMap<SessionFingerprint, Weak<FrontEndTypedResult>>,
}

fn registry() -> &'static Mutex<EntrySessionRegistry> {
    REGISTRY.get_or_init(|| {
        Mutex::new(EntrySessionRegistry {
            sessions: HashMap::new(),
            syntax_generation: HashMap::new(),
            executable_weak: HashMap::new(),
        })
    })
}

/// Monotonic syntax generation id for an entry (bumps after mod-host re-parse).
pub fn next_syntax_generation_id(fingerprint: &SessionFingerprint) -> u64 {
    let mut guard = registry().lock().expect("entry session registry");
    let next = SyntaxGenerationId::allocate().expect("syntax generation identities exhausted").0;
    guard.syntax_generation.insert(fingerprint.clone(), next);
    next
}

/// Current syntax generation without bumping.
pub fn current_syntax_generation_id(fingerprint: &SessionFingerprint) -> u64 {
    let guard = registry().lock().expect("entry session registry");
    guard.syntax_generation.get(fingerprint).copied().unwrap_or(1)
}

pub fn get_or_insert_assembly(
    fingerprint: SessionFingerprint,
    assembly: ProgramAssembly,
) -> anyhow::Result<Arc<CompilationSession>> {
    let mut guard = registry().lock().expect("entry session registry");
    if let Some(session) = guard.sessions.get(&fingerprint) {
        anyhow::ensure!(
            assembly.generation >= session.assembly.generation,
            "entry session syntax generation cannot regress"
        );
        if assembly.generation == session.assembly.generation {
            anyhow::ensure!(
                assembly.roots == session.assembly.roots
                    && assembly.entry_index == session.assembly.entry_index
                    && assembly.discovery == session.assembly.discovery
                    && assembly.has_std_dependency == session.assembly.has_std_dependency
                    && assembly.trusted_corelib_service_paths == session.assembly.trusted_corelib_service_paths
                    && assembly.module_index.module_graph() == session.assembly.module_index.module_graph()
                    && assembly.module_index.prefetched_paths() == session.assembly.module_index.prefetched_paths()
                    && match (&assembly.runtime_fixture, &session.assembly.runtime_fixture) {
                        (Some(incoming), Some(current)) => Arc::ptr_eq(incoming, current),
                        (None, None) => true,
                        _ => false,
                    }
                    && assembly.units.len() == session.assembly.units.len()
                    && assembly.units.iter().zip(session.assembly.units.iter()).all(|(incoming, current)| {
                        incoming.path == current.path
                            && incoming.origin_path == current.origin_path
                            && incoming.logical_name == current.logical_name
                            && incoming.source == current.source
                            && incoming.program == current.program
                    }),
                "one entry session syntax generation cannot describe different sources"
            );
            return Ok(Arc::clone(session));
        }
    }
    assembly.generation.resume_after();
    guard.syntax_generation.insert(fingerprint.clone(), assembly.generation.0);
    guard.executable_weak.remove(&fingerprint);
    let session = Arc::new(CompilationSession {
        fingerprint: fingerprint.clone(),
        assembly: Arc::new(assembly),
        prepared_executable: None,
        semantic_snapshot: None,
    });
    guard.sessions.insert(fingerprint, Arc::clone(&session));
    Ok(session)
}

pub fn update_semantic_snapshot(fingerprint: &SessionFingerprint, snapshot: SemanticSnapshot) {
    let mut guard = registry().lock().expect("entry session registry");
    let Some(session) = guard.sessions.get_mut(fingerprint) else {
        return;
    };
    if snapshot.syntax_generation_id != session.assembly.generation.0 {
        return;
    }
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
) -> Option<Arc<FrontEndTypedResult>> {
    let mut guard = registry().lock().expect("entry session registry");
    let session = guard.sessions.get(fingerprint).cloned()?;
    if snapshot.syntax_generation_id != session.assembly.generation.0
        || executable.as_ref().is_some_and(|front| front.assembly.generation != session.assembly.generation)
    {
        return None;
    }
    let stored = executable.map(Arc::new);
    if let Some(arc) = stored.as_ref() {
        guard.executable_weak.insert(fingerprint.clone(), Arc::downgrade(arc));
    }
    let updated = Arc::new(CompilationSession {
        fingerprint: session.fingerprint.clone(),
        assembly: Arc::clone(&session.assembly),
        prepared_executable: None,
        semantic_snapshot: Some(snapshot),
    });
    guard.sessions.insert(fingerprint.clone(), updated);
    stored
}

pub fn cached_compilation_session(fingerprint: &SessionFingerprint) -> Option<Arc<CompilationSession>> {
    let guard = registry().lock().expect("entry session registry");
    guard.sessions.get(fingerprint).cloned()
}

pub fn cached_semantic_snapshot(fingerprint: &SessionFingerprint) -> Option<SemanticSnapshot> {
    cached_compilation_session(fingerprint).and_then(|s| s.semantic_snapshot.clone())
}

pub fn cached_executable(fingerprint: &SessionFingerprint) -> Option<Arc<FrontEndTypedResult>> {
    let guard = registry().lock().expect("entry session registry");
    guard.executable_weak.get(fingerprint)?.upgrade()
}

/// Cached executable when the entry fingerprint and syntax generation still match.
pub fn cached_executable_if_valid(
    fingerprint: &SessionFingerprint,
    generation: SyntaxGenerationId,
) -> Option<Arc<FrontEndTypedResult>> {
    let guard = registry().lock().expect("entry session registry");
    let session = guard.sessions.get(fingerprint)?;
    let snapshot = session.semantic_snapshot.as_ref()?;
    if session.assembly.generation != generation
        || snapshot.syntax_generation_id != generation.0
        || guard.syntax_generation.get(fingerprint).copied() != Some(generation.0)
        || !snapshot.satisfies_minimum("executable")
    {
        return None;
    }
    let executable = guard.executable_weak.get(fingerprint)?.upgrade()?;
    (executable.assembly.generation == generation).then_some(executable)
}

fn canonical_path(path: &Path) -> std::path::PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub fn invalidate_project(project_root: &Path) {
    let canonical = canonical_path(project_root);
    let mut guard = registry().lock().expect("entry session registry");
    guard.sessions.retain(|fp, _| canonical_path(&fp.project_root) != canonical);
    guard.syntax_generation.retain(|fp, _| canonical_path(&fp.project_root) != canonical);
    guard.executable_weak.retain(|fp, _| canonical_path(&fp.project_root) != canonical);
}

pub fn invalidate_all() {
    let mut guard = registry().lock().expect("entry session registry");
    guard.sessions.clear();
    guard.syntax_generation.clear();
    guard.executable_weak.clear();
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
        assert_eq!(composition_fingerprint(&snap), composition_fingerprint(&snap));
    }
}
