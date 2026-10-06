//! Compilation of a sealed, compiler-issued program initialization closure.
use super::invalid;
use crate::{
    AotResult,
    linker::LinkToolReceipt,
    runtime::{RuntimeArtifact, RuntimeBuildRequest, RuntimeLinkage, prepare_runtime},
};
use beskid_codegen::CodegenArtifact;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// A produced initialization object. Its constructor is private; consumers
/// verify its exact bytes immediately before linking the prepared program.
#[derive(Debug)]
pub struct CompiledDynamicInitialization {
    _stage: tempfile::TempDir,
    object: PathBuf,
    object_sha256: [u8; 32],
    source_sha256: [u8; 32],
    symbol: String,
    generation: u64,
    compiler: LinkToolReceipt,
    runtime: RuntimeArtifact,
}
impl CompiledDynamicInitialization {
    pub fn object_path(&self) -> &Path {
        &self.object
    }
    pub fn initializer_symbol(&self) -> &str {
        &self.symbol
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn compiler_receipt(&self) -> &LinkToolReceipt {
        &self.compiler
    }
    pub fn source_sha256(&self) -> &[u8; 32] {
        &self.source_sha256
    }
    pub fn runtime(&self) -> &RuntimeArtifact {
        &self.runtime
    }
    pub fn verify_object(&self) -> AotResult<()> {
        if hash(&self.object)? != self.object_sha256 {
            return Err(invalid("Dynamic initialization object changed after production"));
        }
        Ok(())
    }
}

/// Compile only the initialization plan retained with an unchanged lowered
/// artifact, using the validated canonical shared provider and target compiler.
/// This does not admit a linked program or grant caller-authored shape metadata.
pub fn compile_dynamic_initialization(
    artifact: &CodegenArtifact,
    runtime_request: &RuntimeBuildRequest,
    output_directory: &Path,
    control: &crate::api::NativeExecutionControl,
) -> AotResult<Option<CompiledDynamicInitialization>> {
    let Some(plan) = artifact.dynamic_initialization.as_ref() else { return Ok(None) };
    plan.validate_artifact(artifact).map_err(invalid)?;
    if &runtime_request.kit.target != plan.target() {
        return Err(invalid("Dynamic initializer target differs from current compiled source authority"));
    }
    if runtime_request.linkage != RuntimeLinkage::GlueSharedProviderV1 {
        return Err(invalid("Dynamic initialization requires the canonical shared-provider profile"));
    }
    let runtime = prepare_runtime(runtime_request)?;
    control.check("Dynamic initialization production")?;
    fs::create_dir_all(output_directory).map_err(|e| invalid(e.to_string()))?;
    let stage = tempfile::Builder::new()
        .prefix(".beskid-dynamic-init-")
        .tempdir_in(output_directory)
        .map_err(|e| invalid(e.to_string()))?;
    let directory = stage.path();
    let source = plan.native_source();
    let stem = plan.initializer_symbol();
    let source_path = directory.join(format!("{stem}.c"));
    let object_path = directory.join(format!("{stem}.o"));
    let header_path = directory.join("owner_identity_v1.h");
    // The provider source/header is the same embedded canonical authority used
    // by runtime-kit validation, never read from an installed build-host path.
    fs::write(&header_path, include_bytes!("../../../../../runtime/Glue/owner_identity_v1.h"))
        .map_err(|e| invalid(e.to_string()))?;
    fs::write(&source_path, &source).map_err(|e| invalid(e.to_string()))?;
    let compiler = crate::api::compile_generated_c_object(
        runtime_request.kit.target.triple.as_str(),
        &source_path,
        &object_path,
        &[directory.to_owned()],
        control,
    )?;
    // Recheck source/header closure after the external tool returned.
    if fs::read(&source_path).map_err(|e| invalid(e.to_string()))? != source
        || fs::read(&header_path).map_err(|e| invalid(e.to_string()))?
            != include_bytes!("../../../../../runtime/Glue/owner_identity_v1.h").as_slice()
    {
        return Err(invalid("Dynamic initialization source changed during compilation"));
    }
    let object_sha256 = hash(&object_path)?;
    Ok(Some(CompiledDynamicInitialization {
        _stage: stage,
        object: object_path,
        object_sha256,
        source_sha256: Sha256::digest(&source).into(),
        symbol: stem.to_owned(),
        generation: plan.generation().0,
        compiler,
        runtime,
    }))
}
fn hash(path: &Path) -> AotResult<[u8; 32]> {
    let metadata = fs::symlink_metadata(path).map_err(|e| invalid(e.to_string()))?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 64 * 1024 * 1024 {
        return Err(invalid("Dynamic initializer object is not a bounded regular artifact"));
    }
    let bytes = fs::read(path).map_err(|e| invalid(e.to_string()))?;
    if bytes.len() as u64 != metadata.len() {
        return Err(invalid("Dynamic initializer object changed while hashing"));
    }
    Ok(Sha256::digest(bytes).into())
}
