//! Cranelift object-module wrapper: declare imports, lower [`CodegenArtifact`] functions, write `.o`/`.obj`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use beskid_codegen::cranelift_host::{
    declare_referenced_builtin_imports, declare_user_functions_with_link_symbols_and_linkage,
    declare_validated_extern_imports, remap_testcase_externals,
};
#[cfg(debug_assertions)]
use beskid_codegen::validate_artifact;
use beskid_codegen::{CodegenArtifact, emit_string_literals, emit_type_descriptors};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::settings;
use cranelift_codegen::settings::Configurable;
use cranelift_module::{DataId, FuncId, Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};

use beskid_pipeline::{PipelineObserver, emit_work_unit, phases::AOT_EMIT_OBJECT};

use crate::api::BuildProfile;
use crate::error::{AotError, AotResult};

/// Fixed C boundary called by the portable executable bootstrap.
pub(crate) const EXECUTABLE_PROGRAM_ENTRY: &str = "beskid_program_main";

/// One logical Beskid function remapped to the executable-host program boundary.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExecutableEntrySymbol<'a> {
    pub(crate) logical: &'a str,
    pub(crate) symbol: &'a str,
}

pub(crate) fn emitted_object_symbol(
    name: &str,
    exports: &[beskid_codegen::ExportEntry],
    executable_entry: Option<ExecutableEntrySymbol<'_>>,
) -> String {
    if let Some(entry) = executable_entry.filter(|entry| name.split('#').next() == Some(entry.logical)) {
        entry.symbol.to_owned()
    } else if executable_entry.is_some()
        && name.split('#').next() == Some("Main")
        && !exports.iter().any(|export| export.beskid_name == "Main" || export.beskid_name == name)
    {
        // In an executable, the portable host owns C `main`. An unselected
        // Beskid Main is an ordinary callable function, not a second startup.
        // It still gets the same link-safe internal name as every other unexported function.
        beskid_codegen::internal_link_symbol(name)
    } else {
        beskid_codegen::object_link_symbol(name, exports)
    }
}

/// Owns a Cranelift object builder until [`Self::finalize_to_path`] consumes it.
pub struct BeskidObjectModule {
    module: Option<ObjectModule>,
    func_ids: HashMap<String, FuncId>,
    data_ids: HashMap<String, DataId>,
    declared_symbols: Vec<String>,
}

impl BeskidObjectModule {
    /// Reissues exact current nominal token layouts, then defines their private
    /// normalized constructors/readers against this object's sole data pass.
    pub(crate) fn emit_glue_handle_transports(
        &mut self,input:&beskid_codegen::CodegenInput<'_>,bindings:&[beskid_queries::AstNodeKey],
    )->AotResult<Vec<beskid_codegen::glue::EmittedHandleTransport>> {
        let module=self.module.as_mut().ok_or_else(||AotError::InvalidRequest{message:"opaque transport after object finalization".into()})?;
        let emitted=beskid_codegen::glue::emit_source_handle_transports(module,input,bindings)?;
        for transport in &emitted {
            for (name,id) in [transport.constructor(),transport.reader()].into_iter().zip(transport.functions().iter().copied()) {
                self.func_ids.insert(name.to_owned(),id);
                if !self.declared_symbols.iter().any(|symbol|symbol==name){self.declared_symbols.push(name.to_owned());}
            }
        }
        Ok(emitted)
    }
    /// Emits the source-issued checked call graph and its caller-root wrapper
    /// into this same object after its sole descriptor pass.
    pub(crate) fn emit_glue_checked_entry<'db>(
        &mut self,
        input: &'db beskid_codegen::CodegenInput<'db>,
        isa: &'db dyn TargetIsa,
        entry: &beskid_codegen::module_emission::CheckedFailureEntry,
        symbol: &str,
    ) -> AotResult<usize> {
        let module = self.module.as_mut().ok_or_else(|| AotError::InvalidRequest {
            message: "checked Glue entry after object finalization".into(),
        })?;
        let mut literals = beskid_codegen::CodegenContext::new_with_artifact_namespace(symbol);
        let emitted = beskid_codegen::reserved_failure::emit_admitted_checked_entry(
            module,
            input,
            isa,
            entry.key(),
            entry.specialization(),
            &mut literals,
            &self.func_ids,
            symbol,
            symbol,
        )?;
        let literal_artifact = CodegenArtifact { string_literals: literals.string_literals, ..Default::default() };
        self.data_ids.extend(emit_string_literals(module, &literal_artifact)?);
        self.func_ids.insert(symbol.to_owned(), emitted.function());
        self.declared_symbols.push(symbol.to_owned());
        Ok(emitted.source_parameter_count())
    }

    /// Emit a source-issued guarded constructor/read/map counterpart into the
    /// same object and retain its exact entry/specialization proof for private
    /// image-role admission. This is not a callback-address registration API.
    pub(crate) fn emit_glue_checked_callback<'db>(
        &mut self,
        input: &'db beskid_codegen::CodegenInput<'db>,
        isa: &'db dyn TargetIsa,
        entry: beskid_queries::AstNodeKey,
        specialization: Option<&beskid_queries::GenericSpecializationInstance>,
        symbol: &str,
    ) -> AotResult<beskid_codegen::checked_callback::EmittedCheckedCallback> {
        let module = self.module.as_mut().ok_or_else(|| AotError::InvalidRequest {
            message: "checked callback after object finalization".into(),
        })?;
        let mut literals = beskid_codegen::CodegenContext::new_with_artifact_namespace(symbol);
        let emitted = beskid_codegen::checked_callback::emit_checked_callback(
            module, input, isa, entry, specialization, &mut literals,
            &self.func_ids, symbol, symbol,
        )?;
        let literal_artifact = CodegenArtifact {
            string_literals: literals.string_literals,
            ..Default::default()
        };
        self.data_ids.extend(emit_string_literals(module, &literal_artifact)?);
        self.func_ids.insert(symbol.to_owned(), emitted.function());
        self.declared_symbols.push(symbol.to_owned());
        Ok(emitted)
    }

    /// Source-issued Glue admission reuses the sole artifact descriptor pass.
    /// It never accepts an independently reconstructed allocation request.
    pub(crate) fn emit_glue_failure_admission(
        &mut self,
        plan: &beskid_codegen::reserved_failure::ReservedFailureStaticPlan,
        symbol: &str,
    ) -> AotResult<()> {
        let module = self.module.as_mut().ok_or_else(|| AotError::InvalidRequest {
            message: "Glue failure admission after object finalization".into(),
        })?;
        let request = |name: &str| match module.get_name(name) {
            Some(cranelift_module::FuncOrDataId::Data(id)) => Ok(id),
            _ => Err(AotError::InvalidRequest {
                message: format!("Glue failure request was not issued by the artifact descriptor pass: {name}"),
            }),
        };
        let result = request(&plan.result().allocation_request_symbol)?;
        let failure = request(&plan.failure().allocation_request_symbol)?;
        let id =
            beskid_codegen::reserved_failure::emit_reserved_failure_admission(module, plan, result, failure, symbol)?;
        self.func_ids.insert(symbol.to_owned(), id);
        self.declared_symbols.push(symbol.to_owned());
        Ok(())
    }

    /// Construct a profiled module for `target_triple` or the host ISA when `None`.
    pub fn new(target_triple: Option<&str>, profile: BuildProfile) -> AotResult<Self> {
        let flags = ObjectCodegenFlags(profile)?;

        let isa_builder = if let Some(triple) = target_triple {
            cranelift_codegen::isa::lookup_by_name(triple)
                .map_err(|err| AotError::IsaInit { message: err.to_string() })?
        } else {
            cranelift_native::builder().map_err(|err| AotError::IsaInit { message: err.to_string() })?
        };

        let isa = isa_builder.finish(flags).map_err(|err| AotError::IsaInit { message: err.to_string() })?;

        let builder = ObjectBuilder::new(isa, "beskid", default_libcall_names())
            .map_err(|err| AotError::ObjectModule { message: err.to_string() })?;

        Ok(Self {
            module: Some(ObjectModule::new(builder)),
            func_ids: HashMap::new(),
            data_ids: HashMap::new(),
            declared_symbols: Vec::new(),
        })
    }

    /// Declare builtins, user functions, externs, data, then define every lowered function in `artifact`.
    ///
    /// This compatibility entrypoint exports every declared function. Production AOT publication
    /// must call [`Self::compile_artifact_with_exports`] with its explicit export boundary.
    pub fn compile_artifact(
        &mut self,
        artifact: &CodegenArtifact,
        pipeline: Option<&dyn PipelineObserver>,
    ) -> AotResult<()> {
        let exports = artifact.exports.clone();
        let exported_symbols = artifact
            .functions
            .iter()
            .map(|function| beskid_codegen::object_link_symbol(&function.name, &exports))
            .collect::<HashSet<_>>();
        self.compile_artifact_with_exports(artifact, &exported_symbols, pipeline)
    }

    /// Compile `artifact` with only `exported_symbols` visible at the native object boundary.
    pub fn compile_artifact_with_exports(
        &mut self,
        artifact: &CodegenArtifact,
        exported_symbols: &HashSet<String>,
        pipeline: Option<&dyn PipelineObserver>,
    ) -> AotResult<()> {
        self.compile_artifact_with_exports_and_executable_entry(artifact, exported_symbols, None, pipeline, None)
    }

    /// Compile an artifact, optionally reserving one internal symbol for the executable host.
    pub fn compile_artifact_with_control(
        &mut self,
        artifact: &CodegenArtifact,
        exports: &HashSet<String>,
        pipeline: Option<&dyn PipelineObserver>,
        control: &crate::api::NativeExecutionControl,
    ) -> AotResult<()> {
        self.compile_artifact_with_exports_and_executable_entry(artifact, exports, None, pipeline, Some(control))
    }

    pub(crate) fn compile_artifact_with_exports_and_executable_entry(
        &mut self,
        artifact: &CodegenArtifact,
        exported_symbols: &HashSet<String>,
        executable_entry: Option<ExecutableEntrySymbol<'_>>,
        pipeline: Option<&dyn PipelineObserver>,
        control: Option<&crate::api::NativeExecutionControl>,
    ) -> AotResult<()> {
        if let Some(plan) = &artifact.dynamic_initialization {
            plan.validate_artifact(artifact).map_err(|message| AotError::InvalidRequest { message })?;
        }
        let module = self
            .module
            .as_mut()
            .ok_or_else(|| AotError::InvalidRequest { message: "object module already finalized".to_owned() })?;

        #[cfg(debug_assertions)]
        if let Err(missing) = validate_artifact(artifact) {
            let names: Vec<_> = missing.iter().map(|m| m.name.as_str()).collect();
            return Err(AotError::InvalidRequest {
                message: format!("codegen artifact validation failed: undefined callees: {}", names.join(", ")),
            });
        }

        let exports = artifact.exports.clone();
        let declared = declare_user_functions_with_link_symbols_and_linkage(
            module,
            artifact,
            &mut self.func_ids,
            |name| emitted_object_symbol(name, &exports, executable_entry),
            |symbol| {
                if exported_symbols.contains(symbol) { Linkage::Export } else { Linkage::Local }
            },
        )?;
        // Only symbols selected by the caller's export policy use Export linkage. This keeps
        // canonical syntax implementation functions out of the static runtime boundary.
        self.declared_symbols.extend(declared);
        declare_referenced_builtin_imports(module, artifact, &mut self.func_ids)?;
        declare_validated_extern_imports(module, artifact, &mut self.func_ids).map_err(|err| match err {
            beskid_codegen::cranelift_host::ExternDeclarationError::InvalidSignature(message) => {
                AotError::InvalidRequest { message }
            }
            beskid_codegen::cranelift_host::ExternDeclarationError::Module(error) => AotError::from(error),
        })?;

        self.data_ids = emit_string_literals(module, artifact)?;
        beskid_codegen::emit_closure_static_plans(module, artifact).map_err(AotError::from)?;
        let descriptor_ids = emit_type_descriptors(module, artifact)?;
        for handles in descriptor_ids.values() {
            let descriptor_name = format!("__data_{}", handles.descriptor.as_u32());
            let offsets_name = format!("__data_{}", handles.offsets.as_u32());
            self.data_ids.entry(descriptor_name).or_insert(handles.descriptor);
            self.data_ids.entry(offsets_name).or_insert(handles.offsets);
        }

        let mut ctx = module.make_context();
        let total = artifact.functions.len() as u64;
        for (index, function) in artifact.functions.iter().enumerate() {
            if let Some(control) = control {
                control.check("compile_function")?;
            }
            let func_id = self
                .func_ids
                .get(&function.name)
                .copied()
                .ok_or_else(|| AotError::MissingFunction { name: function.name.clone() })?;
            ctx.func = function.function.clone();
            remap_testcase_externals(module, &mut ctx, &self.func_ids).map_err(|err| match err {
                beskid_codegen::cranelift_host::HostError::MissingSymbol(name) => AotError::MissingFunction { name },
                beskid_codegen::cranelift_host::HostError::InvalidGlobalValue => {
                    AotError::InvalidRequest { message: "expected symbol global value".to_owned() }
                }
            })?;
            module.define_function(func_id, &mut ctx)?;
            module.clear_context(&mut ctx);
            emit_work_unit(pipeline, AOT_EMIT_OBJECT, (index as u64) + 1, total, function.name.clone());
        }

        Ok(())
    }

    /// Resolved Cranelift function id after declarations (tests / diagnostics).
    pub fn get_func_id(&self, name: &str) -> Option<FuncId> {
        self.func_ids.get(name).copied()
    }

    /// User-declared export symbol names accumulated during [`Self::compile_artifact`].
    pub fn declared_symbols(&self) -> Vec<String> {
        self.declared_symbols.clone()
    }

    /// Finish the module and write object bytes to `output_object` (consumes `self`).
    pub fn finalize_to_path(mut self, output_object: &Path) -> AotResult<()> {
        let module = self
            .module
            .take()
            .ok_or_else(|| AotError::InvalidRequest { message: "object module already finalized".to_owned() })?;
        let product = module.finish();
        let bytes = product.emit().map_err(|err| AotError::ObjectModule { message: err.to_string() })?;
        if let Some(parent) = output_object.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| AotError::Io { path: parent.to_path_buf(), message: err.to_string() })?;
        }
        std::fs::write(output_object, bytes)
            .map_err(|err| AotError::Io { path: output_object.to_path_buf(), message: err.to_string() })
    }
}

// Matches the external Cranelift settings builder convention used across the AOT pipeline.
#[allow(non_snake_case)]
fn ObjectCodegenFlags(profile: BuildProfile) -> AotResult<settings::Flags> {
    #[allow(non_snake_case)]
    let mut flagBuilder = beskid_codegen::cranelift_host::production_isa_settings_builder()
        .map_err(|err| AotError::IsaInit { message: err.to_string() })?;
    flagBuilder.set("is_pic", "true").map_err(|err| AotError::IsaInit { message: err.to_string() })?;
    flagBuilder.set("enable_verifier", "true").map_err(|err| AotError::IsaInit { message: err.to_string() })?;
    #[allow(non_snake_case)]
    let optimizationLevel = match profile {
        BuildProfile::Debug => "none",
        BuildProfile::Release => "speed",
    };
    flagBuilder.set("opt_level", optimizationLevel).map_err(|err| AotError::IsaInit { message: err.to_string() })?;
    Ok(settings::Flags::new(flagBuilder))
}

#[cfg(test)]
mod tests {
    use super::{EXECUTABLE_PROGRAM_ENTRY, ExecutableEntrySymbol, emitted_object_symbol};

    #[test]
    fn executable_entry_remapping_only_changes_the_selected_logical_entry() {
        let entry = ExecutableEntrySymbol { logical: "Start", symbol: EXECUTABLE_PROGRAM_ENTRY };

        assert_eq!(emitted_object_symbol("Start#0", &[], Some(entry)), EXECUTABLE_PROGRAM_ENTRY);
        // Non-entry functions keep their own identity, in the mangled link-safe form that
        // `beskid_codegen::object_link_symbol` gives every unexported internal name.
        assert_eq!(emitted_object_symbol("Helper#0", &[], Some(entry)), "Helper_H23_0");
        assert_eq!(emitted_object_symbol("Main#0", &[], Some(entry)), "Main_H23_0");
        assert_eq!(emitted_object_symbol("Main#0", &[], None), "main");
        let exports = [beskid_codegen::ExportEntry {
            beskid_name: "Main".into(),
            exported_symbol: "PublicHelper".into(),
            abi: "C".into(),
        }];
        assert_eq!(emitted_object_symbol("Main#0", &exports, Some(entry)), "PublicHelper");
    }
}

/// Construct the exact ISA used by AOT object emission for a validated ABI target.
// Matches the external Cranelift settings builder convention used across the AOT pipeline.
#[allow(non_snake_case)]
pub(crate) fn ObjectTargetIsa(target: &str) -> AotResult<std::sync::Arc<dyn TargetIsa>> {
    #[allow(non_snake_case)]
    let mut flagBuilder = beskid_codegen::cranelift_host::production_isa_settings_builder()
        .map_err(|err| AotError::IsaInit { message: err.to_string() })?;
    flagBuilder.set("is_pic", "true").map_err(|err| AotError::IsaInit { message: err.to_string() })?;
    cranelift_codegen::isa::lookup_by_name(target)
        .map_err(|err| AotError::IsaInit { message: err.to_string() })?
        .finish(settings::Flags::new(flagBuilder))
        .map_err(|err| AotError::IsaInit { message: err.to_string() })
}

#[cfg(test)]
mod profile_tests {
    use cranelift_codegen::settings::OptLevel;

    use super::{BuildProfile, ObjectCodegenFlags};

    // Test names follow the CamelCase convention used by the AOT profile suite.
    #[allow(non_snake_case)]
    #[test]
    fn DebugProfileUsesUnoptimizedVerifiedPicCodegen() {
        let flags = ObjectCodegenFlags(BuildProfile::Debug).expect("debug codegen flags");

        assert_eq!(flags.opt_level(), OptLevel::None);
        assert!(flags.enable_verifier());
        assert!(flags.is_pic());
        assert!(flags.preserve_frame_pointers());
    }

    // Test names follow the CamelCase convention used by the AOT profile suite.
    #[allow(non_snake_case)]
    #[test]
    fn ReleaseProfileUsesOptimizedVerifiedPicCodegen() {
        let flags = ObjectCodegenFlags(BuildProfile::Release).expect("release codegen flags");

        assert_eq!(flags.opt_level(), OptLevel::Speed);
        assert!(flags.enable_verifier());
        assert!(flags.is_pic());
        assert!(flags.preserve_frame_pointers());
    }
}
