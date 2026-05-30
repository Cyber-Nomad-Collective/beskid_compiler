//! Process-scoped compilation session metadata (Salsa owns memoization).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::projects::ProgramAssembly;
use crate::services::front_end::FrontEndTypedResult;

/// Stable key for a compilation session within a process.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionFingerprint {
    pub project_root: PathBuf,
    pub entry_canonical: PathBuf,
    pub lockfile_digest: u64,
}

impl SessionFingerprint {
    pub fn for_entry(plan: &crate::projects::CompilePlan, entry_path: &Path) -> Self {
        let project_root = plan.project_root.clone();
        let entry_canonical = entry_path
            .canonicalize()
            .unwrap_or_else(|_| entry_path.to_path_buf());
        let lockfile_digest = lockfile_digest_for_plan(plan);
        Self {
            project_root,
            entry_canonical,
            lockfile_digest,
        }
    }
}

fn lockfile_digest_for_plan(plan: &crate::projects::CompilePlan) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    plan.project_root.hash(&mut hasher);
    plan.target.entry.hash(&mut hasher);
    plan.target.name.hash(&mut hasher);
    if let Ok(bytes) = std::fs::read(plan.project_root.join("Project.lock")) {
        bytes.hash(&mut hasher);
    }
    hasher.finish()
}

/// Cached assembly and optional executable front-end for one entry.
#[derive(Debug)]
pub struct CompilationSession {
    pub fingerprint: SessionFingerprint,
    pub assembly: Arc<ProgramAssembly>,
    pub prepared_executable: Option<Arc<FrontEndTypedResult>>,
    pub semantic_snapshot: Option<SemanticSnapshot>,
}

/// Lightweight semantic snapshot populated at SEMANTIC_SNAPSHOT phase.
#[derive(Debug, Clone)]
pub struct SemanticSnapshot {
    pub resolution_fingerprint: u64,
    pub typed_fingerprint: u64,
    pub diagnostic_count: usize,
}

impl SemanticSnapshot {
    pub fn from_diagnostics(diagnostics: &[crate::analysis::SemanticDiagnostic]) -> Self {
        let mut hasher = DefaultHasher::new();
        for diagnostic in diagnostics {
            diagnostic.message.hash(&mut hasher);
            diagnostic.span.offset().hash(&mut hasher);
            diagnostic.span.len().hash(&mut hasher);
        }
        Self {
            resolution_fingerprint: hasher.finish(),
            typed_fingerprint: 0,
            diagnostic_count: diagnostics.len(),
        }
    }
}

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Wrap assembly for the prepare spine; memoization lives in `beskid_queries::BeskidDatabase`.
pub fn session_for_assembly(
    fingerprint: SessionFingerprint,
    assembly: ProgramAssembly,
) -> Arc<CompilationSession> {
    Arc::new(CompilationSession {
        fingerprint,
        assembly: Arc::new(assembly),
        prepared_executable: None,
        semantic_snapshot: None,
    })
}

/// Store optional executable front-end and semantic snapshot on an existing session.
pub fn store_executable_on_session(
    _fingerprint: &SessionFingerprint,
    _executable: Option<FrontEndTypedResult>,
    _snapshot: SemanticSnapshot,
) {
}

/// Lookup cached executable front-end for an entry fingerprint.
pub fn cached_executable(_fingerprint: &SessionFingerprint) -> Option<Arc<FrontEndTypedResult>> {
    None
}

/// Lookup cached semantic snapshot for an entry fingerprint.
pub fn cached_semantic_snapshot(_fingerprint: &SessionFingerprint) -> Option<SemanticSnapshot> {
    None
}
