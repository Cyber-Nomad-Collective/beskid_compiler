//! Sole executable Mod descriptor schema and closed evidence inventory authority.
use super::types::ContractRegistration;
use anyhow::{Context, Result, bail};
use beskid_abi::{
    abi_v5::TargetMetadata,
    runtime_kit::{
        BuildProfile, GLUE_PROVIDER_MANIFEST_V1, GlueProviderToolV1, exact_kit_metadata_path, host_runtime_target,
        resolve_glue_shared_provider,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

pub const NATIVE_MOD_DESCRIPTOR_SCHEMA: u32 = 2;
pub const NATIVE_MOD_HOST_ABI: u32 = 2;
pub const NATIVE_MOD_DESCRIPTOR_FILE: &str = "mod.descriptor.json";
/// Exact build-tool provenance roles, in producer order (`beskid_aot::build_mod_artifact`).
pub const NATIVE_MOD_BUILD_TOOL_ROLES: [&str; 3] = ["compiler", "adapter-compiler", "linker"];
/// Role of the Mod-host compiler executable in [`ModArtifactDescriptor::build_tools`]. The same
/// executable serves `beskid dev native-mod-worker`, so its digest is the driver identity a cached
/// or shipped artifact must match.
pub const NATIVE_MOD_COMPILER_TOOL_ROLE: &str = "compiler";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeModRuntimeBinding {
    pub profile: BuildProfile,
    pub abi_metadata_sha256: String,
    pub provider_manifest_sha256: String,
    pub runtime_source_sha256: String,
    pub issuer_source_sha256: String,
    pub shared_library_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeModCallableIdentity {
    pub source_file: String,
    pub source_sha256: String,
    pub generation: u64,
    pub node: u32,
    pub internal_symbol: String,
    pub link_symbol: String,
    pub signature_sha256: String,
    pub contract_family: String,
    pub sdk_request_layout_sha256: String,
    pub sdk_result_layout_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModArtifactDescriptor {
    pub schema_version: u32,
    pub artifact_kind: String,
    pub host_abi_version: u32,
    pub package_id: String,
    pub package_version: Option<String>,
    pub mod_source_hash: String,
    pub lock_hash: String,
    pub sdk_schema_sha256: String,
    pub sdk_source_sha256: String,
    pub target_triple: String,
    pub compiler_version: String,
    pub executable_file: String,
    pub discriminator_symbol: String,
    pub registrations: Vec<ContractRegistration>,
    pub callables: BTreeMap<String, NativeModCallableIdentity>,
    pub files: BTreeMap<String, String>,
    pub source_files: BTreeMap<String, String>,
    pub sdk_files: BTreeMap<String, String>,
    /// Source identity of every dependency project of the Mod (dependency name to
    /// [`native_mod_dependency_identity`]). A change in a dependency invalidates the artifact,
    /// as a dependency fingerprint change does for a Cargo unit.
    pub dependency_sources: BTreeMap<String, String>,
    pub build_tools: Vec<GlueProviderToolV1>,
    pub runtime: NativeModRuntimeBinding,
    #[serde(skip)]
    pub artifact_key: String,
    #[serde(skip)]
    pub artifact_dir: PathBuf,
}
impl ModArtifactDescriptor {
    pub fn executable_path(&self) -> PathBuf {
        self.artifact_dir.join(&self.executable_file)
    }
    pub fn sidecar_path(&self) -> PathBuf {
        self.artifact_dir.join(NATIVE_MOD_DESCRIPTOR_FILE)
    }
    pub fn validate(&self, prefix: &Path) -> Result<()> {
        if self.schema_version != NATIVE_MOD_DESCRIPTOR_SCHEMA
            || self.host_abi_version != NATIVE_MOD_HOST_ABI
            || self.artifact_kind != "executable-shared-library"
        {
            bail!("required executable Mod descriptor V2/host ABI V2; rebuild the Mod");
        }
        let target = TargetMetadata::for_triple(&self.target_triple)
            .map_err(|_| anyhow::anyhow!("unsupported native Mod target"))?;
        if target != host_runtime_target().map_err(|error| anyhow::anyhow!("native Mod host target: {error}"))? {
            bail!("native Mod target differs from current host");
        }
        if self.package_id.is_empty()
            || self.package_id.contains(['/', '\\'])
            || self.package_id == "."
            || self.package_id == ".."
        {
            bail!("invalid Mod package identity");
        }
        semver::Version::parse(&self.compiler_version).context("invalid Mod compiler version")?;
        if let Some(version) = &self.package_version {
            semver::Version::parse(version).context("invalid Mod package version")?;
        }
        for digest in [&self.mod_source_hash, &self.lock_hash, &self.sdk_schema_sha256, &self.sdk_source_sha256] {
            valid_digest(digest)?;
        }
        if !symbol(&self.discriminator_symbol)
            || self.registrations.is_empty()
            || self.registrations.len() > 4096
            || self.callables.len() != self.registrations.len()
        {
            bail!("invalid native Mod dispatcher identities");
        }
        let mut entries = std::collections::BTreeSet::new();
        for registration in &self.registrations {
            if !symbol(&registration.entry_symbol) || !entries.insert(&registration.entry_symbol) {
                bail!("ambiguous native Mod entrypoint");
            }
            let callable =
                self.callables.get(&registration.entry_symbol).context("missing canonical Mod callable identity")?;
            if callable.generation == 0
                || callable.contract_family != registration.contract_id
                || !symbol(&callable.link_symbol)
                || callable.internal_symbol.is_empty()
            {
                bail!("invalid canonical Mod callable authority");
            }
            for digest in [
                &callable.source_sha256,
                &callable.signature_sha256,
                &callable.sdk_request_layout_sha256,
                &callable.sdk_result_layout_sha256,
            ] {
                valid_digest(digest)?;
            }
            if self.source_files.get(&callable.source_file) != Some(&callable.source_sha256) {
                bail!("Mod callable source identity differs from source closure");
            }
        }
        normal_path(&self.executable_file)?;
        if !self.files.contains_key(&self.executable_file) {
            bail!("native Mod executable absent from inventory");
        }
        let binary = std::fs::read(self.executable_path())?;
        use object::Object;
        let file = object::File::parse(binary.as_slice()).context("Mod payload is not an executable native library")?;
        if file.kind() != object::ObjectKind::Dynamic {
            bail!("Mod payload is not a linked dynamic library");
        }
        let architecture = if self.target_triple.starts_with("aarch64") {
            object::Architecture::Aarch64
        } else {
            object::Architecture::X86_64
        };
        if file.architecture() != architecture || !file.is_64() {
            bail!("Mod binary target architecture differs from descriptor");
        }
        let expected = self
            .registrations
            .iter()
            .map(|entry| entry.entry_symbol.as_bytes())
            .chain(std::iter::once(self.discriminator_symbol.as_bytes()))
            .collect::<std::collections::BTreeSet<_>>();
        let export_rows = file.exports()?;
        let exported = export_rows.iter().map(|entry| entry.name()).collect::<std::collections::BTreeSet<_>>();
        // Object APIs strip Mach-O's ABI underscore in exports where available; allow that
        // platform prefix only as target-defined symbol syntax, never a guessed entry alias.
        for name in expected {
            if !exported.contains(name)
                && !(self.target_triple.contains("apple")
                    && exported.iter().any(|actual| actual.strip_prefix(b"_") == Some(name)))
            {
                bail!("required Mod C ABI dispatcher is not exported");
            }
        }
        let actual = mod_artifact_inventory(&self.artifact_dir)?;
        if actual != self.files {
            bail!("native Mod executable inventory is missing, changed or has unlisted files");
        }
        validate_subset(&self.files, &self.source_files, "sources/")?;
        validate_subset(&self.files, &self.sdk_files, "sdk/")?;
        if self.sdk_files.get("sdk/schema.json") != Some(&self.sdk_schema_sha256) {
            bail!("native Mod SDK schema identity differs from staged canonical schema evidence");
        }
        let expected_lock = self
            .source_files
            .get("sources/Project.lock")
            .cloned()
            .unwrap_or_else(|| format!("{:x}", Sha256::digest([])));
        if expected_lock != self.lock_hash {
            bail!("native Mod lock identity differs from staged source evidence");
        }
        if native_mod_inventory_identity(&self.source_files) != self.mod_source_hash
            || native_mod_inventory_identity(&self.sdk_files) != self.sdk_source_sha256
        {
            bail!("native Mod source/SDK identity mismatch");
        }
        if self.build_tools.iter().map(|tool| tool.role.as_str()).collect::<Vec<_>>() != NATIVE_MOD_BUILD_TOOL_ROLES {
            bail!("native Mod requires exact compiler/adapter-compiler/linker provenance");
        }
        for (name, digest) in &self.dependency_sources {
            if name.is_empty() {
                bail!("native Mod dependency evidence has an empty dependency name");
            }
            valid_digest(digest)?;
        }
        for tool in &self.build_tools {
            valid_digest(&tool.executable_sha256)?;
            valid_digest(&tool.version_output_sha256)?;
            if self.files.get(&format!("tools/{}-executable", tool.role)) != Some(&tool.executable_sha256)
                || self.files.get(&format!("tools/{}-version", tool.role)) != Some(&tool.version_output_sha256)
            {
                bail!("native Mod tool identity differs from captured exact executable/version evidence");
            }
        }
        let provider = resolve_glue_shared_provider(prefix, &target, self.runtime.profile)
            .map_err(|error| anyhow::anyhow!("native Mod canonical shared provider: {error}"))?;
        let binding = native_mod_runtime_binding(prefix, &target, self.runtime.profile)?;
        if binding != self.runtime || provider.manifest.target != self.target_triple {
            bail!("native Mod runtime/provider/source/issuer identity mismatch");
        }
        Ok(())
    }
}
fn symbol(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}
fn valid_digest(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        bail!("invalid native Mod SHA256 identity");
    }
    Ok(())
}
fn normal_path(value: &str) -> Result<()> {
    if value.is_empty()
        || value.contains('\\')
        || value.split('/').any(|part| part.is_empty() || part == "." || part == "..")
        || Path::new(value).components().any(|part| !matches!(part, Component::Normal(_)))
    {
        bail!("native Mod evidence path must be normalized and relative");
    }
    Ok(())
}
fn validate_subset(files: &BTreeMap<String, String>, subset: &BTreeMap<String, String>, prefix: &str) -> Result<()> {
    if subset.is_empty() {
        bail!("native Mod source/SDK evidence closure is empty");
    }
    for (path, digest) in subset {
        normal_path(path)?;
        if !path.starts_with(prefix) || files.get(path) != Some(digest) {
            bail!("native Mod source/SDK evidence differs from inventory");
        }
    }
    Ok(())
}
pub fn native_mod_inventory_identity(files: &BTreeMap<String, String>) -> String {
    let mut hash = Sha256::new();
    hash.update(b"beskid-native-mod-evidence-v2\0");
    for (path, digest) in files {
        for bytes in [path.as_bytes(), digest.as_bytes()] {
            hash.update((bytes.len() as u64).to_be_bytes());
            hash.update(bytes);
        }
    }
    format!("{:x}", hash.finalize())
}
pub fn native_mod_file_sha256(path: &Path) -> Result<String> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > 512 * 1024 * 1024 {
        bail!("native Mod input is not a bounded regular file");
    }
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 65536];
    use std::io::Read;
    loop {
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn mod_artifact_inventory(root: &Path) -> Result<BTreeMap<String, String>> {
    if !std::fs::symlink_metadata(root)?.is_dir() {
        bail!("native Mod artifact root is not a directory");
    }
    fn walk(
        root: &Path,
        path: &Path,
        files: &mut BTreeMap<String, String>,
        depth: usize,
        total: &mut u64,
    ) -> Result<()> {
        if depth > 64 {
            bail!("native Mod inventory depth exceeded");
        }
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                walk(root, &path, files, depth + 1, total)?;
            } else if kind.is_file() {
                let name = path
                    .strip_prefix(root)?
                    .to_str()
                    .context("non UTF8 native Mod filename")?
                    .replace(std::path::MAIN_SEPARATOR, "/");
                normal_path(&name)?;
                if name == NATIVE_MOD_DESCRIPTOR_FILE {
                    continue;
                }
                *total = total.checked_add(entry.metadata()?.len()).context("native Mod inventory size overflow")?;
                if *total > 1024 * 1024 * 1024 || files.len() >= 4096 {
                    bail!("native Mod inventory budget exceeded");
                }
                files.insert(name, native_mod_file_sha256(&path)?);
            } else {
                bail!("native Mod evidence symlink/special file is forbidden");
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files, 0, &mut 0)?;
    Ok(files)
}
pub fn native_mod_runtime_binding(
    prefix: &Path,
    target: &TargetMetadata,
    profile: BuildProfile,
) -> Result<NativeModRuntimeBinding> {
    let provider = resolve_glue_shared_provider(prefix, target, profile)
        .map_err(|error| anyhow::anyhow!("canonical shared provider: {error}"))?;
    Ok(NativeModRuntimeBinding {
        profile,
        abi_metadata_sha256: native_mod_file_sha256(&exact_kit_metadata_path(prefix, target, profile))?,
        provider_manifest_sha256: native_mod_file_sha256(&provider.kit.root.join(GLUE_PROVIDER_MANIFEST_V1))?,
        runtime_source_sha256: provider.manifest.runtime_source_sha256,
        issuer_source_sha256: provider.manifest.issuer_source_sha256,
        shared_library_sha256: native_mod_file_sha256(&provider.shared_library)?,
    })
}
/// Content-addressed artifact key: SHA-256 of the serialized descriptor identity. The cache
/// directory `<root>/.beskid/obj/mods/<package>/<key>/<target>/` is named by this key, so a cache
/// entry whose directory name differs from its recomputed key is never reused.
pub fn native_mod_artifact_key(descriptor: &ModArtifactDescriptor) -> Result<String> {
    // `artifact_key` and `artifact_dir` are `serde(skip)`, so the key covers identity only.
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(descriptor)?)))
}

/// Runtime-kit profile that native Mods are built, cached and loaded for: `BESKID_RUNTIME_KIT_PROFILE`
/// when set, otherwise debug (the profile installed `beskid build` and `beskid run` select).
pub fn native_mod_runtime_profile() -> Result<BuildProfile> {
    match std::env::var("BESKID_RUNTIME_KIT_PROFILE") {
        Ok(value) => BuildProfile::parse(&value).map_err(|error| anyhow::anyhow!("{error}")),
        Err(std::env::VarError::NotPresent) => Ok(BuildProfile::Debug),
        Err(error) => bail!("invalid BESKID_RUNTIME_KIT_PROFILE: {error}"),
    }
}

/// Source identity of one dependency project: its manifest bytes and every `.bd` source under its
/// source root. Build/VCS directories and nested projects are outside the package, by the same rule
/// as package source proofs.
pub fn native_mod_dependency_identity(manifest_path: &Path, source_root: &Path) -> Result<String> {
    fn walk(root: &Path, at: &Path, files: &mut BTreeMap<String, String>, depth: usize) -> Result<()> {
        if depth > 64 {
            bail!("native Mod dependency source depth exceeded");
        }
        for entry in std::fs::read_dir(at).with_context(|| format!("read Mod dependency sources {}", at.display()))? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                bail!("native Mod dependency source symlink is forbidden: {}", path.display());
            }
            if kind.is_dir() {
                if crate::projects::is_outside_package(&path)? {
                    continue;
                }
                walk(root, &path, files, depth + 1)?;
            } else if kind.is_file() && path.extension().and_then(|extension| extension.to_str()) == Some("bd") {
                let name = path
                    .strip_prefix(root)?
                    .to_str()
                    .context("non UTF8 Mod dependency source")?
                    .replace(std::path::MAIN_SEPARATOR, "/");
                if files.len() >= 65536 {
                    bail!("native Mod dependency source budget exceeded");
                }
                files.insert(format!("sources/{name}"), native_mod_file_sha256(&path)?);
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    files.insert("manifest".to_owned(), native_mod_file_sha256(manifest_path)?);
    walk(source_root, source_root, &mut files, 0)?;
    Ok(native_mod_inventory_identity(&files))
}

/// Dependency evidence for a Mod project's compile plan, keyed by dependency name.
pub fn native_mod_dependency_sources(plan: &crate::projects::CompilePlan) -> Result<BTreeMap<String, String>> {
    let mut sources = BTreeMap::new();
    for dependency in &plan.dependency_projects {
        let identity = native_mod_dependency_identity(&dependency.manifest_path, &dependency.source_root)?;
        if let Some(previous) = sources.insert(dependency.dependency_name.clone(), identity.clone())
            && previous != identity
        {
            bail!("native Mod dependency `{}` resolves to two different projects", dependency.dependency_name);
        }
    }
    Ok(sources)
}

/// Whether a descriptor's source evidence equals the current Mod project: every recorded source,
/// the exact source membership, the manifest, `Project.lock`, `project.mod`, and the dependency
/// source identities. Any difference makes the artifact stale.
pub fn native_mod_evidence_is_current(
    descriptor: &ModArtifactDescriptor,
    project_root: &Path,
    manifest_path: &Path,
    source_root: &Path,
    dependency_sources: &BTreeMap<String, String>,
) -> Result<bool> {
    for (name, digest) in &descriptor.source_files {
        let relative = name.strip_prefix("sources/").context("Mod source evidence outside source closure")?;
        let current = project_root.join(relative);
        if !current.exists() || native_mod_file_sha256(&current)? != *digest {
            return Ok(false);
        }
    }
    let source_inventory = mod_artifact_inventory(source_root)?;
    let source_relative = source_root
        .strip_prefix(project_root)?
        .to_str()
        .context("non UTF8 Mod source root")?
        .replace(std::path::MAIN_SEPARATOR, "/");
    for (name, digest) in source_inventory {
        let evidence =
            if source_relative.is_empty() { format!("sources/{name}") } else { format!("sources/{source_relative}/{name}") };
        if descriptor.source_files.get(&evidence) != Some(&digest) {
            return Ok(false);
        }
    }
    for path in [
        manifest_path.to_path_buf(),
        project_root.join(crate::projects::PROJECT_LOCK_FILE_NAME),
        project_root.join("project.mod"),
    ] {
        if path.exists() {
            let relative = path
                .strip_prefix(project_root)?
                .to_str()
                .context("non UTF8 Mod authority path")?
                .replace(std::path::MAIN_SEPARATOR, "/");
            if descriptor.source_files.get(&format!("sources/{relative}")) != Some(&native_mod_file_sha256(&path)?) {
                return Ok(false);
            }
        }
    }
    Ok(descriptor.dependency_sources == *dependency_sources)
}

pub fn read_mod_artifact_descriptor(path: &Path, runtime_prefix: &Path) -> Result<ModArtifactDescriptor> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
        bail!("native Mod descriptor must be a bounded regular file");
    }
    let mut descriptor: ModArtifactDescriptor =
        serde_json::from_slice(&std::fs::read(path)?).context("invalid executable Mod descriptor V2")?;
    descriptor.artifact_dir = path.parent().context("native Mod descriptor has no artifact directory")?.to_path_buf();
    descriptor.validate(runtime_prefix)?;
    Ok(descriptor)
}

#[cfg(test)]
impl ModArtifactDescriptor {
    pub(crate) fn context_fixture() -> Self {
        Self {
            schema_version: 2,
            artifact_kind: "executable-shared-library".into(),
            host_abi_version: 2,
            package_id: String::new(),
            package_version: None,
            mod_source_hash: String::new(),
            lock_hash: String::new(),
            sdk_schema_sha256: String::new(),
            sdk_source_sha256: String::new(),
            target_triple: String::new(),
            compiler_version: String::new(),
            executable_file: String::new(),
            discriminator_symbol: String::new(),
            registrations: Vec::new(),
            callables: BTreeMap::new(),
            files: BTreeMap::new(),
            source_files: BTreeMap::new(),
            sdk_files: BTreeMap::new(),
            dependency_sources: BTreeMap::new(),
            build_tools: Vec::new(),
            runtime: NativeModRuntimeBinding {
                profile: BuildProfile::Debug,
                abi_metadata_sha256: String::new(),
                provider_manifest_sha256: String::new(),
                runtime_source_sha256: String::new(),
                issuer_source_sha256: String::new(),
                shared_library_sha256: String::new(),
            },
            artifact_key: String::new(),
            artifact_dir: PathBuf::new(),
        }
    }
}
