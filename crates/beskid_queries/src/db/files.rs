use std::path::{Path, PathBuf};

use salsa::Setter;

use crate::inputs::{FileText, ProjectSession};
use crate::stats::record_revision_bump;

use super::BeskidDatabase;

impl BeskidDatabase {
    /// Register or update file text (canonical path key).
    pub fn set_file_text(&mut self, path: PathBuf, text: String) {
        let canonical = path.canonicalize().unwrap_or(path);
        let old_text = self.file_text(&canonical).map(|f| f.text(self).clone());
        record_revision_bump();
        let mut fps = vec![beskid_artifacts::content_fingerprint(&text)];
        if let Some(old) = old_text.as_deref() {
            fps.push(beskid_artifacts::content_fingerprint(old));
        }
        self.invalidate_unit_fingerprints(&fps);
        self.set_file_text_inner(canonical.clone(), text);
        if let Some(session) = self.active_project_session() {
            self.invalidate_import_dependents(session, canonical, self.known_file_paths());
        }
    }

    /// Register file text when changed; skips revision bump and cache clear on identical content.
    pub fn ensure_file_text(&mut self, path: PathBuf, text: String) {
        let canonical = path.canonicalize().unwrap_or(path.clone());
        if let Some(existing) = self.file_text(&canonical)
            && existing.text(self) == &text
        {
            return;
        }
        record_revision_bump();
        let old_text = self.file_text(&canonical).map(|f| f.text(self).clone());
        let mut fps = vec![beskid_artifacts::content_fingerprint(&text)];
        if let Some(old) = old_text.as_deref() {
            fps.push(beskid_artifacts::content_fingerprint(old));
        }
        self.invalidate_unit_fingerprints(&fps);
        self.set_file_text_inner(canonical.clone(), text);
        if let Some(session) = self.active_project_session() {
            self.invalidate_import_dependents(session, canonical, self.known_file_paths());
        }
    }

    fn known_file_paths(&self) -> Vec<PathBuf> {
        self.file_registry.lock().expect("file registry").keys().cloned().collect()
    }

    fn active_project_session(&self) -> Option<ProjectSession> {
        self.project_registry.lock().expect("project registry").values().next().copied()
    }

    fn set_file_text_inner(&mut self, canonical: PathBuf, text: String) {
        let existing = {
            let registry = self.file_registry.lock().expect("file registry");
            registry.get(&canonical).copied()
        };
        if let Some(existing) = existing {
            existing.set_text(self).to(text.clone());
        } else {
            let file = FileText::new(self, canonical.clone(), text.clone());
            self.file_registry.lock().expect("file registry").insert(canonical.clone(), file);
        }
        if let Some(root) = &self.persistence_root {
            let _ = crate::persistence::persist_file_text(root, &canonical, &text);
        }
    }

    pub fn file_text(&self, path: &Path) -> Option<FileText> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.file_registry.lock().expect("file registry").get(&canonical).copied()
    }
}
