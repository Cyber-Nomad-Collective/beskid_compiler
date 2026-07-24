//! Process-scoped compilation session metadata (entry registry in [`super::entry_session`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::composition::CompositionSnapshot;
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
        let project_root = plan.project_root.canonicalize().unwrap_or_else(|_| plan.project_root.clone());
        let entry_canonical = entry_path.canonicalize().unwrap_or_else(|_| entry_path.to_path_buf());
        let lockfile_digest = lockfile_digest_for_plan(plan);
        Self { project_root, entry_canonical, lockfile_digest }
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

/// Versioned semantic snapshot at pipeline boundaries (`semantic.snapshot` and later).
#[derive(Debug, Clone)]
pub struct SemanticSnapshot {
    pub version: u32,
    pub syntax_generation_id: u64,
    pub diagnostic_count: usize,
    pub diagnostic_fingerprint: u64,
    pub composition_fingerprint: u64,
    pub resolution_fingerprint: u64,
    pub typed_fingerprint: u64,
    /// Minimum pipeline stage materialized in this snapshot (`semantic`, `composition`, `executable`).
    pub staged_through: &'static str,
}

pub const SEMANTIC_SNAPSHOT_VERSION: u32 = 1;

impl SemanticSnapshot {
    pub fn from_diagnostics(
        diagnostics: &[crate::analysis::SemanticDiagnostic],
        syntax_generation_id: u64,
        staged_through: &'static str,
    ) -> Self {
        Self {
            version: SEMANTIC_SNAPSHOT_VERSION,
            syntax_generation_id,
            diagnostic_count: diagnostics.len(),
            diagnostic_fingerprint: fingerprint_diagnostics(diagnostics),
            composition_fingerprint: 0,
            resolution_fingerprint: 0,
            typed_fingerprint: 0,
            staged_through,
        }
    }

    pub fn with_composition(mut self, composition: &CompositionSnapshot) -> Self {
        self.composition_fingerprint = super::entry_session::composition_fingerprint(composition);
        self.staged_through = "composition";
        self
    }

    pub fn with_typed_resolution(mut self, resolution_fingerprint: u64, typed_fingerprint: u64) -> Self {
        self.resolution_fingerprint = resolution_fingerprint;
        self.typed_fingerprint = typed_fingerprint;
        self.staged_through = "executable";
        self
    }

    /// Whether this snapshot has reached at least `minimum_stage` in the prepare spine.
    pub fn satisfies_minimum(&self, minimum_stage: &str) -> bool {
        stage_rank(self.staged_through) >= stage_rank(minimum_stage)
    }
}

fn stage_rank(stage: &str) -> u8 {
    match stage {
        "semantic" => 1,
        "composition" => 2,
        "executable" => 3,
        _ => 0,
    }
}

fn fingerprint_diagnostics(diagnostics: &[crate::analysis::SemanticDiagnostic]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    for diagnostic in diagnostics {
        diagnostic.message.hash(&mut hasher);
        diagnostic.span.offset().hash(&mut hasher);
        diagnostic.span.len().hash(&mut hasher);
        if let Some(code) = diagnostic.code.as_ref() {
            code.hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Register assembly for an entry (see [`super::entry_session::get_or_insert_assembly`]).
pub fn session_for_assembly(fingerprint: SessionFingerprint, assembly: ProgramAssembly) -> Arc<CompilationSession> {
    super::entry_session::get_or_insert_assembly(fingerprint, assembly)
}

/// Store optional executable front-end and semantic snapshot on an existing session.
pub fn store_executable_on_session(
    fingerprint: &SessionFingerprint,
    executable: Option<FrontEndTypedResult>,
    snapshot: SemanticSnapshot,
) {
    super::entry_session::store_executable_and_snapshot(fingerprint, executable, snapshot);
}

/// Lookup cached executable front-end for an entry fingerprint.
pub fn cached_executable(fingerprint: &SessionFingerprint) -> Option<Arc<FrontEndTypedResult>> {
    super::entry_session::cached_executable(fingerprint)
}

/// Lookup cached semantic snapshot for an entry fingerprint.
pub fn cached_semantic_snapshot(fingerprint: &SessionFingerprint) -> Option<SemanticSnapshot> {
    super::entry_session::cached_semantic_snapshot(fingerprint)
}

/// Lookup the full compilation session for an entry fingerprint.
pub fn cached_compilation_session(fingerprint: &SessionFingerprint) -> Option<Arc<CompilationSession>> {
    super::entry_session::cached_compilation_session(fingerprint)
}
