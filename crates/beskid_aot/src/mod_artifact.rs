//! Atomic executable Mod publication from codegen-issued typed SDK trampolines.
use crate::{
    api::{BuildOutputKind, BuildProfile, LinkMode, NativeExecutionControl, RuntimeKitRequest},
    error::{AotError, AotResult},
    linker::{LinkRequest, LinkToolInvocation, LinkToolReceipt, link_with_control, run_link_tool},
};
use beskid_abi::runtime_kit::GlueProviderToolV1;
pub use beskid_analysis::mod_host::{ContractRegistration, ModArtifactDescriptor};
use beskid_analysis::mod_host::{
    mod_artifact_inventory, native_mod_file_sha256, native_mod_inventory_identity, native_mod_runtime_binding,
    read_mod_artifact_descriptor,
};
use beskid_codegen::PreparedNativeMod;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// Source-to-native witness issued only by the actual typed producer and linker.
/// Reading a descriptor or rehashing a caller-provided image cannot construct it.
pub struct QualifiedNativeMod {
    descriptor: ModArtifactDescriptor,
    runtime_prefix: PathBuf,
}
impl QualifiedNativeMod {
    pub fn descriptor(&self) -> &ModArtifactDescriptor {
        &self.descriptor
    }
    pub fn verify_native_closure(&self) -> AotResult<()> {
        self.descriptor.validate(&self.runtime_prefix).map_err(invalid)
    }
}

pub struct ModArtifactBuildRequest {
    pub prepared: PreparedNativeMod,
    pub workspace_root: PathBuf,
    pub project_root: PathBuf,
    pub manifest_path: PathBuf,
    pub source_root: PathBuf,
    pub lockfile_path: Option<PathBuf>,
    pub package_id: String,
    pub package_version: Option<String>,
    pub compiler_version: String,
    pub runtime: RuntimeKitRequest,
    pub control: NativeExecutionControl,
}
fn invalid(error: impl std::fmt::Display) -> AotError {
    AotError::InvalidRequest { message: error.to_string() }
}
fn io(path: &Path, error: impl std::fmt::Display) -> AotError {
    AotError::Io { path: path.to_path_buf(), message: error.to_string() }
}

pub fn build_mod_artifact(req: ModArtifactBuildRequest) -> AotResult<QualifiedNativeMod> {
    req.control.check("Mod artifact preparation")?;
    if req.prepared.target() != &req.runtime.target || req.prepared.host_abi_version() != 2 {
        return Err(invalid("prepared Mod requires exact runtime target and typed host ABI2"));
    }
    if req.package_id.is_empty()
        || req.package_id.contains(['/', '\\'])
        || matches!(req.package_id.as_str(), "." | "..")
    {
        return Err(invalid("invalid Mod package identity"));
    }
    let root = req.workspace_root.join(".beskid/obj/mods").join(&req.package_id);
    fs::create_dir_all(&root).map_err(|error| io(&root, error))?;
    let stage = tempfile::Builder::new().prefix(".staged-mod-").tempdir_in(&root).map_err(|error| io(&root, error))?;
    let dir = stage.path();
    let source_files = stage_sources(&req, dir)?;
    let mut sdk_files = BTreeMap::new();
    for (name, bytes) in req.prepared.sdk_sources() {
        write_evidence(dir, &format!("sdk/{name}"), bytes, &mut sdk_files)?;
    }
    write_evidence(dir, "sdk/schema.json", req.prepared.sdk_schema(), &mut sdk_files)?;
    let sdk_schema_sha256 = native_mod_file_sha256(&dir.join("sdk/schema.json")).map_err(invalid)?;
    let mut registrations = Vec::new();
    let mut callables = BTreeMap::new();
    let mut exports = vec![req.prepared.discriminator_symbol().to_owned()];
    for entry in req.prepared.entries() {
        let registration = entry.registration().clone();
        let identity = entry.identity().clone();
        if source_files.get(&identity.source_file) != Some(&identity.source_sha256) {
            return Err(invalid("prepared Mod source differs from staged source evidence"));
        }
        if callables.insert(registration.entry_symbol.clone(), identity).is_some() {
            return Err(invalid("duplicate prepared Mod dispatcher identity"));
        }
        exports.push(registration.entry_symbol.clone());
        registrations.push(registration);
    }
    if registrations.is_empty() {
        return Err(invalid("Mod has no codegen-issued typed contract adapter"));
    }
    fs::write(dir.join("adapter-plan.json"), req.prepared.adapter_metadata()).map_err(|error| io(dir, error))?;
    let adapter_source = dir.join("adapter.c");
    fs::write(&adapter_source, req.prepared.adapter_source()).map_err(|error| io(&adapter_source, error))?;
    let adapter_object = dir.join("adapter.o");
    let adapter_compiler = crate::api::compile_generated_c_object(
        req.runtime.target.triple.as_str(),
        &adapter_source,
        &adapter_object,
        &[],
        &req.control,
    )?;
    let object = dir.join("mod.o");
    let mut module =
        crate::object_module::BeskidObjectModule::new(Some(req.runtime.target.triple.as_str()), BuildProfile::Debug)?;
    module.compile_artifact_with_control(
        req.prepared.artifact(),
        &req.prepared.artifact().exports.iter().map(|entry| entry.exported_symbol.clone()).collect(),
        None,
        &req.control,
    )?;
    module.finalize_to_path(&object)?;
    req.control.check("Mod object emission")?;
    let runtime = crate::runtime::prepare_runtime(&crate::runtime::RuntimeBuildRequest {
        kit: req.runtime.clone(),
        linkage: crate::runtime::RuntimeLinkage::GlueSharedProviderV1,
    })?;
    let executable_file = format!("{}mod{}", std::env::consts::DLL_PREFIX, std::env::consts::DLL_SUFFIX);
    let executable = dir.join(&executable_file);
    let linked = link_with_control(
        &LinkRequest {
            target_triple: Some(req.runtime.target.triple.as_str().to_owned()),
            output_kind: BuildOutputKind::SharedLib,
            output_path: executable,
            object_path: object,
            additional_object_paths: vec![adapter_object],
            runtime: Some(crate::linker::RuntimeLinkInput::try_from(&runtime)?),
            host_staticlib: None,
            entrypoint_symbol: String::new(),
            exported_symbols: exports,
            link_mode: LinkMode::Auto,
            verbose: false,
            external_libraries: runtime.platform_libraries.clone(),
            library_search_paths: Vec::new(),
        },
        Some(&req.control),
    )?;
    let linker = linked.tool_receipt.ok_or_else(|| invalid("Mod shared library lacks exact linker receipt"))?;
    let compiler_path = std::env::current_exe()
        .map_err(|error| invalid(format!("Mod compiler identity: {error}")))?
        .canonicalize()
        .map_err(invalid)?;
    let compiler =
        LinkToolReceipt { sha256: native_mod_file_sha256(&compiler_path).map_err(invalid)?, executable: compiler_path };
    let build_tools = vec![
        stage_tool(dir, "compiler", &compiler, &req.control)?,
        stage_tool(dir, "adapter-compiler", &adapter_compiler, &req.control)?,
        stage_tool(dir, "linker", &linker, &req.control)?,
    ];
    let runtime_binding =
        native_mod_runtime_binding(&req.runtime.prefix, &req.runtime.target, req.runtime.profile).map_err(invalid)?;
    let lock_hash = match &req.lockfile_path {
        Some(path) => native_mod_file_sha256(path).map_err(invalid)?,
        None => format!("{:x}", Sha256::digest([])),
    };
    let mut descriptor = ModArtifactDescriptor {
        schema_version: 2,
        artifact_kind: "executable-shared-library".into(),
        host_abi_version: 2,
        package_id: req.package_id,
        package_version: req.package_version,
        mod_source_hash: native_mod_inventory_identity(&source_files),
        lock_hash,
        sdk_schema_sha256,
        sdk_source_sha256: native_mod_inventory_identity(&sdk_files),
        target_triple: req.runtime.target.triple.as_str().to_owned(),
        compiler_version: req.compiler_version,
        executable_file,
        discriminator_symbol: req.prepared.discriminator_symbol().into(),
        registrations,
        callables,
        files: mod_artifact_inventory(dir).map_err(invalid)?,
        source_files,
        sdk_files,
        build_tools,
        runtime: runtime_binding,
        artifact_key: String::new(),
        artifact_dir: dir.to_path_buf(),
    };
    descriptor.validate(&req.runtime.prefix).map_err(invalid)?;
    let identity = serde_json::to_vec(&descriptor).map_err(invalid)?;
    descriptor.artifact_key = format!("{:x}", Sha256::digest(identity));
    let final_dir = mod_artifact_dir(
        &req.workspace_root,
        &descriptor.package_id,
        &descriptor.artifact_key,
        &descriptor.target_triple,
    );
    fs::write(descriptor.sidecar_path(), serde_json::to_vec_pretty(&descriptor).map_err(invalid)?)
        .map_err(|error| io(dir, error))?;
    sync_tree(dir)?;
    fs::create_dir_all(final_dir.parent().expect("cache target parent")).map_err(|error| io(&final_dir, error))?;
    req.control.check("Mod atomic publication")?;
    if final_dir.exists() {
        let existing = read_mod_artifact_descriptor(&final_dir.join("mod.descriptor.json"), &req.runtime.prefix)
            .map_err(invalid)?;
        if serde_json::to_value(&existing).map_err(invalid)? != serde_json::to_value(&descriptor).map_err(invalid)? {
            return Err(invalid("conflicting immutable Mod artifact cache entry"));
        }
        return Ok(QualifiedNativeMod { descriptor: existing, runtime_prefix: req.runtime.prefix.clone() });
    }
    // The content-addressed parent lock excludes cooperating publishers; never replace a cache entry.
    let lock = final_dir.parent().unwrap().join(".publish-lock");
    let _guard = PublicationLock::acquire(&lock)?;
    if final_dir.exists() {
        return Err(invalid("Mod cache publication raced; retry validated cache lookup"));
    }
    publish_directory_no_replace(dir, &final_dir).map_err(|error| io(&final_dir, error))?;
    sync_directory(final_dir.parent().unwrap())?;
    descriptor.artifact_dir = final_dir;
    Ok(QualifiedNativeMod { descriptor, runtime_prefix: req.runtime.prefix.clone() })
}
fn write_evidence(dir: &Path, name: &str, bytes: &[u8], out: &mut BTreeMap<String, String>) -> AotResult<()> {
    if name.contains('\\')
        || name.split('/').any(|part| part.is_empty() || matches!(part, "." | ".."))
        || Path::new(name).is_absolute()
    {
        return Err(invalid("invalid Mod evidence path"));
    }
    let path = dir.join(name);
    fs::create_dir_all(path.parent().unwrap()).map_err(|error| io(&path, error))?;
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|error| io(&path, error))?;
    use std::io::Write;
    file.write_all(bytes).map_err(|error| io(&path, error))?;
    out.insert(name.into(), native_mod_file_sha256(&path).map_err(invalid)?);
    Ok(())
}
fn stage_sources(req: &ModArtifactBuildRequest, dir: &Path) -> AotResult<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let project = req.project_root.canonicalize().map_err(invalid)?;
    let mut inputs = vec![req.manifest_path.clone()];
    fn collect(path: &Path, inputs: &mut Vec<PathBuf>, depth: usize) -> AotResult<()> {
        if depth > 64 {
            return Err(invalid("Mod source depth exceeded"));
        }
        for entry in fs::read_dir(path).map_err(|error| io(path, error))? {
            let entry = entry.map_err(invalid)?;
            let path = entry.path();
            let kind = entry.file_type().map_err(invalid)?;
            if kind.is_dir() {
                collect(&path, inputs, depth + 1)?;
            } else if kind.is_file() {
                inputs.push(path);
            } else {
                return Err(invalid("Mod source symlinks/special files forbidden"));
            }
        }
        Ok(())
    }
    collect(&req.source_root, &mut inputs, 0)?;
    if let Some(lock) = &req.lockfile_path {
        inputs.push(lock.clone());
    }
    let mod_file = project.join("project.mod");
    if mod_file.is_file() {
        inputs.push(mod_file);
    }
    inputs.sort();
    inputs.dedup();
    for original_path in inputs {
        native_mod_file_sha256(&original_path).map_err(invalid)?;
        let path = original_path.canonicalize().map_err(invalid)?;
        let relative = path
            .strip_prefix(&project)
            .map_err(|_| invalid("Mod source lies outside canonical project"))?
            .to_str()
            .ok_or_else(|| invalid("non UTF8 Mod source"))?
            .replace(std::path::MAIN_SEPARATOR, "/");
        native_mod_file_sha256(&path).map_err(invalid)?;
        write_evidence(
            dir,
            &format!("sources/{relative}"),
            &fs::read(&path).map_err(|error| io(&path, error))?,
            &mut out,
        )?;
    }
    Ok(out)
}
fn stage_tool(
    dir: &Path,
    role: &str,
    receipt: &LinkToolReceipt,
    control: &NativeExecutionControl,
) -> AotResult<GlueProviderToolV1> {
    let invocation = LinkToolInvocation {
        program: receipt.executable.as_os_str().to_owned(),
        args: vec!["--version".into()],
        environment: Vec::new(),
        current_dir: None,
    };
    let (output, current) = run_link_tool(&invocation, dir, Some(control))?;
    if !output.status.success()
        || current.sha256 != receipt.sha256
        || output.stdout.len() + output.stderr.len() > 1024 * 1024
        || output.stdout.is_empty() && output.stderr.is_empty()
    {
        return Err(invalid("Mod tool version/identity verification failed"));
    }
    let mut bytes = Vec::new();
    for part in [&output.stdout, &output.stderr] {
        bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
        bytes.extend_from_slice(part);
    }
    let mut evidence = BTreeMap::new();
    write_evidence(dir, &format!("tools/{role}-version"), &bytes, &mut evidence)?;
    let binary = fs::read(&receipt.executable).map_err(invalid)?;
    write_evidence(dir, &format!("tools/{role}-executable"), &binary, &mut evidence)?;
    Ok(GlueProviderToolV1 {
        role: role.into(),
        executable_sha256: receipt.sha256.clone(),
        version_output_sha256: format!("{:x}", Sha256::digest(bytes)),
    })
}
fn sync_tree(path: &Path) -> AotResult<()> {
    for entry in fs::read_dir(path).map_err(invalid)? {
        let entry = entry.map_err(invalid)?;
        if entry.file_type().map_err(invalid)?.is_dir() {
            sync_tree(&entry.path())?;
        } else {
            // FlushFileBuffers requires a write-capable handle on Windows. These are
            // private staged files, never pre-existing user/cache payloads.
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(entry.path())
                .map_err(invalid)?
                .sync_all()
                .map_err(invalid)?;
        }
    }
    sync_directory(path)
}
fn sync_directory(path: &Path) -> AotResult<()> {
    #[cfg(unix)]
    {
        fs::File::open(path).map_err(invalid)?.sync_all().map_err(invalid)?;
    }
    #[cfg(windows)]
    {
        // Windows publication uses MoveFileExW WRITE_THROUGH after flushing every
        // staged file. Unix directory fsync is not emulated with a read-only handle.
        let _ = path;
    }
    #[cfg(not(any(unix, windows)))]
    {
        return Err(invalid("durable native Mod cache publication is unsupported on this platform"));
    }
    Ok(())
}
/// Publish one immutable same-volume cache directory without replacing any destination.
fn publish_directory_no_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::os::unix::ffi::OsStrExt;
        let source = std::ffi::CString::new(source.as_os_str().as_bytes())
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "publication source contains NUL"))?;
        let destination = std::ffi::CString::new(destination.as_os_str().as_bytes()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "publication destination contains NUL")
        })?;
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                destination.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(target_os = "macos")]
        let result = unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }
        return Ok(());
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
        }
        let mut source: Vec<u16> = source.as_os_str().encode_wide().collect();
        let mut destination: Vec<u16> = destination.as_os_str().encode_wide().collect();
        if source.contains(&0) || destination.contains(&0) {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "publication path contains NUL"));
        }
        source.push(0);
        destination.push(0);
        // No REPLACE_EXISTING or COPY_ALLOWED: existing destinations and cross-volume
        // moves fail, and no partially copied cache directory becomes visible.
        const MOVEFILE_WRITE_THROUGH: u32 = 8;
        if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        return Ok(());
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = (source, destination);
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "atomic no-replace publication is unsupported"))
    }
}

#[cfg(test)]
mod publication_v06_tests {
    use super::*;
    #[test]
    fn v06_mod_publication_rejects_existing_empty_destination() {
        let root = tempfile::tempdir().unwrap();
        let staged = root.path().join("staged");
        let destination = root.path().join("existing");
        fs::create_dir(&staged).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(staged.join("payload"), b"staged").unwrap();
        assert!(publish_directory_no_replace(&staged, &destination).is_err());
        assert_eq!(fs::read(staged.join("payload")).unwrap(), b"staged");
        assert!(destination.is_dir());
        assert!(!destination.join("payload").exists());
    }
    #[test]
    fn v06_mod_publication_moves_complete_flushed_tree() {
        let root = tempfile::tempdir().unwrap();
        let staged = root.path().join("staged");
        let destination = root.path().join("published");
        fs::create_dir_all(staged.join("nested")).unwrap();
        fs::write(staged.join("nested/payload"), b"complete").unwrap();
        sync_tree(&staged).unwrap();
        publish_directory_no_replace(&staged, &destination).unwrap();
        sync_directory(root.path()).unwrap();
        assert!(!staged.exists());
        assert_eq!(fs::read(destination.join("nested/payload")).unwrap(), b"complete");
    }
}

struct PublicationLock(PathBuf);
impl PublicationLock {
    fn acquire(path: &Path) -> AotResult<Self> {
        fs::create_dir(path).map_err(|error| io(path, error))?;
        Ok(Self(path.to_path_buf()))
    }
}
impl Drop for PublicationLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}
pub fn mod_artifact_dir(workspace_root: &Path, package_id: &str, artifact_key: &str, target_triple: &str) -> PathBuf {
    workspace_root.join(".beskid/obj/mods").join(package_id).join(artifact_key).join(target_triple)
}

/// A producer-owned executable set. Callers cannot attach a raw image or descriptor.
pub struct QualifiedModInvoker {
    artifacts: Vec<QualifiedNativeMod>,
    worker: PathBuf,
    control: NativeExecutionControl,
}
impl QualifiedModInvoker {
    pub fn new(
        artifacts: Vec<QualifiedNativeMod>,
        worker: PathBuf,
        control: NativeExecutionControl,
    ) -> AotResult<Self> {
        if artifacts.is_empty() {
            return Err(invalid("qualified Mod executor requires producer-issued artifacts"));
        }
        let worker = worker.canonicalize().map_err(|e| io(&worker, e))?;
        if !worker.is_file() {
            return Err(invalid("native Mod worker is not an executable file"));
        }
        for artifact in &artifacts {
            artifact.verify_native_closure()?;
        }
        Ok(Self { artifacts, worker, control })
    }
    fn execute(
        &self,
        r: &ContractRegistration,
        context: &beskid_analysis::mod_host::ModCollectRequest,
        targets: &[String],
        family: &str,
        authority: Option<&dyn beskid_analysis::mod_host::ModSemanticAuthority>,
    ) -> anyhow::Result<beskid_analysis::mod_host::NativeInvocationOutput> {
        let matches = self
            .artifacts
            .iter()
            .filter(|a| a.descriptor.registrations.iter().any(|issued| issued == r))
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            anyhow::bail!("native registration has absent or ambiguous private producer authority");
        }
        let artifact = matches[0];
        artifact.verify_native_closure()?;
        let authority =
            authority.ok_or_else(|| anyhow::anyhow!("native contract requires current registered syntax authority"))?;
        beskid_analysis::mod_host::invoke_qualified_native_transport(
            &artifact.descriptor,
            &artifact.runtime_prefix,
            &self.worker,
            &r.entry_symbol,
            serde_json::to_value(context)?,
            targets,
            family,
            authority,
            &self.control,
        )
    }
    fn error(
        r: &ContractRegistration,
        e: impl std::fmt::Display,
    ) -> beskid_analysis::mod_host::ContractInvocationError {
        beskid_analysis::mod_host::ContractInvocationError {
            package_id: String::new(),
            contract_id: r.contract_id.clone(),
            type_id: r.type_id.clone(),
            message: e.to_string(),
        }
    }
    fn schema(&self, r: &ContractRegistration) -> anyhow::Result<Vec<u8>> {
        let mut matches = self.artifacts.iter().filter(|a| a.descriptor.registrations.iter().any(|issued| issued == r));
        let artifact = matches.next().ok_or_else(|| anyhow::anyhow!("native registration absent"))?;
        if matches.next().is_some() {
            anyhow::bail!("native registration ambiguous")
        };
        artifact.verify_native_closure()?;
        Ok(std::fs::read(artifact.descriptor.artifact_dir.join("sdk/schema.json"))?)
    }
}
impl beskid_analysis::mod_host::ContractInvoker for QualifiedModInvoker {
    fn invoke_collector(
        &self,
        r: &ContractRegistration,
        request: &beskid_analysis::mod_host::ModCollectRequest,
        authority: Option<&dyn beskid_analysis::mod_host::ModSemanticAuthority>,
    ) -> Result<beskid_analysis::mod_host::CollectorOutcome, beskid_analysis::mod_host::ContractInvocationError> {
        self.execute(r, request, &[], "Collector", authority)
            .and_then(|value| beskid_analysis::mod_host::native_collector_result(&r.type_id, &value.value))
            .map_err(|e| Self::error(r, e))
    }
    fn invoke_generator(
        &self,
        r: &ContractRegistration,
        request: &beskid_analysis::mod_host::ModGenerationRequest,
        authority: Option<&dyn beskid_analysis::mod_host::ModSemanticAuthority>,
    ) -> Result<beskid_analysis::mod_host::GeneratorOutcome, beskid_analysis::mod_host::ContractInvocationError> {
        let family = match r.contract_id.as_str() {
            "Beskid.Compiler.Collect.GrammarGenerator" => "GrammarGenerator",
            "Beskid.Compiler.Collect.AttributeGenerator" => "AttributeGenerator",
            _ => "Generator",
        };
        self.execute(r, &request.context, &request.targets.target_ids, family, authority)
            .and_then(|value| {
                let mut outcome = if family == "AttributeGenerator" {
                    beskid_analysis::mod_host::native_attribute_result(&r.type_id, &value.value, &self.schema(r)?)?
                } else {
                    beskid_analysis::mod_host::native_generator_result(&r.type_id, &value.value, &self.schema(r)?)?
                };
                outcome.compiled_metadata = value.compiled_metadata;
                Ok(outcome)
            })
            .map_err(|e| Self::error(r, e))
    }
    fn invoke_analyzer(
        &self,
        r: &ContractRegistration,
        request: &beskid_analysis::mod_host::ModCollectRequest,
        _: Option<&beskid_analysis::services::SemanticSnapshot>,
        authority: Option<&dyn beskid_analysis::mod_host::ModSemanticAuthority>,
    ) -> Result<beskid_analysis::mod_host::AnalyzerOutcome, beskid_analysis::mod_host::ContractInvocationError> {
        self.execute(r, request, &[], "Analyzer", authority)
            .and_then(|value| {
                beskid_analysis::mod_host::native_analyzer_result(
                    &r.type_id,
                    &value.value,
                    request.compilation.entry_source_text.len(),
                )
            })
            .map_err(|e| Self::error(r, e))
    }
    fn invoke_rewriter(
        &self,
        r: &ContractRegistration,
        request: &beskid_analysis::mod_host::ModCollectRequest,
        authority: Option<&dyn beskid_analysis::mod_host::ModSemanticAuthority>,
    ) -> Result<beskid_analysis::mod_host::RewriterOutcome, beskid_analysis::mod_host::ContractInvocationError> {
        let run = || -> anyhow::Result<_> {
            let matches = self
                .artifacts
                .iter()
                .filter(|a| a.descriptor.registrations.iter().any(|issued| issued == r))
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                anyhow::bail!("Rewriter lacks unique private producer witness")
            };
            let artifact = matches[0];
            artifact.verify_native_closure()?;
            beskid_analysis::mod_host::invoke_qualified_native_rewriter(
                &artifact.descriptor,
                &artifact.runtime_prefix,
                &self.worker,
                &r.entry_symbol,
                serde_json::to_value(request)?,
                authority.ok_or_else(|| anyhow::anyhow!("Rewriter requires current registered syntax authority"))?,
                &self.control,
            )
        };
        run().map_err(|e| Self::error(r, e))
    }
}
