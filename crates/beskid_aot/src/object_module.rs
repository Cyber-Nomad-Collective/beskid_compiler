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

/// Owns a Cranelift object builder until [`Self::finalize_to_path`] consumes it.
pub struct BeskidObjectModule {
    module: Option<ObjectModule>,
    func_ids: HashMap<String, FuncId>,
    data_ids: HashMap<String, DataId>,
    declared_symbols: Vec<String>,
}

impl BeskidObjectModule {
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
        self.compile_artifact_with_exports_and_entry_adapter(artifact, exported_symbols, None, pipeline)
    }

    /// Compile an artifact, optionally reserving native `main` for the generated Core.Args entry adapter.
    pub fn compile_artifact_with_exports_and_entry_adapter(
        &mut self,
        artifact: &CodegenArtifact,
        exported_symbols: &HashSet<String>,
        entry_adapter_program: Option<&str>,
        pipeline: Option<&dyn PipelineObserver>,
    ) -> AotResult<()> {
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
            |name| {
                if let Some(program_entry) =
                    entry_adapter_program.filter(|_| name.split('#').next().is_some_and(|logical| logical == "Main"))
                {
                    program_entry.to_owned()
                } else {
                    beskid_codegen::object_link_symbol(name, &exports)
                }
            },
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
    let mut flagBuilder = settings::builder();
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

/// Construct the exact ISA used by AOT object emission for a validated ABI target.
// Matches the external Cranelift settings builder convention used across the AOT pipeline.
#[allow(non_snake_case)]
pub(crate) fn ObjectTargetIsa(target: &str) -> AotResult<std::sync::Arc<dyn TargetIsa>> {
    #[allow(non_snake_case)]
    let mut flagBuilder = settings::builder();
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
    }

    // Test names follow the CamelCase convention used by the AOT profile suite.
    #[allow(non_snake_case)]
    #[test]
    fn ReleaseProfileUsesOptimizedVerifiedPicCodegen() {
        let flags = ObjectCodegenFlags(BuildProfile::Release).expect("release codegen flags");

        assert_eq!(flags.opt_level(), OptLevel::Speed);
        assert!(flags.enable_verifier());
        assert!(flags.is_pic());
    }
}
