//! On-disk persistence adapter for Salsa session state under `obj/beskid/cache/salsa/`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalsaPersistenceManifest {
    pub grammar_rev: String,
    pub compiler_version: String,
    pub persisted_files: usize,
}

pub fn ensure_salsa_dir(root: &Path) -> std::io::Result<()> {
    fs::create_dir_all(root)?;
    let manifest_path = root.join("manifest.json");
    if !manifest_path.is_file() {
        let manifest = SalsaPersistenceManifest {
            grammar_rev: env!("CARGO_PKG_VERSION").to_string(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            persisted_files: 0,
        };
        fs::write(manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    }
    fs::create_dir_all(root.join("files"))?;
    Ok(())
}

pub fn persist_file_text(root: &Path, path: &Path, text: &str) -> std::io::Result<()> {
    ensure_salsa_dir(root)?;
    let key = file_key(path);
    fs::write(root.join("files").join(format!("{key}.bd")), text)?;
    update_manifest_count(root)
}

pub fn load_persisted_file_text(root: &Path, path: &Path) -> Option<String> {
    let key = file_key(path);
    fs::read_to_string(root.join("files").join(format!("{key}.bd"))).ok()
}

fn file_key(path: &Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn update_manifest_count(root: &Path) -> std::io::Result<()> {
    let count = fs::read_dir(root.join("files"))?.filter_map(|e| e.ok()).count();
    let manifest_path = root.join("manifest.json");
    let mut manifest: SalsaPersistenceManifest = if manifest_path.is_file() {
        serde_json::from_str(&fs::read_to_string(&manifest_path)?).unwrap_or(SalsaPersistenceManifest {
            grammar_rev: env!("CARGO_PKG_VERSION").to_string(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            persisted_files: 0,
        })
    } else {
        SalsaPersistenceManifest {
            grammar_rev: env!("CARGO_PKG_VERSION").to_string(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            persisted_files: 0,
        }
    };
    manifest.persisted_files = count;
    fs::write(manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    Ok(())
}

pub fn load_manifest(root: &Path) -> Option<SalsaPersistenceManifest> {
    let text = fs::read_to_string(root.join("manifest.json")).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn cache_root_for_project(project_root: &Path) -> PathBuf {
    project_root.join("obj").join("beskid").join("cache").join("salsa")
}

// ---------------------------------------------------------------------------
// Salsa DB snapshot (cross-run incremental compilation).
//
// Salsa 0.26's `persistence` feature exposes `<dyn salsa::Database>::as_serialize`
// and `deserialize`, which snapshot the full ingredient state (inputs, interned
// structs, and tracked-fn memos marked with `#[persist]`) to any serde format.
// We wrap that snapshot in a versioned envelope under `obj/beskid/cache/salsa/`
// so a stale or grammar-incompatible snapshot is rejected closed rather than
// corrupting a fresh compile.
//
// When the `persistence` cargo feature is off, these are no-ops so callers
// (beskid_cli, beskid_lsp, beskid_aot) compile identically with or without the
// feature wired.
// ---------------------------------------------------------------------------

/// On-disk snapshot envelope. `version` gates reload; `db` is the serialized salsa DB.
#[cfg(feature = "persistence")]
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotEnvelope<'a> {
    version: String,
    compiler: String,
    digest: String,
    #[serde(borrow)]
    db: &'a serde_json::value::RawValue,
}

/// Version gate for the on-disk snapshot. Combines the `beskid_queries` crate
/// version (snapshot format/ingredient set) with `beskid_pipeline::GRAMMAR_REVISION`
/// (the grammar revision baked into every unit's invalidation key) so a grammar
/// or compiler bump rejects a stale snapshot rather than corrupting a fresh
/// compile. Computed at runtime because `GRAMMAR_REVISION` is a `const &str`, not
/// a literal usable by `concat!`.
#[cfg(feature = "persistence")]
fn snapshot_format_version() -> String {
    format!("beskid-queries:{}:grammar:{}:snapshot:2", env!("CARGO_PKG_VERSION"), beskid_pipeline::GRAMMAR_REVISION,)
}

/// Exact executable identity includes the ingredient schema, compiler semantics,
/// feature set and embedded ABI. Package versions alone do not change on rebuild.
#[cfg(feature = "persistence")]
fn compiler_fingerprint() -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    static FINGERPRINT: std::sync::OnceLock<Result<String, String>> = std::sync::OnceLock::new();
    FINGERPRINT
        .get_or_init(|| {
            let compute = || -> std::io::Result<String> {
                let mut executable = fs::File::open(std::env::current_exe()?)?;
                let mut digest = Sha256::new();
                let mut buffer = [0_u8; 65536];
                loop {
                    let count = executable.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    digest.update(&buffer[..count]);
                }
                Ok(format!("{:x}", digest.finalize()))
            };
            compute().map_err(|error| error.to_string())
        })
        .clone()
        .map_err(std::io::Error::other)
}

#[cfg(feature = "persistence")]
fn payload_digest(payload: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(payload.as_bytes()))
}

/// Inspect Salsa's serialized ordering without allocating any database entries.
/// Borrowed raw values keep both object order and string lifetimes intact.
#[cfg(feature = "persistence")]
struct OrderedObject<'a>(Vec<(&'a str, &'a serde_json::value::RawValue)>);

#[cfg(feature = "persistence")]
impl<'de: 'a, 'a> Deserialize<'de> for OrderedObject<'a> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = OrderedObject<'de>;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an ordered Salsa object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(OrderedObject(entries))
            }
        }
        deserializer.deserialize_map(Visitor).map(|object| OrderedObject(object.0))
    }
}

#[cfg(feature = "persistence")]
fn validate_payload_order(payload: &str) -> Result<(), String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Payload<'a> {
        #[serde(borrow)]
        runtime: &'a serde_json::value::RawValue,
        #[serde(borrow)]
        ingredients: OrderedObject<'a>,
    }
    let payload: Payload<'_> = serde_json::from_str(payload).map_err(|error| error.to_string())?;
    let _ = payload.runtime;
    let mut ingredients = std::collections::HashSet::new();
    let mut allocated = std::collections::HashSet::new();
    let mut memos_started = false;
    for (ingredient, entries) in payload.ingredients.0 {
        let ingredient: u32 = ingredient.parse().map_err(|_| "invalid ingredient index")?;
        if ingredient > i32::MAX as u32 || !ingredients.insert(ingredient) {
            return Err("duplicate or invalid ingredient index".into());
        }
        let entries: OrderedObject<'_> = serde_json::from_str(entries.get()).map_err(|error| error.to_string())?;
        let mut keys = std::collections::HashSet::new();
        let mut previous_slot = 0;
        for (key, _) in entries.0 {
            if !keys.insert(key) {
                return Err("duplicate ingredient entry".into());
            }
            if let Some((owner, slot)) = key.split_once(':') {
                let owner: u32 = owner.parse().map_err(|_| "invalid memo owner")?;
                let slot: u64 = slot.parse().map_err(|_| "invalid memo slot")?;
                if !allocated.contains(&(owner, slot)) {
                    return Err("memo precedes or references an absent struct entry".into());
                }
                memos_started = true;
            } else {
                if memos_started {
                    return Err("struct entry follows a memo ingredient".into());
                }
                let slot: u64 = key.parse().map_err(|_| "invalid struct slot")?;
                let index = slot as u32;
                if index == 0 || index <= previous_slot {
                    return Err("struct entries are not in allocation order".into());
                }
                previous_slot = index;
                allocated.insert((ingredient, slot));
            }
        }
    }
    Ok(())
}

#[cfg(feature = "persistence")]
fn snapshot_path(root: &Path) -> PathBuf {
    root.join("db.json")
}

/// Serialize the salsa DB snapshot to `root/db.json`.
///
/// Only ingredients marked with `#[salsa::*(persist)]` are emitted; the rest are
/// skipped by salsa's `should_serialize` filter. Returns `Ok(())` when persistence
/// is disabled so callers need not feature-gate their call sites.
#[cfg(feature = "persistence")]
pub fn save_db_snapshot(db: &mut crate::db::BeskidDatabase, root: &Path) -> std::io::Result<()> {
    ensure_salsa_dir(root)?;
    // Salsa deliberately serializes structs before memos. Never pass through
    // Value: its map ordering and owned-string deserializer break that protocol.
    let serialized = serde_json::to_string(&<dyn salsa::Database>::as_serialize(db))
        .map_err(|err| std::io::Error::other(err.to_string()))?;
    let raw = serde_json::value::RawValue::from_string(serialized).map_err(std::io::Error::other)?;
    let envelope = SnapshotEnvelope {
        version: snapshot_format_version(),
        compiler: compiler_fingerprint()?,
        digest: payload_digest(raw.get()),
        db: &raw,
    };
    let bytes = serde_json::to_vec_pretty(&envelope).map_err(|err| std::io::Error::other(err.to_string()))?;
    let path = snapshot_path(root);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &bytes)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Load the salsa DB snapshot from `root/db.json` into `db`, rehydrating the
/// file/project registries from the persisted input entries.
///
/// Returns `true` when a snapshot was loaded and applied. Returns `false`
/// (without touching `db`) when persistence is disabled, the snapshot is absent,
/// or validation/deserialization rejects it. Rejections are logged, and Salsa
/// deserialization mutates only a disposable candidate until fully successful.
#[cfg(feature = "persistence")]
pub fn load_db_snapshot(db: &mut crate::db::BeskidDatabase, root: &Path) -> bool {
    let Ok(bytes) = fs::read(snapshot_path(root)) else { return false };
    let envelope: SnapshotEnvelope = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(err) => {
            log::warn!("beskid salsa snapshot rejected: parse error: {err}");
            return false;
        }
    };
    let expected = snapshot_format_version();
    if envelope.version != expected {
        log::info!(
            "beskid salsa snapshot rejected: version mismatch (on-disk `{}`, expected `{}`)",
            envelope.version,
            expected
        );
        return false;
    }
    let compiler = match compiler_fingerprint() {
        Ok(value) => value,
        Err(error) => {
            log::warn!("beskid salsa snapshot rejected: compiler identity unavailable: {error}");
            return false;
        }
    };
    if envelope.compiler != compiler || envelope.digest != payload_digest(envelope.db.get()) {
        log::warn!("beskid salsa snapshot rejected: compiler/schema identity or payload integrity mismatch");
        return false;
    }
    if let Err(error) = validate_payload_order(envelope.db.get()) {
        log::warn!("beskid salsa snapshot rejected: invalid payload order: {error}");
        return false;
    }
    let mut candidate = crate::db::BeskidDatabase::uninitialized(db.persistence_root().map(Path::to_path_buf));
    let mut deserializer = serde_json::Deserializer::from_str(envelope.db.get());
    if let Err(err) = <dyn salsa::Database>::deserialize(&mut candidate, &mut deserializer) {
        log::warn!("beskid salsa snapshot rejected: deserialize error: {err}");
        return false;
    }
    if let Err(err) = deserializer.end() {
        log::warn!("beskid salsa snapshot rejected: trailing payload: {err}");
        return false;
    }
    let restored_generation = rehydrate_registries(&mut candidate);
    candidate.grammar_revision();
    // Restored dependency memos may be verified through erased Salsa access
    // before their generated query accessor runs. Publish the existing typed
    // compiler view, not a query-specific warm-up list or an inferred cast.
    <dyn crate::db::Db as crate::db::Db>::zalsa_register_downcaster(&candidate);
    if let Some(generation) = restored_generation {
        generation.resume_after();
    }
    crate::db::replace_compilation_database(db, candidate);
    true
}

/// Re-populate the plain-Rust file/project registries from the persisted salsa
/// input entries so subsequent `set_file_text` / `ensure_file_text` calls reuse
/// the existing `FileText` IDs instead of allocating duplicates.
#[cfg(feature = "persistence")]
fn rehydrate_registries(db: &mut crate::db::BeskidDatabase) -> Option<crate::SyntaxGenerationId> {
    use crate::db::Db;
    use salsa::plumbing::ZalsaDatabase;

    // Collect first (borrowing db immutably), then mutate the registries.
    let mut files: Vec<(std::path::PathBuf, crate::inputs::FileText)> = Vec::new();
    for entry in crate::inputs::FileText::ingredient(db).entries(db.zalsa()) {
        let file = entry.as_struct();
        let path = file.path(db).clone();
        files.push((path, file));
    }

    let mut projects: Vec<((std::path::PathBuf, std::path::PathBuf, String), crate::inputs::ProjectSession)> =
        Vec::new();
    for entry in crate::inputs::ProjectSession::ingredient(db).entries(db.zalsa()) {
        let session = entry.as_struct();
        let key = (session.project_root(db).clone(), session.entry_path(db).clone(), session.target_name(db).clone());
        projects.push((key, session));
    }

    let mut syntax_units: Vec<(crate::semantic_contract::SourceUnitId, crate::semantic_contract::SyntaxUnitInput)> =
        Vec::new();
    let mut restored_generation = None;
    for entry in crate::semantic_contract::SyntaxUnitInput::ingredient(db).entries(db.zalsa()) {
        let input = entry.as_struct();
        let unit = input.unit(db);
        restored_generation = restored_generation.max(Some(input.generation(db)));
        syntax_units.push((unit, input));
    }

    {
        let mut registry = db.file_registry().lock().expect("file registry");
        for (path, file) in files {
            registry.entry(path).or_insert(file);
        }
    }
    let project_registry = db.project_registry();
    {
        let mut registry = project_registry.lock().expect("project registry");
        for (key, session) in projects {
            registry.entry(key).or_insert(session);
        }
    }
    {
        let mut registry = db.syntax_unit_registry().lock().expect("syntax unit registry");
        for (unit, input) in syntax_units {
            registry.entry(unit).or_insert(input);
        }
    }
    restored_generation
}

// No-op stubs when the `persistence` feature is disabled. Keeps call sites in
// beskid_cli / beskid_lsp / beskid_aot uniform regardless of feature wiring.
#[cfg(not(feature = "persistence"))]
pub fn save_db_snapshot(_db: &mut crate::db::BeskidDatabase, _root: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(feature = "persistence"))]
pub fn load_db_snapshot(_db: &mut crate::db::BeskidDatabase, _root: &Path) -> bool {
    false
}

/// Persist the current DB snapshot if the database has a persistence root.
pub fn persist_session_snapshot(db: &mut crate::db::BeskidDatabase) {
    let Some(root) = db.persistence_root().map(std::path::Path::to_path_buf) else { return };
    if let Err(err) = save_db_snapshot(db, &root) {
        log::warn!("beskid salsa snapshot save failed: {err}");
    }
}
