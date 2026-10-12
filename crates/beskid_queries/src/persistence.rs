//! On-disk persistence adapter for Salsa session state under `obj/beskid/cache/salsa/`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Create the snapshot directory. The snapshot file itself is the only persisted state: file
/// inputs travel inside it and are replaced from disk by the prepare spine when they differ.
pub fn ensure_salsa_dir(root: &Path) -> std::io::Result<()> {
    fs::create_dir_all(root)
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

/// On-disk snapshot envelope. `version`, `compiler` and `corelib` gate reload; `db` is the
/// serialized Salsa DB and `digest` its SHA-256. The writer streams `db` first and appends the
/// identity fields after it, so the payload is never held in memory twice while saving.
#[cfg(feature = "persistence")]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotEnvelope<'a> {
    version: String,
    compiler: String,
    corelib: String,
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
    format!("beskid-queries:{}:grammar:{}:snapshot:4", env!("CARGO_PKG_VERSION"), beskid_pipeline::GRAMMAR_REVISION,)
}

/// Identity of the Corelib every snapshot unit may have been materialized from: the sealed
/// bundle fingerprint of the resolved installed Corelib, or the root path of an unmanaged
/// (developer checkout) Corelib, whose text inputs the prepare spine re-reads from disk. A
/// snapshot taken against another Corelib is rejected instead of being diffed unit by unit.
#[cfg(feature = "persistence")]
fn corelib_identity() -> String {
    let Ok(root) = beskid_abi::runtime_kit::installed_corelib_root() else {
        return "unresolved".into();
    };
    match fs::read_to_string(root.join(beskid_abi::corelib_bundle::CORELIB_BUNDLE_FINGERPRINT_FILE)) {
        Ok(fingerprint) => format!("bundle:{}", fingerprint.trim()),
        Err(_) => format!("unmanaged:{}", root.canonicalize().unwrap_or(root).display()),
    }
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
    let payload: Payload<'_> = deserialize_snapshot(payload.as_bytes()).map_err(|error| error.to_string())?;
    let _ = payload.runtime;
    let mut ingredients = std::collections::HashSet::new();
    let mut allocated = std::collections::HashSet::new();
    let mut memos_started = false;
    for (ingredient, entries) in payload.ingredients.0 {
        let ingredient: u32 = ingredient.parse().map_err(|_| "invalid ingredient index")?;
        if ingredient > i32::MAX as u32 || !ingredients.insert(ingredient) {
            return Err("duplicate or invalid ingredient index".into());
        }
        let entries: OrderedObject<'_> =
            deserialize_snapshot(entries.get().as_bytes()).map_err(|error| error.to_string())?;
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

/// Limits apply to the complete encoded envelope, before allocating Salsa state, and to every
/// snapshot the writer produces. The incremental scanner is a resource preflight, not a
/// replacement JSON parser. The byte bound admits a whole-Corelib session (all ~1300 units with
/// their persisted memos) with headroom while still refusing unbounded input.
#[cfg(feature = "persistence")]
#[derive(Clone, Copy)]
struct SnapshotLimits {
    bytes: usize,
    depth: usize,
    containers: usize,
    string_bytes: usize,
}

#[cfg(feature = "persistence")]
impl Default for SnapshotLimits {
    fn default() -> Self {
        Self { bytes: 1024 * 1024 * 1024, depth: 256, containers: 64 * 1024 * 1024, string_bytes: 4 * 1024 * 1024 }
    }
}

#[cfg(feature = "persistence")]
const SNAPSHOT_STACK_BYTES: usize = 16 * 1024 * 1024;

/// Streaming JSON resource scanner: tracks string and container nesting across chunk boundaries.
#[cfg(feature = "persistence")]
struct LimitScanner {
    limits: SnapshotLimits,
    stack: Vec<u8>,
    total: usize,
    containers: usize,
    quoted: bool,
    escaped: bool,
    string_bytes: usize,
}

#[cfg(feature = "persistence")]
impl LimitScanner {
    fn new(limits: SnapshotLimits) -> Self {
        Self { limits, stack: Vec::new(), total: 0, containers: 0, quoted: false, escaped: false, string_bytes: 0 }
    }

    fn feed(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.total = self.total.saturating_add(bytes.len());
        if self.total > self.limits.bytes {
            return Err("snapshot byte limit exceeded".into());
        }
        for &byte in bytes {
            if self.quoted {
                if byte == b'"' && !self.escaped {
                    self.quoted = false;
                    continue;
                }
                self.string_bytes += 1;
                if self.string_bytes > self.limits.string_bytes {
                    return Err("snapshot string byte limit exceeded".into());
                }
                if self.escaped {
                    self.escaped = false;
                } else if byte == b'\\' {
                    self.escaped = true;
                }
                continue;
            }
            match byte {
                b'"' => {
                    self.quoted = true;
                    self.string_bytes = 0;
                }
                b'{' | b'[' => {
                    if self.stack.len() >= self.limits.depth {
                        return Err("snapshot container depth limit exceeded".into());
                    }
                    self.containers += 1;
                    if self.containers > self.limits.containers {
                        return Err("snapshot container count limit exceeded".into());
                    }
                    self.stack.push(byte);
                }
                b'}' | b']' => {
                    let expected = if byte == b'}' { b'{' } else { b'[' };
                    if self.stack.pop() != Some(expected) {
                        return Err("snapshot has mismatched JSON delimiters".into());
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn finish(&self) -> Result<(), String> {
        if self.quoted || !self.stack.is_empty() {
            return Err("snapshot has incomplete JSON structure".into());
        }
        Ok(())
    }
}

#[cfg(feature = "persistence")]
fn validate_snapshot_limits(bytes: &[u8], limits: SnapshotLimits) -> Result<(), String> {
    let mut scanner = LimitScanner::new(limits);
    scanner.feed(bytes)?;
    scanner.finish()
}

#[cfg(feature = "persistence")]
fn read_bounded_snapshot(root: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let file = fs::File::open(snapshot_path(root))?;
    let metadata = file.metadata()?;
    let limits = SnapshotLimits::default();
    if !metadata.is_file() || metadata.len() > limits.bytes as u64 {
        return Err(std::io::Error::other("snapshot is not a bounded regular file"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    // A concurrently growing file cannot bypass the byte limit after metadata.
    file.take(limits.bytes as u64 + 1).read_to_end(&mut bytes)?;
    validate_snapshot_limits(&bytes, limits).map_err(std::io::Error::other)?;
    Ok(bytes)
}

/// Call only for preflighted input on the bounded snapshot stack. RawValue keeps
/// borrowed strings and Salsa ingredient order intact; no owned Value tree.
#[cfg(feature = "persistence")]
fn deserialize_snapshot<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    deserializer.disable_recursion_limit();
    let value = T::deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
}

/// Writer that bounds, scans and optionally hashes everything it forwards. A limit violation
/// aborts serialization at the first offending byte instead of after the payload is complete.
#[cfg(feature = "persistence")]
struct ScanningWriter<W: std::io::Write> {
    inner: W,
    scanner: LimitScanner,
    digest: Option<sha2::Sha256>,
}

#[cfg(feature = "persistence")]
impl<W: std::io::Write> std::io::Write for ScanningWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.scanner.feed(bytes).map_err(std::io::Error::other)?;
        if let Some(digest) = &mut self.digest {
            sha2::Digest::update(digest, bytes);
        }
        self.inner.write_all(bytes)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Serialize the salsa DB snapshot to `root/db.json`.
///
/// Only ingredients marked with `#[salsa::*(persist)]` are emitted; the rest are
/// skipped by salsa's `should_serialize` filter. Returns `Ok(())` when persistence
/// is disabled so callers need not feature-gate their call sites.
#[cfg(feature = "persistence")]
pub fn save_db_snapshot(db: &mut crate::db::BeskidDatabase, root: &Path) -> std::io::Result<()> {
    stacker::grow(SNAPSHOT_STACK_BYTES, || save_db_snapshot_on_stack(db, root))
}

/// Stream the payload straight into a temporary sibling file while scanning and hashing it, then
/// append the identity fields and publish the file with one rename. Any failure removes the
/// temporary file and leaves the previous snapshot untouched.
#[cfg(feature = "persistence")]
fn save_db_snapshot_on_stack(db: &mut crate::db::BeskidDatabase, root: &Path) -> std::io::Result<()> {
    use std::io::Write;
    ensure_salsa_dir(root)?;
    let path = snapshot_path(root);
    let tmp = path.with_extension("json.tmp");
    let mut write = || -> std::io::Result<()> {
        let file = std::io::BufWriter::with_capacity(1 << 20, fs::File::create(&tmp)?);
        let mut writer =
            ScanningWriter { inner: file, scanner: LimitScanner::new(SnapshotLimits::default()), digest: None };
        writer.write_all(b"{\"db\":")?;
        writer.digest = Some(<sha2::Sha256 as sha2::Digest>::new());
        // Salsa deliberately serializes structs before memos. Never pass through
        // Value: its map ordering and owned-string deserializer break that protocol.
        {
            // Batch Serde's many small writes so the scanner and digest see large chunks.
            let mut buffered = std::io::BufWriter::with_capacity(1 << 20, &mut writer);
            serde_json::to_writer(&mut buffered, &<dyn salsa::Database>::as_serialize(db))
                .map_err(|err| std::io::Error::other(err.to_string()))?;
            buffered.flush()?;
        }
        let digest = writer.digest.take().expect("payload digest");
        let identity = serde_json::json!({
            "version": snapshot_format_version(),
            "compiler": compiler_fingerprint()?,
            "corelib": corelib_identity(),
            "digest": format!("{:x}", sha2::Digest::finalize(digest)),
        });
        let identity = serde_json::to_string(&identity).map_err(std::io::Error::other)?;
        writer.write_all(b",")?;
        writer.write_all(&identity.as_bytes()[1..])?;
        writer.scanner.finish().map_err(std::io::Error::other)?;
        let file = writer.inner.into_inner().map_err(|error| error.into_error())?;
        file.sync_all()?;
        Ok(())
    };
    let started = std::time::Instant::now();
    if let Err(error) = write() {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    fs::rename(&tmp, &path)?;
    log::info!(
        "beskid salsa snapshot saved: {} bytes in {} ms",
        fs::metadata(&path).map(|metadata| metadata.len()).unwrap_or(0),
        started.elapsed().as_millis()
    );
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
    let started = std::time::Instant::now();
    let bytes = match read_bounded_snapshot(root) {
        Ok(bytes) => bytes,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!("beskid salsa snapshot rejected: input preflight: {error}");
            }
            return false;
        }
    };
    // The depth preflight bounds this stack's work. Keep partial candidate Drop,
    // registry rehydration and replacement on it as well as deserialization.
    let loaded = stacker::grow(SNAPSHOT_STACK_BYTES, || load_db_snapshot_on_stack(db, &bytes));
    if loaded {
        log::info!("beskid salsa snapshot loaded: {} bytes in {} ms", bytes.len(), started.elapsed().as_millis());
    }
    loaded
}

#[cfg(feature = "persistence")]
fn load_db_snapshot_on_stack(db: &mut crate::db::BeskidDatabase, bytes: &[u8]) -> bool {
    let envelope: SnapshotEnvelope = match deserialize_snapshot(bytes) {
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
    if envelope.corelib != corelib_identity() {
        log::info!("beskid salsa snapshot rejected: taken against a different Corelib");
        return false;
    }
    if let Err(error) = validate_payload_order(envelope.db.get()) {
        log::warn!("beskid salsa snapshot rejected: invalid payload order: {error}");
        return false;
    }
    let mut candidate = crate::db::BeskidDatabase::uninitialized(db.persistence_root().map(Path::to_path_buf));
    let mut deserializer = serde_json::Deserializer::from_str(envelope.db.get());
    deserializer.disable_recursion_limit();
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

#[cfg(all(test, feature = "persistence"))]
mod snapshot_limit_tests {
    use super::{LimitScanner, SnapshotLimits, validate_snapshot_limits};

    #[test]
    fn chunked_scanning_matches_whole_input_scanning() {
        let limits = SnapshotLimits { bytes: 100, depth: 3, containers: 4, string_bytes: 8 };
        for input in [
            br#"{"a":["x\"[",{"b":"]"}]}"#.as_slice(),
            br#"{"a":[[[[]]]]}"#.as_slice(),
            br#"{"a":"123456789"}"#.as_slice(),
            br#"{"a":["#.as_slice(),
        ] {
            let whole = validate_snapshot_limits(input, limits);
            let mut scanner = LimitScanner::new(limits);
            let chunked = input
                .iter()
                .try_for_each(|byte| scanner.feed(std::slice::from_ref(byte)))
                .and_then(|()| scanner.finish());
            assert_eq!(whole, chunked, "{}", String::from_utf8_lossy(input));
        }
    }

    #[test]
    fn exact_limits_accept_and_each_resource_overrun_rejects() {
        let bytes = br#"{"nested":[{"label":"[{}]"}]}"#;
        let limits = SnapshotLimits { bytes: bytes.len(), depth: 3, containers: 3, string_bytes: 6 };
        assert!(validate_snapshot_limits(bytes, limits).is_ok());
        for (policy, metric) in [
            (SnapshotLimits { bytes: bytes.len() - 1, ..limits }, "byte limit"),
            (SnapshotLimits { depth: 2, ..limits }, "depth limit"),
            (SnapshotLimits { containers: 2, ..limits }, "count limit"),
            (SnapshotLimits { string_bytes: 5, ..limits }, "string byte limit"),
        ] {
            assert!(validate_snapshot_limits(bytes, policy).unwrap_err().contains(metric));
        }
    }

    #[test]
    fn quoted_delimiters_and_escaped_quotes_do_not_change_depth() {
        let bytes = br#"{"text":"\"[{}]\\\""}"#;
        let limits = SnapshotLimits { bytes: 100, depth: 1, containers: 1, string_bytes: 100 };
        assert!(validate_snapshot_limits(bytes, limits).is_ok());
        for malformed in [b"{]".as_slice(), b"[", b"\"unterminated", b"[\"trailing\\"] {
            assert!(validate_snapshot_limits(malformed, limits).is_err());
        }
    }
}
