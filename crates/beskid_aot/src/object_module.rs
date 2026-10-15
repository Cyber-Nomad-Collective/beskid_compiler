//! Cranelift object-module wrapper: declare imports, lower [`CodegenArtifact`] functions, write `.o`/`.obj`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use beskid_codegen::cranelift_host::{
    declare_referenced_builtin_imports, declare_user_functions_with_link_symbols_and_linkage,
    declare_validated_extern_imports, remap_testcase_externals,
};
#[cfg(debug_assertions)]
use beskid_codegen::validate_artifact;
use beskid_codegen::{CodegenArtifact, emit_string_literals};
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

    /// Emit the checked body counterpart of a fallible Glue export into the same object after its
    /// sole descriptor pass. `input` is the Glue body input of the current packet.
    pub(crate) fn emit_glue_checked_export<'db>(
        &mut self,
        input: &'db beskid_codegen::CodegenInput<'db>,
        isa: &'db dyn TargetIsa,
        entry: beskid_queries::AstNodeKey,
        symbol: &str,
    ) -> AotResult<()> {
        let module = self.module.as_mut().ok_or_else(|| AotError::InvalidRequest {
            message: "checked Glue export after object finalization".into(),
        })?;
        let mut literals = beskid_codegen::CodegenContext::new_with_artifact_namespace(symbol);
        let function = beskid_codegen::glue::emit_checked_export(
            module,
            input,
            isa,
            entry,
            &mut literals,
            &self.func_ids,
            symbol,
        )?;
        let literal_artifact = CodegenArtifact { string_literals: literals.string_literals, ..Default::default() };
        self.data_ids.extend(emit_string_literals(module, &literal_artifact)?);
        self.func_ids.insert(symbol.to_owned(), function);
        self.declared_symbols.push(symbol.to_owned());
        Ok(())
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

        let platform_imports = referenced_platform_imports(module.isa(), artifact)?;
        #[cfg(debug_assertions)]
        if let Err(missing) = validate_artifact(artifact) {
            let names: Vec<_> = missing
                .iter()
                .map(|m| m.name.as_str())
                .filter(|name| !platform_imports.iter().any(|(symbol, _)| symbol == name))
                .collect();
            if !names.is_empty() {
                return Err(AotError::InvalidRequest {
                    message: format!("codegen artifact validation failed: undefined callees: {}", names.join(", ")),
                });
            }
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
        // Kit platform imports (libm math, libc, ...) are the exact-kit authority for `clif { call @symbol }`
        // callees that no `[Extern]` contract declares, exactly as the JIT admits them through the kit
        // import allowlist. The native link already carries the target's runtime platform libraries.
        for (symbol, signature) in platform_imports {
            if self.func_ids.contains_key(&symbol) {
                continue;
            }
            let id = module.declare_function(&symbol, Linkage::Import, &signature)?;
            self.func_ids.insert(symbol, id);
        }

        self.data_ids = emit_string_literals(module, artifact)?;
        beskid_codegen::emit_closure_static_plans(module, artifact).map_err(AotError::from)?;

        // Bind every function to its declared id and final external names first (cheap, needs
        // the module), then compile the independent bodies to machine code in parallel, then
        // define them in artifact order so the object layout does not depend on scheduling.
        let mut bound = Vec::with_capacity(artifact.functions.len());
        let mut ctx = module.make_context();
        for function in &artifact.functions {
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
            bound.push((func_id, std::mem::replace(&mut ctx.func, cranelift_codegen::ir::Function::new())));
        }
        module.clear_context(&mut ctx);
        if let Some(control) = control {
            control.check("compile_function")?;
        }
        let compiled = compile_functions(module.isa(), bound)?;
        let total = compiled.len() as u64;
        for (index, (function, compiled)) in artifact.functions.iter().zip(compiled).enumerate() {
            if let Some(control) = control {
                control.check("compile_function")?;
            }
            module.define_function_bytes(compiled.func_id, compiled.alignment, &compiled.bytes, &compiled.relocs)?;
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

/// Function-kind platform imports of the target's canonical ABI-v5 manifest that `artifact`'s CLIF
/// calls and that no artifact function, extern import, or runtime import supplies, each with its
/// call signature. Every call site must match the manifest signature exactly; a mismatch fails closed.
fn referenced_platform_imports(
    isa: &dyn TargetIsa,
    artifact: &CodegenArtifact,
) -> AotResult<Vec<(String, cranelift_codegen::ir::Signature)>> {
    use beskid_abi::abi_v5::{AbiManifestV5, AbiType, PlatformImportKind, TargetMetadata};
    use cranelift_codegen::ir::{ExternalName, types};

    let defined = artifact.functions.iter().map(|function| function.name.as_str()).collect::<HashSet<_>>();
    let supplied = artifact
        .extern_imports
        .iter()
        .chain(artifact.trusted_extern_imports.iter())
        .map(|import| import.symbol.as_str())
        .collect::<HashSet<_>>();
    let mut referenced: Vec<(String, cranelift_codegen::ir::Signature)> = Vec::new();
    for function in &artifact.functions {
        for (_, external) in function.function.dfg.ext_funcs.iter() {
            let ExternalName::TestCase(name) = &external.name else {
                continue;
            };
            let symbol = String::from_utf8_lossy(name.raw()).into_owned();
            if defined.contains(symbol.as_str())
                || supplied.contains(symbol.as_str())
                || beskid_abi::is_runtime_import_symbol(&symbol)
            {
                continue;
            }
            referenced.push((symbol, function.function.dfg.signatures[external.signature].clone()));
        }
    }
    if referenced.is_empty() {
        return Ok(Vec::new());
    }
    let Ok(target) = TargetMetadata::for_triple(&isa.triple().to_string()) else {
        return Ok(Vec::new());
    };
    let manifest = AbiManifestV5::canonical_runtime(target);
    let pointer = isa.pointer_type();
    let clif_type = |ty: AbiType| match ty {
        AbiType::Void => None,
        AbiType::Pointer | AbiType::USize | AbiType::ISize => Some(pointer),
        AbiType::I8 | AbiType::U8 => Some(types::I8),
        AbiType::I16 | AbiType::U16 => Some(types::I16),
        AbiType::I32 | AbiType::U32 => Some(types::I32),
        AbiType::I64 | AbiType::U64 => Some(types::I64),
        AbiType::V128 => Some(types::I8X16),
        AbiType::F32 => Some(types::F32),
        AbiType::F64 => Some(types::F64),
    };
    let mut admitted: Vec<(String, cranelift_codegen::ir::Signature)> = Vec::new();
    for (symbol, signature) in referenced {
        let Some(import) = manifest
            .platform_imports
            .iter()
            .find(|import| import.kind == PlatformImportKind::Function && import.symbol == symbol)
        else {
            continue;
        };
        let params = signature.params.iter().map(|param| Some(param.value_type)).collect::<Vec<_>>();
        let expected_params = import.params.iter().map(|ty| clif_type(*ty)).collect::<Vec<_>>();
        let returns = signature.returns.iter().map(|ret| ret.value_type).collect::<Vec<_>>();
        let expected_returns = clif_type(import.result).into_iter().collect::<Vec<_>>();
        if params != expected_params || returns != expected_returns {
            return Err(AotError::InvalidRequest {
                message: format!(
                    "platform import `{symbol}` is called with signature {signature}, which does not match the \
                     ABI-v5 manifest for target `{}`",
                    isa.triple()
                ),
            });
        }
        if !admitted.iter().any(|(existing, _)| existing == &symbol) {
            admitted.push((symbol, signature));
        }
    }
    Ok(admitted)
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

/// Machine code of one function, ready for [`Module::define_function_bytes`].
struct CompiledFunction {
    func_id: FuncId,
    alignment: u64,
    bytes: Vec<u8>,
    relocs: Vec<cranelift_module::ModuleReloc>,
}

/// Fewer functions than this are compiled on the calling thread.
const PARALLEL_COMPILE_THRESHOLD: usize = 32;

/// Compile bound functions to machine code, in parallel for large artifacts, returning results in
/// input order. The first failure in input order is reported, as serial compilation would.
/// `BESKID_CODEGEN_THREADS` bounds the worker count (1 forces serial compilation).
fn compile_functions(
    isa: &dyn TargetIsa,
    functions: Vec<(FuncId, cranelift_codegen::ir::Function)>,
) -> AotResult<Vec<CompiledFunction>> {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let compile = |func_id: FuncId, function: cranelift_codegen::ir::Function| -> AotResult<CompiledFunction> {
        let mut ctx = cranelift_codegen::Context::for_function(function);
        ctx.compile(isa, &mut cranelift_codegen::control::ControlPlane::default())
            .map_err(|error| AotError::from(cranelift_module::ModuleError::from(error)))?;
        let compiled = ctx.compiled_code().expect("compiled code after a successful compile");
        let alignment = compiled.buffer.alignment as u64;
        let bytes = compiled.buffer.data().to_vec();
        let relocs = compiled
            .buffer
            .relocs()
            .iter()
            .map(|reloc| cranelift_module::ModuleReloc::from_mach_reloc(reloc, &ctx.func, func_id))
            .collect();
        Ok(CompiledFunction { func_id, alignment, bytes, relocs })
    };
    let count = functions.len();
    let requested = std::env::var("BESKID_CODEGEN_THREADS").ok().and_then(|value| value.parse::<usize>().ok());
    let available = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let workers =
        if count < PARALLEL_COMPILE_THRESHOLD { 1 } else { requested.unwrap_or(available).clamp(1, 16).min(count) };
    if workers <= 1 {
        return functions.into_iter().map(|(func_id, function)| compile(func_id, function)).collect();
    }
    let queue = functions.into_iter().map(Some).map(Mutex::new).collect::<Vec<_>>();
    let next = AtomicUsize::new(0);
    let first_error = AtomicUsize::new(usize::MAX);
    let mut slots: Vec<Option<AotResult<CompiledFunction>>> = (0..count).map(|_| None).collect();
    std::thread::scope(|scope| -> AotResult<()> {
        let mut spawned = Vec::with_capacity(workers);
        for _ in 0..workers {
            let (queue, next, first_error, compile) = (&queue, &next, &first_error, &compile);
            let worker = std::thread::Builder::new()
                .name("beskid-object".into())
                .stack_size(beskid_pipeline::compiler_stack_size())
                .spawn_scoped(scope, move || {
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        if index >= count || index > first_error.load(Ordering::Relaxed) {
                            break;
                        }
                        let (func_id, function) =
                            queue[index].lock().expect("compile queue").take().expect("each function is taken once");
                        let result = compile(func_id, function);
                        if result.is_err() {
                            first_error.fetch_min(index, Ordering::Relaxed);
                        }
                        done.push((index, result));
                    }
                    done
                })
                .map_err(|error| AotError::InvalidRequest {
                    message: format!("spawn object compile worker: {error}"),
                })?;
            spawned.push(worker);
        }
        for worker in spawned {
            match worker.join() {
                Ok(done) => {
                    for (index, result) in done {
                        slots[index] = Some(result);
                    }
                }
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        Ok(())
    })?;
    let mut compiled = Vec::with_capacity(count);
    for slot in slots {
        match slot {
            Some(result) => compiled.push(result?),
            None => {
                return Err(AotError::InvalidRequest {
                    message: "parallel object compilation skipped a function before any failure".into(),
                });
            }
        }
    }
    Ok(compiled)
}

#[cfg(test)]
mod platform_import_tests {
    use cranelift_codegen::ir::{AbiParam, ExtFuncData, ExternalName, Function, Signature, types};
    use cranelift_codegen::settings;

    use beskid_codegen::{CodegenArtifact, LoweredFunction};

    use super::referenced_platform_imports;

    fn host_isa() -> std::sync::Arc<dyn cranelift_codegen::isa::TargetIsa> {
        cranelift_native::builder().expect("host ISA").finish(settings::Flags::new(settings::builder())).expect("ISA")
    }

    fn calling(symbol: &str, param: types::Type, result: types::Type) -> CodegenArtifact {
        let isa = host_isa();
        let mut function = Function::new();
        let mut signature = Signature::new(isa.default_call_conv());
        signature.params.push(AbiParam::new(param));
        signature.returns.push(AbiParam::new(result));
        let signature = function.import_signature(signature);
        function.import_function(ExtFuncData {
            name: ExternalName::testcase(symbol.as_bytes()),
            signature,
            colocated: false,
            patchable: false,
        });
        CodegenArtifact { functions: vec![LoweredFunction { name: "Floor".into(), function }], ..Default::default() }
    }

    #[test]
    fn manifest_math_import_is_admitted_with_its_exact_signature() {
        let isa = host_isa();
        if beskid_abi::abi_v5::TargetMetadata::for_triple(&isa.triple().to_string()).is_err() {
            return;
        }
        let admitted = referenced_platform_imports(isa.as_ref(), &calling("floor", types::F64, types::F64))
            .expect("floor matches the manifest");
        assert_eq!(admitted.iter().map(|(symbol, _)| symbol.as_str()).collect::<Vec<_>>(), ["floor"]);
    }

    #[test]
    fn manifest_math_import_with_a_mismatched_signature_fails_closed() {
        let isa = host_isa();
        if beskid_abi::abi_v5::TargetMetadata::for_triple(&isa.triple().to_string()).is_err() {
            return;
        }
        let error = referenced_platform_imports(isa.as_ref(), &calling("floor", types::I64, types::I64))
            .expect_err("an i64 floor call must not match the manifest");
        assert!(error.to_string().contains("platform import `floor`"), "{error}");
    }

    #[test]
    fn symbols_outside_the_manifest_are_not_admitted() {
        let isa = host_isa();
        let admitted = referenced_platform_imports(isa.as_ref(), &calling("not_a_platform_symbol", types::F64, types::F64))
            .expect("unknown symbols are left to the undefined-callee check");
        assert!(admitted.is_empty());
    }
}
