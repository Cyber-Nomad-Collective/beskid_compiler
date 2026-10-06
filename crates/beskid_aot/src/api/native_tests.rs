//! One compiled object shared by fresh-process native test entrypoints.
use crate::object_module::{BeskidObjectModule, emitted_object_symbol};
use crate::runtime::{RuntimeArtifact, RuntimeBuildRequest, RuntimeLinkage, merged_link_libraries, prepare_runtime};
use crate::{AotError, AotResult, BuildOutputKind, BuildProfile, LinkMode, RuntimeKitRequest};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub struct NativeTestObject {
    object: PathBuf,
    target: String,
    entries: HashMap<String, String>,
    core_args: Option<&'static beskid_abi::generated::abi_v5_contract::GeneratedCoreArgsEntryAdapter>,
    runtime: RuntimeArtifact,
    initialization: Option<super::glue::CompiledDynamicInitialization>,
    control: super::NativeExecutionControl,
    external_libraries: Vec<String>,
    library_search_paths: Vec<PathBuf>,
}

pub fn emit_native_test_object(
    prepared: beskid_codegen::PreparedSyntaxEntrypoints,
    directory: &Path,
    profile: BuildProfile,
    kit: RuntimeKitRequest,
    external_libraries: Vec<String>,
    library_search_paths: Vec<PathBuf>,
    pipeline: Option<&dyn beskid_pipeline::PipelineObserver>,
    control: super::NativeExecutionControl,
) -> AotResult<NativeTestObject> {
    control.check("emit_object")?;
    let target = kit.target.triple.as_str().to_owned();
    super::validation::validate_extern_libraries(prepared.artifact(), &external_libraries)?;
    let core_args = super::validation::core_args_entry_adapter(prepared.artifact(), &target)?;
    let mut entries = HashMap::new();
    let mut linked = HashSet::new();
    for entry in prepared.entries() {
        let function =
            prepared.artifact().functions.iter().find(|function| function.name == entry.symbol()).ok_or_else(|| {
                AotError::InvalidRequest { message: format!("native test `{}` has no emitted function", entry.name()) }
            })?;
        if entry.return_type() != beskid_queries::SemanticTypeId::UNIT
            || !function.function.signature.returns.is_empty()
            || !function.function.signature.params.is_empty()
        {
            return Err(AotError::InvalidRequest {
                message: format!("native test `{}` must take no arguments and return void", entry.name()),
            });
        }
        let symbol = emitted_object_symbol(entry.symbol(), &prepared.artifact().exports, None);
        if !beskid_codegen::artifact::is_valid_link_name(&symbol)
            || symbol == "main"
            || symbol == "wmain"
            || !linked.insert(symbol.clone())
        {
            return Err(AotError::InvalidRequest {
                message: format!("native test `{}` has an invalid or colliding host symbol", entry.name()),
            });
        }
        if entries.insert(entry.name().to_owned(), symbol).is_some() {
            return Err(AotError::InvalidRequest { message: "duplicate native test selection".into() });
        }
    }
    std::fs::create_dir_all(directory)
        .map_err(|error| AotError::Io { path: directory.to_owned(), message: error.to_string() })?;
    let platform = crate::target::detect_target(Some(&target))?;
    let object = directory.join(format!("tests.{}", platform.object_ext));
    let mut module = BeskidObjectModule::new(Some(&target), profile)?;
    beskid_pipeline::observe_phase_result(pipeline, beskid_pipeline::phases::AOT_EMIT_OBJECT, || {
        module.compile_artifact_with_control(prepared.artifact(), &linked, pipeline, &control)
    })?;
    module.finalize_to_path(&object)?;
    control.check("emit_object")?;
    let request = RuntimeBuildRequest {
        kit,
        linkage: if prepared.artifact().dynamic_initialization.is_some() {
            RuntimeLinkage::GlueSharedProviderV1
        } else {
            RuntimeLinkage::CanonicalStatic
        },
    };
    let initialization =
        super::glue::compile_dynamic_initialization(prepared.artifact(), &request, directory, &control)?;
    let runtime = prepare_runtime(&request)?;
    Ok(NativeTestObject {
        object,
        target,
        entries,
        core_args,
        runtime,
        initialization,
        control,
        external_libraries,
        library_search_paths,
    })
}

impl NativeTestObject {
    pub fn execute_linked_entry(&self, executable: &Path, directory: &Path) -> AotResult<std::process::Output> {
        self.control
            .run_command(&mut std::process::Command::new(executable), directory, "execute test")
            .map_err(Into::into)
    }
    pub fn link_entry(&self, name: &str, directory: &Path) -> AotResult<PathBuf> {
        let symbol = self.entries.get(name).ok_or_else(|| AotError::MissingEntrypoint { symbol: name.into() })?;
        std::fs::create_dir_all(directory)
            .map_err(|error| AotError::Io { path: directory.to_owned(), message: error.to_string() })?;
        let bootstrap = super::platform_objects::compile_native_test_bootstrap(
            &self.target,
            self.core_args,
            directory,
            symbol,
            &self.control,
            self.initialization.as_ref().map(|init| (init.initializer_symbol(), init.generation())),
        )?;
        let target = crate::target::detect_target(Some(&self.target))?;
        let output = directory.join(crate::target::output_filename("test", BuildOutputKind::Exe, &target));
        let mut additional_objects = vec![bootstrap];
        if let Some(initialization) = &self.initialization {
            initialization.verify_object()?;
            additional_objects.push(initialization.object_path().to_owned());
        }
        crate::linker::link_with_control(
            &crate::linker::LinkRequest {
                target_triple: Some(self.target.clone()),
                output_kind: BuildOutputKind::Exe,
                output_path: output.clone(),
                object_path: self.object.clone(),
                additional_object_paths: additional_objects,
                runtime: Some(crate::linker::RuntimeLinkInput::try_from(&self.runtime)?),
                host_staticlib: None,
                entrypoint_symbol: symbol.clone(),
                exported_symbols: Vec::new(),
                link_mode: LinkMode::Auto,
                verbose: false,
                external_libraries: merged_link_libraries(&self.external_libraries, &self.runtime.platform_libraries),
                library_search_paths: self.library_search_paths.clone(),
            },
            Some(&self.control),
        )?;
        Ok(output)
    }
}
