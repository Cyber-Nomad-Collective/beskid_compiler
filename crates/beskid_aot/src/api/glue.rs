//! Compiler-issued native Glue admission. Generated JSON is a packet, not authority.
mod rust_owner;
pub use rust_owner::{
    PreparedRustOwner, RustOwnerBuildRequest, RustOwnerCallable, RustOwnerSource, RustOwnerType, build_rust_owner,
};
mod owner_tables;
pub use owner_tables::{GlueDeclaration, glue_declarations, rust_owner_declaration_libraries, rust_owner_source_tables};
mod initialization;
mod loader;
pub use initialization::{CompiledDynamicInitialization, compile_dynamic_initialization};
mod published;
pub use published::{PublishedGlue, PublishedGlueRequest, admit_published_glue_images};
mod reserved;
use crate::{
    AotError, AotResult,
    linker::{LinkResult, LinkToolReceipt},
    runtime::{RuntimeArtifact, RuntimeBuildRequest, RuntimeLinkage, prepare_runtime},
};
use beskid_codegen::{CodegenInput, glue::GlueArtifact, module_emission::SyntaxModuleItem};
pub use beskid_glue::{codec, wire};
pub use loader::{CheckedGlueCallable, GlueProcess};
use object::{Object, ObjectKind};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

const GLUE_INITIALIZATION_SYMBOL: &str = "beskid_glue_artifact_v1_initialize";
const GLUE_IMPORT_BINDING_SYMBOL:&str="beskid_glue_artifact_v1_bind_imports";

/// Non-deserializable witness issued only after current source and native closure validation.
#[derive(Debug)]
pub struct PreparedGlueArtifact {
    issued: IssuedGlueImage,
    link_tool: LinkToolReceipt,
    dependency_identity: String,
}

/// Loader-facing admission facts for one consumer image. Only actual production
/// (`build_glue_artifact`) or a pinned published admission record
/// (`admit_published_glue_images`) constructs one.
#[derive(Debug)]
struct IssuedGlueImage {
    packet: GlueArtifact,
    payload_path: PathBuf,
    payload_sha256: String,
    provider_path: PathBuf,
    provider_sha256: String,
    producer: GlueProducerWitness,
}

impl IssuedGlueImage {
    /// Loader revalidation before retaining provider and foreign-image handles.
    fn verify_native_closure(&self) -> AotResult<()> {
        if hash_file(&self.payload_path)? != self.payload_sha256
            || hash_file(&self.provider_path)? != self.provider_sha256
        {
            return Err(invalid("Glue native payload or provider changed after admission"));
        }
        Ok(())
    }
}

#[derive(Debug)]
struct GlueProducerWitness {
    syntax_generation: beskid_analysis::syntax::SyntaxGenerationId,
    body_object_sha256: String,
    adapter_object_sha256: String,
    adapter_source_sha256: String,
    adapter_compiler: LinkToolReceipt,
    native_sha256: String,
    failure_admissions: Vec<String>,
    checked_entries: Vec<CheckedEntryWitness>,
    dynamic_shapes: Vec<DynamicShapeWitness>,
    handle_transports: Vec<HandleTransportWitness>,
}

#[derive(Debug)]
struct HandleTransportWitness {
    brand: String,
    constructor: String,
    reader: String,
    descriptor: String,
}

#[derive(Debug)]
struct DynamicShapeWitness {
    signature: Vec<u8>,
    signature_sha256: [u8; 32],
    payload_descriptor_getter: String,
}

#[derive(Debug)]
struct CheckedEntryWitness {
    symbol: String,
    source_symbol: String,
    admission_symbol: String,
    source_parameter_count: usize,
}

pub struct GlueArtifactBuildRequest<'a> {
    pub input: &'a CodegenInput<'a>,
    pub all_items: &'a [SyntaxModuleItem],
    pub selected: &'a [SyntaxModuleItem],
    pub native_library: &'a str,
    pub expected: &'a GlueArtifact,
    pub runtime: &'a RuntimeBuildRequest,
    pub output_path: &'a Path,
    pub control: &'a super::NativeExecutionControl,
    /// Rust owner images already produced for libraries this consumer references. The
    /// consumer's embedded admission record pins their exact image digests (Gate WEB-GLUE03).
    pub owners: &'a [&'a PreparedRustOwner],
}

/// Produce native bodies and exact generated adapters in one authority-owned path.
pub fn build_glue_artifact(req: &GlueArtifactBuildRequest<'_>) -> AotResult<PreparedGlueArtifact> {
    use crate::{
        api::{BuildOutputKind, BuildProfile, LinkMode},
        linker::{LinkRequest, RuntimeLinkInput, link_with_control},
        object_module::{BeskidObjectModule, ObjectTargetIsa},
    };
    let packet = beskid_codegen::glue::emit(req.input, req.selected, req.native_library)
        .map_err(|error| invalid(error.to_string()))?;
    if &packet != req.expected {
        return Err(invalid("Glue packet differs from current registered assembly"));
    }
    req.control.check("Glue production")?;
    let runtime = prepare_runtime(req.runtime)?;
    let parent = req.output_path.parent().ok_or_else(|| invalid("Glue output requires a parent directory"))?;
    fs::create_dir_all(parent).map_err(|error| invalid(error.to_string()))?;
    let stage = tempfile::Builder::new()
        .prefix(".beskid-glue-")
        .tempdir_in(parent)
        .map_err(|error| invalid(error.to_string()))?;
    for file in &packet.files {
        let path = stage.path().join(&file.path);
        fs::create_dir_all(path.parent().ok_or_else(|| invalid("invalid generated Glue path"))?)
            .map_err(|error| invalid(error.to_string()))?;
        fs::write(path, &file.bytes).map_err(|error| invalid(error.to_string()))?;
    }
    let target = req.runtime.kit.target.triple.as_str();
    let isa = ObjectTargetIsa(target)?;
    let mut bodies = beskid_codegen::glue::lower_native_bodies(
        req.input,
        isa.as_ref(),
        req.all_items,
        req.selected,
        req.native_library,
        &packet,
    )
    .map_err(|error| invalid(error.to_string()))?;
    // Validate the complete original source seal before this private producer
    // appends its separately reissued immutable reservation descriptors. The
    // public bare-artifact path cannot perform this privileged transformation.
    if let Some(plan) = &bodies.dynamic_initialization {
        plan.validate_artifact(&bodies).map_err(invalid)?;
    }
    let _issued_initialization = bodies.dynamic_initialization.take();
    // Source-issued immutable failure constructors join the same descriptor pass
    // as every other body. No second emitter may recreate their descriptors.
    let mut failures = std::collections::BTreeMap::new();
    let checked_sources = beskid_codegen::module_emission::checked_failure_entries(req.input, req.all_items)
        .map_err(|error| invalid(error.to_string()))?;
    for entry in &checked_sources {
        let plan = entry.plan();
        let identity =
            serde_json::to_vec(&(&plan.result().allocation_request_symbol, &plan.failure().allocation_request_symbol))
                .map_err(|error| invalid(error.to_string()))?;
        let symbol = format!("beskid_glue_failure_admit_{:x}", Sha256::digest(identity));
        for allocation in [plan.result(), plan.failure()] {
            if let Some(existing) = bodies
                .aggregate_static_plans
                .iter()
                .find(|existing| existing.allocation_request_symbol == allocation.allocation_request_symbol)
            {
                if existing != allocation {
                    return Err(invalid("Glue failure allocation plan conflicts with source descriptor pass"));
                }
            } else {
                bodies.aggregate_static_plans.push(allocation.clone());
            }
        }
        failures.entry(symbol).or_insert_with(|| plan.clone());
    }
    let failure_admissions = failures.keys().cloned().collect::<Vec<_>>();
    let mut dynamic_shapes = std::collections::BTreeMap::new();
    for entry in &checked_sources {
        let Some(instance) = entry.specialization() else { continue };
        let Some(shape) = req.input.compiled_dynamic_packing_shape(instance).map_err(invalid)? else { continue };
        let bridge = beskid_queries::dynamic_packing_bridge(req.input.database(), instance)
            .map_err(|error| invalid(error.to_string()))?
            .ok_or_else(|| invalid("compiled Dynamic Pack bridge unavailable"))?;
        let plan = req
            .input
            .aggregate_static_plan_for_specialization(bridge.box_literal(), Some(instance))
            .ok_or_else(|| invalid("compiled Dynamic Pack allocation unavailable"))?;
        if !bodies.aggregate_static_plans.iter().any(|existing| existing == &plan) {
            return Err(invalid("Dynamic Pack descriptor absent from actual source descriptor pass"));
        }
        let getter = plan.descriptor_getter.ok_or_else(|| invalid("Dynamic Pack descriptor getter unavailable"))?;
        dynamic_shapes.entry(*shape.sha256()).or_insert(DynamicShapeWitness {
            signature: shape.bytes().to_vec(),
            signature_sha256: *shape.sha256(),
            payload_descriptor_getter: getter,
        });
    }
    let dynamic_shapes = dynamic_shapes.into_values().collect::<Vec<_>>();
    let profile = match req.runtime.kit.profile {
        beskid_abi::runtime_kit::BuildProfile::Debug => BuildProfile::Debug,
        beskid_abi::runtime_kit::BuildProfile::Release => BuildProfile::Release,
    };
    let body_path = stage.path().join("bodies.o");
    let mut module = BeskidObjectModule::new(Some(target), profile)?;
    let exports = packet
        .manifest
        .bindings
        .iter()
        .filter(|binding| binding.direction == "export")
        .map(|binding| binding.body_symbol.clone())
        .collect();
    module.compile_artifact_with_control(&bodies, &exports, None, req.control)?;
    let emitted_transports =
        module.emit_glue_handle_transports(req.input, &req.selected.iter().map(|item| item.key).collect::<Vec<_>>())?;
    // Actual emitted positions carry private source-object provenance. Bind
    // metadata to those positions; public packet names cannot issue a transport.
    for transport in &emitted_transports {
        let source = transport.binding();
        let span = beskid_queries::node_span(req.input.database(), source)
            .map_err(|error| invalid(error.to_string()))?
            .ok_or_else(|| invalid("opaque source position has no canonical span"))?;
        let unit = req.input.typed_program().assembly.units.iter().find(|unit|
            beskid_queries::SourceUnitId::new(req.input.database(),unit.path.clone()) == source.unit)
            .ok_or_else(|| invalid("opaque source unit is not in current assembly"))?;
        let binding = packet.manifest.bindings.iter().find(|binding|
            binding.source.logical_unit == unit.logical_name
            && binding.source.span_start == span.start && binding.source.span_end == span.end)
            .ok_or_else(|| invalid("opaque object transport has no selected source binding"))?;
        let physical = match transport.position() {
            Some(position) => binding.parameters.get(position)
                .ok_or_else(|| invalid("opaque transport exceeds source arity"))?,
            None => &binding.result,
        };
        let metadata = physical.opaque.as_ref()
            .ok_or_else(|| invalid("opaque object transport has no normalized metadata"))?;
        if metadata.brand_sha256 != transport.brand_sha256()
            || metadata.constructor != transport.constructor() || metadata.reader != transport.reader()
            || metadata.descriptor != transport.descriptor() {
            return Err(invalid("opaque packet differs from actual source object transport"));
        }
    }
    let handle_transports = emitted_transports
        .iter()
        .map(|transport| HandleTransportWitness {
            brand: transport.brand_sha256().to_owned(),
            constructor: transport.constructor().to_owned(),
            reader: transport.reader().to_owned(),
            descriptor: transport.descriptor().to_owned(),
        })
        .collect::<Vec<_>>();
    let expected_handles = packet.manifest.bindings.iter().map(|binding|
        binding.parameters.iter().filter(|parameter|parameter.opaque.is_some()).count()
        + usize::from(binding.result.opaque.is_some())).sum::<usize>();
    if expected_handles != handle_transports.len() {
        return Err(invalid("opaque packet/object position coverage differs"));
    }
    for (symbol, plan) in &failures {
        module.emit_glue_failure_admission(plan, symbol)?;
    }
    let mut checked_entries = Vec::new();
    let mut emitted = std::collections::BTreeSet::new();
    for entry in &checked_sources {
        let plan = entry.plan();
        let identity =
            serde_json::to_vec(&(&plan.result().allocation_request_symbol, &plan.failure().allocation_request_symbol))
                .map_err(|error| invalid(error.to_string()))?;
        let admission_symbol = format!("beskid_glue_failure_admit_{:x}", Sha256::digest(identity));
        let identity =
            serde_json::to_vec(&(entry.symbol(), &admission_symbol)).map_err(|error| invalid(error.to_string()))?;
        let symbol = format!("beskid_glue_source_checked_{:x}", Sha256::digest(identity));
        if !emitted.insert(symbol.clone()) {
            continue;
        }
        let source_parameter_count = module.emit_glue_checked_entry(req.input, isa.as_ref(), entry, &symbol)?;
        checked_entries.push(CheckedEntryWitness {
            symbol,
            source_symbol: entry.symbol().to_owned(),
            admission_symbol,
            source_parameter_count,
        });
    }
    module.finalize_to_path(&body_path)?;
    let adapter_path = stage.path().join("adapters.o");
    let source = stage.path().join("native/adapters.c");
    let adapter_compiler = super::compile_generated_c_object(
        target,
        &source,
        &adapter_path,
        &[stage.path().join("include")],
        req.control,
    )?;
    let body_object_sha256 = hash_file(&body_path)?;
    let adapter_object_sha256 = hash_file(&adapter_path)?;
    let adapter_source_sha256 = hash_file(&source)?;
    // The native digest is known only after linking; every other producer fact is final here.
    let mut producer = GlueProducerWitness {
        syntax_generation: req.input.typed_program().generation,
        body_object_sha256,
        adapter_object_sha256,
        adapter_source_sha256,
        adapter_compiler,
        native_sha256: String::new(),
        failure_admissions,
        checked_entries,
        dynamic_shapes,
        handle_transports,
    };
    // Immutable admission record (Gate WEB-GLUE03): compiled into its own object before the
    // link, so the linker output and its platform code signature already contain it. It is never
    // patched into a linked image.
    let provider_path = fs::canonicalize(
        runtime.shared_library_path.as_ref().ok_or_else(|| invalid("Glue shared provider payload is missing"))?,
    )
    .map_err(|error| invalid(error.to_string()))?;
    let record = published::ConsumerRecord::new(
        req.native_library,
        &packet,
        &hash_file(&provider_path)?,
        &provider_identity(target, &provider_path)?,
        &producer,
        req.owners,
    )?;
    let record_source = stage.path().join("native/admission_record.c");
    let record_object = stage.path().join("admission_record.o");
    fs::write(&record_source, published::record_c_source(published::CONSUMER_RECORD_SYMBOL, &record.framed()?))
        .map_err(|error| invalid(error.to_string()))?;
    let record_compiler = super::compile_generated_c_object(target, &record_source, &record_object, &[], req.control)?;
    if record_compiler.sha256 != producer.adapter_compiler.sha256 {
        return Err(invalid("Glue admission record compiler differs from the adapter compiler"));
    }
    let record_object_sha256 = hash_file(&record_object)?;
    let native_path =
        stage.path().join(req.output_path.file_name().ok_or_else(|| invalid("invalid Glue output filename"))?);
    let linked = link_with_control(
        &LinkRequest {
            target_triple: Some(req.runtime.kit.target.triple.as_str().to_owned()),
            output_kind: BuildOutputKind::SharedLib,
            output_path: native_path,
            object_path: body_path.clone(),
            additional_object_paths: vec![adapter_path.clone(), record_object.clone()],
            runtime: Some(RuntimeLinkInput::try_from(&runtime)?),
            host_staticlib: None,
            entrypoint_symbol: String::new(),
            exported_symbols: consumer_exports(&packet.manifest, &producer),
            link_mode: LinkMode::Auto,
            verbose: false,
            external_libraries: runtime.platform_libraries.clone(),
            optional_libraries: Vec::new(),
            library_search_paths: vec![],
        },
        Some(req.control),
    )?;
    if hash_file(&body_path)? != producer.body_object_sha256
        || hash_file(&adapter_path)? != producer.adapter_object_sha256
        || hash_file(&source)? != producer.adapter_source_sha256
        || hash_file(&record_object)? != record_object_sha256
    {
        return Err(invalid("Glue native source or object changed during linking"));
    }
    producer.native_sha256 = hash_file(&linked.output_path)?;
    let mut admitted = prepare_glue_artifact(
        req.input,
        req.selected,
        req.native_library,
        &packet,
        req.runtime,
        &runtime,
        &linked,
        producer,
        &record,
    )?;
    // Atomic no-replace publication on the same filesystem, after complete validation.
    fs::hard_link(&admitted.issued.payload_path, req.output_path).map_err(|error| invalid(error.to_string()))?;
    admitted.issued.payload_path = fs::canonicalize(req.output_path).map_err(|error| invalid(error.to_string()))?;
    admitted.verify_native_closure()?;
    Ok(admitted)
}

impl PreparedGlueArtifact {
    pub fn packet(&self) -> &GlueArtifact {
        &self.issued.packet
    }
    pub fn native_path(&self) -> &Path {
        &self.issued.payload_path
    }
    pub fn native_sha256(&self) -> &str {
        &self.issued.payload_sha256
    }
    pub fn provider_path(&self) -> &Path {
        &self.issued.provider_path
    }
    pub fn provider_sha256(&self) -> &str {
        &self.issued.provider_sha256
    }
    pub fn link_tool(&self) -> &LinkToolReceipt {
        &self.link_tool
    }
    pub fn adapter_compiler(&self) -> &LinkToolReceipt {
        &self.issued.producer.adapter_compiler
    }
    pub fn producer_object_digests(&self) -> (&str, &str, &str) {
        let producer = &self.issued.producer;
        (&producer.body_object_sha256, &producer.adapter_object_sha256, &producer.adapter_source_sha256)
    }
    pub fn dependency_identity(&self) -> &str {
        &self.dependency_identity
    }

    /// Loader revalidation before retaining provider and foreign-image handles.
    pub fn verify_native_closure(&self) -> AotResult<()> {
        self.issued.verify_native_closure()
    }

    fn into_issued(self) -> IssuedGlueImage {
        self.issued
    }
}

/// Every symbol a consumer image must export: the public packet closure, the private
/// producer closure, and the embedded admission record.
fn consumer_exports(manifest: &beskid_codegen::glue::GlueManifest, producer: &GlueProducerWitness) -> Vec<String> {
    manifest
        .bindings
        .iter()
        .filter(|binding| binding.direction == "export")
        .map(beskid_codegen::glue::artifact::export_transport_symbol)
        .chain(beskid_codegen::glue::artifact::checked_invocation_symbols(manifest))
        .chain(beskid_codegen::glue::artifact::owned_release_symbols(manifest))
        .chain([
            GLUE_INITIALIZATION_SYMBOL.to_owned(),
            GLUE_IMPORT_BINDING_SYMBOL.to_owned(),
            published::CONSUMER_RECORD_SYMBOL.to_owned(),
        ])
        .chain(producer.failure_admissions.iter().cloned())
        .chain(producer.checked_entries.iter().map(|entry| entry.symbol.clone()))
        .chain(producer.dynamic_shapes.iter().map(|shape| shape.payload_descriptor_getter.clone()))
        .chain(producer.handle_transports.iter().flat_map(|transport| {
            [transport.constructor.clone(), transport.reader.clone(), transport.descriptor.clone()]
        }))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Loader identity of the canonical shared provider: `@rpath/<basename>` on Mach-O, else the basename.
fn provider_identity(target: &str, provider: &Path) -> AotResult<String> {
    let filename =
        provider.file_name().and_then(|name| name.to_str()).ok_or_else(|| invalid("invalid shared provider filename"))?;
    Ok(if target.contains("apple") { format!("@rpath/{filename}") } else { filename.to_owned() })
}

/// The image must depend on exactly one canonical shared runtime provider.
fn require_sole_provider(native: &object::File<'_>, identity: &str) -> AotResult<()> {
    let dependencies = loader_dependencies(native)?;
    if dependencies.iter().filter(|entry| *entry == identity).count() != 1
        || dependencies.iter().any(|entry| entry != identity && entry.contains("beskid_runtime"))
    {
        return Err(invalid("Glue image must depend on exactly one canonical shared runtime provider"));
    }
    Ok(())
}

fn require_exports<'a>(native: &object::File<'_>, symbols: impl IntoIterator<Item = &'a String>) -> AotResult<()> {
    let exports = native.exports().map_err(|error| invalid(error.to_string()))?;
    for symbol in symbols {
        let expected =
            if native.format() == object::BinaryFormat::MachO { format!("_{symbol}") } else { symbol.to_owned() };
        if !exports.iter().any(|export| export.name() == expected.as_bytes()) {
            return Err(invalid(format!("Glue required export is absent: {symbol}")));
        }
    }
    Ok(())
}

/// Parse a bounded native shared image and require the target's format, width, architecture and bounds.
fn target_shared_image<'d>(
    bytes: &'d [u8],
    target: &beskid_abi::abi_v5::TargetMetadata,
) -> AotResult<object::File<'d>> {
    let native = object::File::parse(bytes).map_err(|error| invalid(error.to_string()))?;
    if native.kind() != ObjectKind::Dynamic || native.is_64() != (target.pointer_width == 64) {
        return Err(invalid("Glue output is not a shared image with the required native width"));
    }
    let triple = target.triple.as_str();
    let expected_arch = if triple.starts_with("aarch64-") {
        object::Architecture::Aarch64
    } else if triple.starts_with("x86_64-") {
        object::Architecture::X86_64
    } else {
        return Err(invalid("unsupported Glue native architecture"));
    };
    let expected_format = if triple.contains("apple") {
        object::BinaryFormat::MachO
    } else if triple.contains("windows") {
        object::BinaryFormat::Pe
    } else {
        object::BinaryFormat::Elf
    };
    if native.architecture() != expected_arch || native.format() != expected_format {
        return Err(invalid("Glue native architecture or object format mismatch"));
    }
    if !native.is_little_endian()
        || native.symbols().take(65537).count() > 65536
        || native.dynamic_symbols().take(65537).count() > 65536
    {
        return Err(invalid("Glue native symbol table or byte order violates target bounds"));
    }
    macro_rules! bounded_pe_exports {
        ($pe:expr) => {{
            if let Some(table) = $pe.export_table().map_err(|error| invalid(error.to_string()))? {
                let directory = table.directory();
                if directory.number_of_names.get(object::LittleEndian) > 65536
                    || directory.number_of_functions.get(object::LittleEndian) > 65536
                {
                    return Err(invalid("Glue PE export table exceeds bound"));
                }
            }
        }};
    }
    match &native {
        object::File::Pe32(pe) => bounded_pe_exports!(pe),
        object::File::Pe64(pe) => bounded_pe_exports!(pe),
        _ => {}
    }
    Ok(native)
}

fn prepare_glue_artifact(
    input: &CodegenInput<'_>,
    selected: &[SyntaxModuleItem],
    native_library: &str,
    expected: &GlueArtifact,
    request: &RuntimeBuildRequest,
    runtime: &RuntimeArtifact,
    linked: &LinkResult,
    producer: GlueProducerWitness,
    record: &published::ConsumerRecord,
) -> AotResult<PreparedGlueArtifact> {
    if producer.syntax_generation.0 == 0 || producer.syntax_generation != input.typed_program().generation {
        return Err(invalid("Glue producer syntax generation differs from current assembly authority"));
    }
    let current =
        beskid_codegen::glue::emit(input, selected, native_library).map_err(|error| invalid(error.to_string()))?;
    if current != *expected {
        return Err(invalid("Glue packet differs from current registered assembly"));
    }
    if request.linkage != RuntimeLinkage::GlueSharedProviderV1 {
        return Err(invalid("Glue admission requires the canonical shared provider profile"));
    }
    let canonical = prepare_runtime(request)?;
    if canonical.linkage != runtime.linkage
        || canonical.link_path != runtime.link_path
        || canonical.shared_library_path != runtime.shared_library_path
        || canonical.owner_issuer_source_sha256 != runtime.owner_issuer_source_sha256
        || canonical.exported_symbols != runtime.exported_symbols
        || canonical.platform_libraries != runtime.platform_libraries
    {
        return Err(invalid("Glue runtime artifact differs from independently validated provider"));
    }
    if current.manifest.target != request.kit.target.triple.as_str()
        || current.manifest.runtime_abi != beskid_abi::BESKID_RUNTIME_ABI_VERSION
    {
        return Err(invalid("Glue target or runtime ABI does not match the provider"));
    }
    let receipt =
        linked.tool_receipt.clone().ok_or_else(|| invalid("Glue link lacks an exact successful tool receipt"))?;
    if hash_file(&receipt.executable)? != receipt.sha256 {
        return Err(invalid("Glue linker executable changed since the successful link"));
    }
    let payload_path = fs::canonicalize(&linked.output_path).map_err(|error| invalid(error.to_string()))?;
    let provider_path = fs::canonicalize(
        canonical.shared_library_path.as_ref().ok_or_else(|| invalid("Glue shared provider payload is missing"))?,
    )
    .map_err(|error| invalid(error.to_string()))?;
    let payload = bounded_file(&payload_path)?;
    let native = target_shared_image(&payload, &request.kit.target)?;
    require_exports(&native, &consumer_exports(&current.manifest, &producer))?;
    let identity = provider_identity(request.kit.target.triple.as_str(), &provider_path)?;
    require_sole_provider(&native, &identity)?;
    // The linked image must carry exactly the record this producer issued.
    let embedded: published::ConsumerRecord = published::read_record(&native, published::CONSUMER_RECORD_SYMBOL)?;
    if &embedded != record {
        return Err(invalid("Glue image admission record differs from the issued producer record"));
    }
    let admitted = PreparedGlueArtifact {
        issued: IssuedGlueImage {
            packet: current,
            payload_path,
            payload_sha256: hash(&payload),
            provider_sha256: hash_file(&provider_path)?,
            provider_path,
            producer,
        },
        link_tool: receipt,
        dependency_identity: identity,
    };
    if admitted.issued.payload_sha256 != admitted.issued.producer.native_sha256 {
        return Err(invalid("Glue native payload differs from privately issued producer witness"));
    }
    let compiler = &admitted.issued.producer.adapter_compiler;
    if hash_file(&compiler.executable)? != compiler.sha256 {
        return Err(invalid("Glue adapter compiler changed during production"));
    }
    if record.provider_sha256 != admitted.issued.provider_sha256 {
        return Err(invalid("Glue provider changed between admission record and link"));
    }
    admitted.verify_native_closure()?;
    Ok(admitted)
}

fn invalid(message: impl Into<String>) -> AotError {
    AotError::InvalidRequest { message: message.into() }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn bounded_file(path: &Path) -> AotResult<Vec<u8>> {
    let metadata = fs::metadata(path).map_err(|error| invalid(error.to_string()))?;
    if !metadata.is_file() || metadata.len() > 256 * 1024 * 1024 {
        return Err(invalid("Glue closure file is not a bounded regular file"));
    }
    let bytes = fs::read(path).map_err(|error| invalid(error.to_string()))?;
    if bytes.len() > 256 * 1024 * 1024 {
        return Err(invalid("Glue closure file grew beyond bound"));
    }
    Ok(bytes)
}
fn hash_file(path: &Path) -> AotResult<String> {
    Ok(hash(&bounded_file(path)?))
}

fn loader_dependencies(file: &object::File<'_>) -> AotResult<Vec<String>> {
    use object::read::elf::Dyn;
    let mut entries = Vec::new();
    macro_rules! elf {
        ($elf:expr) => {{
            let elf = $elf;
            let sections = elf.elf_section_table();
            if let Some((dynamic, index)) =
                sections.dynamic(elf.endian(), elf.data()).map_err(|error| invalid(error.to_string()))?
            {
                let strings =
                    sections.strings(elf.endian(), elf.data(), index).map_err(|error| invalid(error.to_string()))?;
                if dynamic.len() > 65536 {
                    return Err(invalid("Glue dynamic table exceeds bound"));
                }
                for entry in dynamic {
                    if entry.tag32(elf.endian()) == Some(object::elf::DT_NEEDED) {
                        append_dependency(
                            &mut entries,
                            entry.string(elf.endian(), strings).map_err(|error| invalid(error.to_string()))?,
                        )?;
                    }
                }
            }
        }};
    }
    macro_rules! macho {
        ($mach:expr) => {{
            let mach = $mach;
            let mut commands = mach.macho_load_commands().map_err(|error| invalid(error.to_string()))?;
            let mut count = 0usize;
            while let Some(command) = commands.next().map_err(|error| invalid(error.to_string()))? {
                count += 1;
                if count > 65536 {
                    return Err(invalid("Glue load command table exceeds bound"));
                }
                if command.cmd() == object::macho::LC_ID_DYLIB {
                    continue;
                }
                if let Some(dylib) = command.dylib().map_err(|error| invalid(error.to_string()))? {
                    append_dependency(
                        &mut entries,
                        command.string(mach.endian(), dylib.dylib.name).map_err(|error| invalid(error.to_string()))?,
                    )?;
                }
            }
        }};
    }
    macro_rules! pe {
        ($pe:expr) => {{
            if let Some(table) = $pe.import_table().map_err(|error| invalid(error.to_string()))? {
                let mut descriptors = table.descriptors().map_err(|error| invalid(error.to_string()))?;
                while let Some(descriptor) = descriptors.next().map_err(|error| invalid(error.to_string()))? {
                    append_dependency(
                        &mut entries,
                        table
                            .name(descriptor.name.get(object::LittleEndian))
                            .map_err(|error| invalid(error.to_string()))?,
                    )?;
                }
            }
        }};
    }
    match file {
        object::File::Elf32(elf) => elf!(elf),
        object::File::Elf64(elf) => elf!(elf),
        object::File::MachO32(mach) => macho!(mach),
        object::File::MachO64(mach) => macho!(mach),
        object::File::Pe32(pe) => pe!(pe),
        object::File::Pe64(pe) => pe!(pe),
        _ => return Err(invalid("Glue image has an unsupported loader format")),
    }
    if entries.len() > 4096 {
        return Err(invalid("Glue native dependency closure exceeds bound"));
    }
    Ok(entries)
}

fn append_dependency(entries: &mut Vec<String>, bytes: &[u8]) -> AotResult<()> {
    if entries.len() >= 4096 || bytes.is_empty() || bytes.len() > 4096 || bytes.contains(&0) {
        return Err(invalid("Glue loader dependency identity exceeds closed bounds"));
    }
    let name = std::str::from_utf8(bytes).map_err(|error| invalid(error.to_string()))?;
    entries.push(name.to_owned());
    Ok(())
}
