use std::path::Path;

use beskid_analysis::projects::CompilePlan;
use salsa::Setter;

use crate::inputs::ProjectSession;

use super::BeskidDatabase;

impl BeskidDatabase {
    pub fn ensure_project_session(
        &mut self,
        plan: &CompilePlan,
        entry_path: &Path,
        lockfile_digest: String,
    ) -> ProjectSession {
        let key = (
            plan.project_root.clone(),
            entry_path.canonicalize().unwrap_or_else(|_| entry_path.to_path_buf()),
            plan.target.name.clone(),
        );
        let existing = {
            let registry = self.project_registry.lock().expect("project registry");
            registry.get(&key).copied()
        };
        if let Some(existing) = existing {
            existing.set_lockfile_digest(self).to(lockfile_digest);
            return existing;
        }
        let session = ProjectSession::new(
            self,
            plan.project_root.clone(),
            key.1.clone(),
            plan.target.name.clone(),
            lockfile_digest,
        );
        self.project_registry.lock().expect("project registry").insert(key, session);
        session
    }
}
