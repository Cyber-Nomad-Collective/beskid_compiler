//! Map diagnostics from materialized copies under `obj/beskid/` back to the authored source.
//!
//! Project preparation mirrors the host source root and every path dependency into
//! `<project>/obj/beskid/` and records each mirrored root with its origin in
//! `obj/beskid/origins.json` ([`SourceOrigins`]). Before any renderer runs, the compiler renames a
//! diagnostic whose file lies under a recorded root to the authored file. The mirror is a byte copy,
//! so spans stay valid; when the authored file changed after materialization the diagnostic keeps
//! the materialized path rather than pointing at the wrong text.
//!
//! Registry packages are not recorded: the package cache keeps only artifact archives, so the
//! read-only tree extracted under `obj/beskid/deps/src/` is the package source. Corelib is not
//! recorded either: its units are read in place from the toolchain's shared Corelib, so their
//! diagnostics already name the shared files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::NamedSource;
use serde::{Deserialize, Serialize};

use super::SemanticDiagnostic;

/// File name of the origin record inside `obj/beskid/`.
pub const ORIGINS_FILE: &str = "origins.json";
const SCHEMA: u32 = 1;

/// One mirrored root: `materialized` is relative to `obj/beskid/`, `origin` is absolute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginRoot {
    pub materialized: PathBuf,
    pub origin: PathBuf,
}

/// The origin record of one project's materialized tree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceOrigins {
    pub schema: u32,
    pub roots: Vec<OriginRoot>,
}

impl SourceOrigins {
    pub fn new() -> Self {
        Self { schema: SCHEMA, roots: Vec::new() }
    }

    /// Record that `materialized` (absolute, inside `obj_beskid`) mirrors `origin`.
    pub fn record(&mut self, obj_beskid: &Path, materialized: &Path, origin: &Path) {
        let Ok(relative) = materialized.strip_prefix(obj_beskid) else { return };
        let origin = origin.canonicalize().unwrap_or_else(|_| origin.to_path_buf());
        self.roots.retain(|root| root.materialized != relative);
        self.roots.push(OriginRoot { materialized: relative.to_path_buf(), origin });
    }

    /// Write `obj_beskid/origins.json`.
    pub fn write(&self, obj_beskid: &Path) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(obj_beskid.join(ORIGINS_FILE), text)
    }

    /// Read `obj_beskid/origins.json`; a missing or unreadable record maps nothing.
    pub fn load(obj_beskid: &Path) -> Self {
        std::fs::read_to_string(obj_beskid.join(ORIGINS_FILE))
            .ok()
            .and_then(|text| serde_json::from_str::<Self>(&text).ok())
            .filter(|origins| origins.schema == SCHEMA)
            .unwrap_or_default()
    }

    /// Authored path of `path` (absolute, inside `obj_beskid`), by the longest recorded root.
    pub fn map(&self, obj_beskid: &Path, path: &Path) -> Option<PathBuf> {
        let relative = path.strip_prefix(obj_beskid).ok()?;
        self.roots
            .iter()
            .filter(|root| relative.starts_with(&root.materialized))
            .max_by_key(|root| root.materialized.components().count())
            .map(|root| root.origin.join(relative.strip_prefix(&root.materialized).expect("prefix checked")))
    }
}

/// The `obj/beskid` directory that contains `path`, if any.
fn obj_beskid_ancestor(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| {
            ancestor.file_name().is_some_and(|name| name == "beskid")
                && ancestor.parent().and_then(Path::file_name).is_some_and(|name| name == "obj")
        })
        .map(Path::to_path_buf)
}

/// Maps file names of diagnostics to authored sources, reading each origin record and each
/// authored file once.
#[derive(Debug, Default)]
pub struct SourceMapper {
    origins: HashMap<PathBuf, SourceOrigins>,
    authored_text: HashMap<PathBuf, Option<String>>,
}

impl SourceMapper {
    pub fn new() -> Self {
        Self::default()
    }

    /// Authored file name for `name` when `name` is a materialized copy with a recorded origin.
    pub fn authored_name(&mut self, name: &str) -> Option<String> {
        let path = Path::new(name);
        let obj_beskid = obj_beskid_ancestor(path)?;
        let origins = self.origins.entry(obj_beskid.clone()).or_insert_with(|| SourceOrigins::load(&obj_beskid));
        origins.map(&obj_beskid, path).map(|authored| authored.display().to_string())
    }

    /// The authored name for a file whose diagnostic text is `text`, only when the authored file
    /// still holds exactly that text (so spans stay correct).
    fn authored_for_text(&mut self, name: &str, text: &str) -> Option<String> {
        let authored = self.authored_name(name)?;
        let current = self
            .authored_text
            .entry(PathBuf::from(&authored))
            .or_insert_with(|| std::fs::read_to_string(&authored).ok());
        (current.as_deref() == Some(text)).then_some(authored)
    }

    /// Rename the primary and related sources of `diagnostic` to their authored files.
    pub fn map_diagnostic(&mut self, mut diagnostic: SemanticDiagnostic) -> SemanticDiagnostic {
        if let Some(authored) = self.authored_for_text(diagnostic.file_name(), diagnostic.source_text()) {
            let text = diagnostic.source_text().to_string();
            diagnostic.src = NamedSource::new(authored, text);
        }
        let primary = diagnostic.file_name().to_string();
        for related in &mut diagnostic.related {
            let Some(source) = related.source.as_mut() else { continue };
            if let Some(authored) = self.authored_for_text(&source.name, &source.text) {
                source.name = authored;
            }
            if source.name == primary {
                related.source = None;
            }
        }
        diagnostic
    }
}

/// Map every diagnostic to its authored source, then drop duplicates (first reporter wins).
pub fn map_and_dedupe(diagnostics: Vec<SemanticDiagnostic>) -> Vec<SemanticDiagnostic> {
    let mut mapper = SourceMapper::new();
    let mapped = diagnostics.into_iter().map(|diagnostic| mapper.map_diagnostic(diagnostic)).collect();
    super::dedupe_diagnostics(mapped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::diagnostics::{Severity, make_diagnostic};
    use crate::syntax::SpanInfo;

    const TEXT: &str = "i32 Main() { return x; }";

    fn diagnostic(name: &str) -> SemanticDiagnostic {
        make_diagnostic(
            name,
            TEXT,
            SpanInfo { start: 20, end: 21, line_col_start: (1, 21), line_col_end: (1, 22) },
            "Unknown value `x`.",
            "not found",
            None,
            Some("E1101".into()),
            Severity::Error,
        )
    }

    /// A project with `Src/Main.bd` mirrored to `obj/beskid/root/Src/Main.bd` and its origin record.
    fn materialized_project() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let project = tempfile::tempdir().expect("project");
        let authored = project.path().join("Src/Main.bd");
        let obj = project.path().join("obj/beskid");
        let mirrored = obj.join("root/Src/Main.bd");
        for path in [&authored, &mirrored] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, TEXT).unwrap();
        }
        let mut origins = SourceOrigins::new();
        origins.record(&obj, &obj.join("root/Src"), &project.path().join("Src"));
        origins.write(&obj).unwrap();
        (project, authored.canonicalize().unwrap(), mirrored)
    }

    #[test]
    fn materialized_diagnostic_maps_to_the_authored_file() {
        let (_project, authored, mirrored) = materialized_project();
        let mapped = SourceMapper::new().map_diagnostic(diagnostic(mirrored.to_str().unwrap()));
        assert_eq!(mapped.file_name(), authored.to_str().unwrap());
        assert_eq!(mapped.range(), (20, 21));
    }

    #[test]
    fn changed_authored_file_keeps_the_materialized_path() {
        let (_project, authored, mirrored) = materialized_project();
        std::fs::write(&authored, "// edited\n").unwrap();
        let mapped = SourceMapper::new().map_diagnostic(diagnostic(mirrored.to_str().unwrap()));
        assert_eq!(mapped.file_name(), mirrored.to_str().unwrap());
    }

    #[test]
    fn unrecorded_and_ordinary_paths_are_unchanged() {
        let (project, _authored, _mirrored) = materialized_project();
        let registry = project.path().join("obj/beskid/deps/src/Pkg/Src/Lib.bd");
        let mut mapper = SourceMapper::new();
        assert_eq!(mapper.authored_name(registry.to_str().unwrap()), None);
        assert_eq!(mapper.map_diagnostic(diagnostic("Src/Main.bd")).file_name(), "Src/Main.bd");
    }

    #[test]
    fn the_same_cause_from_the_copy_and_the_authored_file_is_reported_once() {
        let (_project, authored, mirrored) = materialized_project();
        let kept = map_and_dedupe(vec![
            diagnostic(authored.to_str().unwrap()),
            diagnostic(mirrored.to_str().unwrap()).with_help("second phase"),
        ]);
        assert_eq!(kept.len(), 1);
        assert!(kept[0].help.is_none(), "the first reporter wins");
    }

    /// Corelib units are read in place from the toolchain's shared Corelib, so a diagnostic in
    /// Corelib already names the shared file; mapping leaves it, its span and its text unchanged,
    /// while host diagnostics of the same project still map from the mirror.
    #[test]
    fn a_corelib_diagnostic_points_at_the_shared_file() {
        let (project, authored, mirrored) = materialized_project();
        let toolchain = tempfile::tempdir().expect("toolchain");
        let shared = toolchain.path().join("beskid_corelib/packages/console/src/Console/Style.bd");
        std::fs::create_dir_all(shared.parent().unwrap()).unwrap();
        std::fs::write(&shared, TEXT).unwrap();
        let obj = project.path().join("obj/beskid");
        assert!(
            SourceOrigins::load(&obj).roots.iter().all(|root| !root.origin.starts_with(toolchain.path())),
            "the shared Corelib is never a mirrored root"
        );

        let mut mapper = SourceMapper::new();
        let corelib = mapper.map_diagnostic(diagnostic(shared.to_str().unwrap()));
        assert_eq!(corelib.file_name(), shared.to_str().unwrap());
        assert_eq!(corelib.range(), (20, 21));
        assert_eq!(
            mapper.map_diagnostic(diagnostic(mirrored.to_str().unwrap())).file_name(),
            authored.to_str().unwrap()
        );
    }

    #[test]
    fn longest_recorded_root_wins() {
        let obj = PathBuf::from("/p/obj/beskid");
        let mut origins = SourceOrigins::new();
        origins.record(&obj, &obj.join("root"), Path::new("/nonexistent/a"));
        origins.record(&obj, &obj.join("root/.generated"), Path::new("/nonexistent/b"));
        assert_eq!(origins.map(&obj, &obj.join("root/.generated/X.bd")), Some(PathBuf::from("/nonexistent/b/X.bd")));
        assert_eq!(origins.map(&obj, &obj.join("root/Src/X.bd")), Some(PathBuf::from("/nonexistent/a/Src/X.bd")));
    }
}
