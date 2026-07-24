//! On-disk layout: `{cache_root}/units/{content_fp}/{meta.json, ast.bin}`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static ARTIFACT_STORE_WRITE_LOCK: Mutex<()> = Mutex::new(());

use crate::fingerprint::grammar_revision;
use crate::manifest::{ARTIFACT_SCHEMA_VERSION, ArtifactManifest};
use crate::snapshot::{AstUnitSnapshot, decode_ast, encode_ast};

#[derive(Debug, Clone)]
pub struct UnitArtifactPaths {
    pub unit_dir: PathBuf,
    pub meta: PathBuf,
    pub ast: PathBuf,
}

pub struct ArtifactStore {
    cache_root: PathBuf,
}

impl ArtifactStore {
    pub fn new(project_root: &Path) -> Self {
        Self { cache_root: cache_root_for_project(project_root) }
    }

    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        let _guard = ARTIFACT_STORE_WRITE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.ensure_dirs_locked()
    }

    fn ensure_dirs_locked(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.cache_root)?;
        let manifest_path = self.cache_root.join("manifest.json");
        let manifest = self.manifest();
        let is_current = manifest.as_ref().is_some_and(|manifest| {
            manifest.grammar_rev == grammar_revision() && manifest.schema_version == ARTIFACT_SCHEMA_VERSION
        });
        if is_current {
            fs::create_dir_all(self.cache_root.join("units"))?;
            return Ok(());
        }

        self.replace_stale_units_tree()?;
        let manifest = ArtifactManifest {
            grammar_rev: grammar_revision().to_string(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            schema_version: ARTIFACT_SCHEMA_VERSION,
            persisted_units: 0,
        };
        write_atomically(&manifest_path, serde_json::to_string_pretty(&manifest)?.as_bytes())?;
        Ok(())
    }

    /// Replace the complete stale unit namespace before a current manifest can be published.
    fn replace_stale_units_tree(&self) -> std::io::Result<()> {
        let units = self.cache_root.join("units");
        let staging = self.cache_root.join("units.schema-current.tmp");
        let stale = self.cache_root.join("units.schema-stale.purge");
        remove_dir_if_present(&staging)?;
        remove_dir_if_present(&stale)?;
        fs::create_dir(&staging)?;

        if units.exists() {
            fs::rename(&units, &stale)?;
        }
        if let Err(error) = fs::rename(&staging, &units) {
            if stale.exists() {
                let _ = fs::rename(&stale, &units);
            }
            return Err(error);
        }
        if let Err(error) = remove_dir_if_present(&stale) {
            let _ = fs::rename(&units, &staging);
            let _ = fs::rename(&stale, &units);
            let _ = remove_dir_if_present(&staging);
            return Err(error);
        }
        Ok(())
    }

    pub fn manifest(&self) -> Option<ArtifactManifest> {
        let text = fs::read_to_string(self.cache_root.join("manifest.json")).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn is_manifest_current(&self) -> bool {
        self.manifest()
            .map(|m| m.grammar_rev == grammar_revision() && m.schema_version == ARTIFACT_SCHEMA_VERSION)
            .unwrap_or(false)
    }

    pub fn unit_paths(&self, content_fingerprint: &str) -> UnitArtifactPaths {
        let unit_dir = self.cache_root.join("units").join(content_fingerprint);
        UnitArtifactPaths { meta: unit_dir.join("meta.json"), ast: unit_dir.join("ast.bin"), unit_dir }
    }

    pub fn read_ast(&self, content_fingerprint: &str) -> Option<AstUnitSnapshot> {
        if !self.is_manifest_current() {
            return None;
        }
        let paths = self.unit_paths(content_fingerprint);
        let bytes = fs::read(paths.ast).ok()?;
        let snapshot = decode_ast(&bytes).ok()?;
        if snapshot.schema_version != ARTIFACT_SCHEMA_VERSION || snapshot.meta.grammar_rev != grammar_revision() {
            return None;
        }
        Some(snapshot)
    }

    pub fn write_unit(&self, ast: &AstUnitSnapshot) -> std::io::Result<()> {
        let _guard = ARTIFACT_STORE_WRITE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.ensure_dirs_locked()?;
        let fp = &ast.meta.content_fingerprint;
        let paths = self.unit_paths(fp);
        fs::create_dir_all(&paths.unit_dir)?;
        write_atomically(&paths.meta, serde_json::to_string_pretty(&ast.meta)?.as_bytes())?;
        write_atomically(&paths.ast, &encode_ast(ast).map_err(io_err)?)?;
        let legacy_hir = paths.unit_dir.join("hir.bin");
        if legacy_hir.exists() {
            fs::remove_file(legacy_hir)?;
        }
        Ok(())
    }

    /// Recompute `persisted_units` from disk (call once per assembly batch, not per unit).
    pub fn refresh_manifest(&self) -> std::io::Result<()> {
        let _guard = ARTIFACT_STORE_WRITE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        self.ensure_dirs_locked()?;
        self.update_manifest_count_locked()
    }

    fn update_manifest_count_locked(&self) -> std::io::Result<()> {
        let count =
            fs::read_dir(self.cache_root.join("units"))?.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).count();
        let manifest_path = self.cache_root.join("manifest.json");
        let mut manifest: ArtifactManifest = if manifest_path.is_file() {
            serde_json::from_str(&fs::read_to_string(&manifest_path)?).unwrap_or(ArtifactManifest {
                grammar_rev: grammar_revision().to_string(),
                compiler_version: env!("CARGO_PKG_VERSION").to_string(),
                schema_version: ARTIFACT_SCHEMA_VERSION,
                persisted_units: 0,
            })
        } else {
            ArtifactManifest {
                grammar_rev: grammar_revision().to_string(),
                compiler_version: env!("CARGO_PKG_VERSION").to_string(),
                schema_version: ARTIFACT_SCHEMA_VERSION,
                persisted_units: 0,
            }
        };
        manifest.grammar_rev = grammar_revision().to_string();
        manifest.schema_version = ARTIFACT_SCHEMA_VERSION;
        manifest.persisted_units = count;
        write_atomically(&manifest_path, serde_json::to_string_pretty(&manifest)?.as_bytes())?;
        Ok(())
    }
}

fn remove_dir_if_present(path: &Path) -> std::io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

pub fn cache_root_for_project(project_root: &Path) -> PathBuf {
    project_root.join("obj").join("beskid").join("cache").join("salsa")
}

fn io_err(err: postcard::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string())
}
