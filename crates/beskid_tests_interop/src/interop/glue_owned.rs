//! v0.6 Rust Glue owned end-to-end harness.
//!
//! Route: actual project manifest -> CompilePlan/prepared front end -> registered
//! codegen input -> private `build_rust_owner` and `build_glue_artifact` ->
//! `GlueProcess`. Owner sources come from the manifest `glue "glue_manual"` block
//! (`collect_glue_owner_sources`), and owner type and callable tables come only from
//! the fixture's `RustOwner` source facts (`beskid_queries::rust_owner_tables`), the
//! same route `beskid build --backend glue-rust` uses. The consumer identity is the
//! target name, as in the CLI. The producer runs only from a private copy of a qualified
//! installation prefix (`<prefix>/bin` holds the exact driver sibling and this
//! test executable; the runtime provider comes from the same prefix). It never
//! runs from `target/*/deps`.
//!
//! `BESKID_V06_QUALIFIED_PREFIX` must name the qualified installation prefix.
//! Absence is a test failure, never a skip. The test binary itself must be built
//! with `BESKID_COMPILER_DRIVER_BUILD_RECEIPT` for the same driver bytes that the
//! prefix ships; otherwise the driver selector fails closed and these tests fail.
//!
//! The old manual `native.rs`/`native_gate.py` registry route is not ownership
//! evidence and is not used here.
use beskid_aot::{
    api::{
        NativeExecutionControl, RuntimeKitRequest,
        glue::{
            GlueArtifactBuildRequest, GlueDeclaration, GlueProcess, PreparedGlueArtifact, PreparedRustOwner,
            PublishedGlue, PublishedGlueRequest, RustOwnerBuildRequest, RustOwnerCallable, RustOwnerSource,
            RustOwnerType, admit_published_glue_images, build_glue_artifact, build_rust_owner, glue_declarations,
            rust_owner_source_tables,
        },
    },
    runtime::{RuntimeBuildRequest, RuntimeLinkage},
};
use beskid_codegen::{
    CodegenInput,
    glue::{
        GlueArtifact,
        artifact::{checked_invocation_symbol, owned_release_symbol},
    },
    module_emission::SyntaxModuleItem,
};
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

const QUALIFIED_PREFIX_ENV: &str = "BESKID_V06_QUALIFIED_PREFIX";
const QUALIFIED_PROFILE_ENV: &str = "BESKID_V06_QUALIFIED_PROFILE";
const CARGO_ENV: &str = "BESKID_V06_GLUE_CARGO";
const RUSTC_ENV: &str = "BESKID_V06_GLUE_RUSTC";
const LINKER_ENV: &str = "BESKID_V06_GLUE_LINKER";
const CHILD_ENV: &str = "BESKID_V06_GLUE_CHILD";
const GATE_ENV: &str = "BESKID_V06_GLUE_BORROW_GATE";
const OWNER_LIBRARY: &str = "glue_manual";
/// Consumer native library identity: the target name, as `beskid build --backend glue-rust` uses.
const CONSUMER_LIBRARY: &str = "ManualOwned";
/// `RustOwner` type path the fixture declares on `ManualOpaque`.
const OWNER_TYPE_PATH: &str = "implementation::ManualOwned";
const BORROW_GATE_LENGTH: usize = 4093;
const OWNED_LIMIT: usize = 16 * 1024 * 1024;
const PROVIDER_BUSY: i32 = 4;


fn driver_name() -> &'static str {
    if cfg!(windows) { "beskid_native_tool_driver.exe" } else { "beskid_native_tool_driver" }
}

fn sha256_file(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))))
}

fn host_target() -> beskid_abi::abi_v5::TargetMetadata {
    beskid_abi::runtime_kit::host_runtime_target().expect("v0.6 Glue harness requires a supported native host")
}

fn qualified_profile() -> beskid_abi::runtime_kit::BuildProfile {
    match std::env::var(QUALIFIED_PROFILE_ENV).as_deref() {
        Err(_) | Ok("debug") => beskid_abi::runtime_kit::BuildProfile::Debug,
        Ok("release") => beskid_abi::runtime_kit::BuildProfile::Release,
        Ok(other) => panic!("{QUALIFIED_PROFILE_ENV} must be `debug` or `release`, not `{other}`"),
    }
}

fn kit_relative() -> PathBuf {
    Path::new(beskid_abi::runtime_kit::installed_runtime_root())
        .join(host_target().triple.as_str())
        .join(beskid_abi::runtime_kit::profile_directory_name(qualified_profile()))
}

/// Fails, never skips, when the qualified installation prefix is absent or incomplete.
fn require_qualified_prefix() -> PathBuf {
    let Some(raw) = std::env::var_os(QUALIFIED_PREFIX_ENV) else {
        panic!(
            "{QUALIFIED_PREFIX_ENV} is not set. The v0.6 Rust Glue harness needs a qualified installation prefix \
             (<prefix>/bin/{} and <prefix>/{}). It does not skip; CI must supply the prefix.",
            driver_name(),
            kit_relative().display()
        );
    };
    let prefix = fs::canonicalize(&raw)
        .unwrap_or_else(|error| panic!("{QUALIFIED_PREFIX_ENV}={}: {error}", PathBuf::from(&raw).display()));
    let driver = prefix.join("bin").join(driver_name());
    let metadata = fs::symlink_metadata(&driver)
        .unwrap_or_else(|error| panic!("qualified prefix lacks driver {}: {error}", driver.display()));
    assert!(metadata.is_file(), "qualified driver {} must be a regular file", driver.display());
    let kit = prefix.join(kit_relative());
    assert!(kit.join("abi.json").is_file(), "qualified prefix lacks runtime kit {}", kit.display());
    prefix
}

fn require_tool(name: &str) -> PathBuf {
    let raw = std::env::var_os(name)
        .unwrap_or_else(|| panic!("{name} is not set; the Rust owner build needs an explicit tool path"));
    fs::canonicalize(&raw).unwrap_or_else(|error| panic!("{name}={}: {error}", PathBuf::from(&raw).display()))
}

struct Tools {
    cargo: PathBuf,
    rustc: PathBuf,
    linker: PathBuf,
}
fn require_tools() -> Tools {
    Tools { cargo: require_tool(CARGO_ENV), rustc: require_tool(RUSTC_ENV), linker: require_tool(LINKER_ENV) }
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap_or_else(|error| panic!("read {}: {error}", source.display())) {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target);
        } else if kind.is_file() {
            fs::copy(entry.path(), &target).unwrap();
        } else {
            panic!("qualified runtime kit contains a non-regular entry: {}", entry.path().display());
        }
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DriverStaging {
    Exact,
    Tampered,
    Missing,
    #[cfg(unix)]
    Symlinked,
}

/// Private prefix: `bin/` holds a copy of this test executable and the driver
/// sibling; the exact runtime kit is copied from the qualified prefix.
fn stage_private_prefix(qualified: &Path, staging: DriverStaging) -> tempfile::TempDir {
    let staged = tempfile::Builder::new().prefix("beskid-v06-glue-prefix-").tempdir().unwrap();
    let bin = staged.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let driver = qualified.join("bin").join(driver_name());
    let staged_driver = bin.join(driver_name());
    match staging {
        DriverStaging::Exact => {
            fs::copy(&driver, &staged_driver).unwrap();
        }
        DriverStaging::Tampered => {
            let mut bytes = fs::read(&driver).unwrap();
            bytes.push(0);
            fs::write(&staged_driver, bytes).unwrap();
        }
        DriverStaging::Missing => {}
        #[cfg(unix)]
        DriverStaging::Symlinked => std::os::unix::fs::symlink(&driver, &staged_driver).unwrap(),
    }
    if staging == DriverStaging::Tampered {
        make_executable(&staged_driver);
    }
    copy_tree(&qualified.join(kit_relative()), &staged.path().join(kit_relative()));
    let executable = std::env::current_exe().unwrap();
    fs::copy(&executable, bin.join(executable.file_name().unwrap())).unwrap();
    staged
}

fn test_path(name: &str) -> String {
    let module = module_path!().split_once("::").map(|(_, rest)| rest).unwrap_or(module_path!());
    format!("{module}::{name}")
}

/// Runs `body` inside a child copy of this test executable placed in a private
/// qualified prefix. In the parent this stages the prefix and requires that the
/// child really executed exactly this test and passed.
fn run_in_private_prefix(name: &str, staging: DriverStaging, body: impl FnOnce(&Path)) {
    let path = test_path(name);
    if std::env::var(CHILD_ENV).as_deref() == Ok(path.as_str()) {
        let executable = fs::canonicalize(std::env::current_exe().unwrap()).unwrap();
        let bin = executable.parent().unwrap();
        assert_eq!(bin.file_name().and_then(|name| name.to_str()), Some("bin"), "child must run from <prefix>/bin");
        assert!(
            !executable.components().any(|component| component.as_os_str() == "deps"),
            "child must not run from a build tree: {}",
            executable.display()
        );
        body(bin.parent().unwrap());
        return;
    }
    let qualified = require_qualified_prefix();
    let staged = stage_private_prefix(&qualified, staging);
    let executable = staged.path().join("bin").join(std::env::current_exe().unwrap().file_name().unwrap());
    let corelib = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib");
    let output = Command::new(&executable)
        .args([path.as_str(), "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, &path)
        .env(GATE_ENV, staged.path().join("borrow.gate"))
        .env(beskid_abi::runtime_kit::ENV_CORELIB_ROOT, fs::canonicalize(corelib).unwrap())
        .env_remove(beskid_abi::runtime_kit::ENV_RUNTIME_PREFIX)
        .output()
        .unwrap_or_else(|error| panic!("spawn private-prefix child {}: {error}", executable.display()));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "private-prefix child {path} failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(
        stdout.contains("test result: ok. 1 passed"),
        "private-prefix child did not execute exactly {path}\nstdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Driver selector: immutable embedded digest + actual running executable prefix.

#[test]
fn v06_glue_driver_denied_from_build_tree() {
    let error = match beskid_abi::compiler_driver::installed_compiler_driver() {
        Ok(driver) => panic!("driver admitted from build tree: {}", driver.path().display()),
        Err(error) => error,
    };
    assert!(!error.is_empty());
}

#[test]
fn v06_glue_driver_admitted_only_from_private_prefix() {
    let qualified = std::env::var_os(QUALIFIED_PREFIX_ENV).map(PathBuf::from);
    run_in_private_prefix("v06_glue_driver_admitted_only_from_private_prefix", DriverStaging::Exact, |prefix| {
        let driver = beskid_abi::compiler_driver::installed_compiler_driver().unwrap_or_else(|error| {
            panic!("qualified driver rejected; build this test with BESKID_COMPILER_DRIVER_BUILD_RECEIPT: {error}")
        });
        let expected = fs::canonicalize(prefix.join("bin").join(driver_name())).unwrap();
        assert_eq!(driver.path(), expected);
        let qualified = fs::canonicalize(qualified.expect("child inherits the qualified prefix")).unwrap();
        assert_eq!(driver.sha256(), sha256_file(&qualified.join("bin").join(driver_name())));
        driver.verify().unwrap();
    });
}

#[test]
fn v06_glue_driver_denied_when_sibling_tampered() {
    run_in_private_prefix("v06_glue_driver_denied_when_sibling_tampered", DriverStaging::Tampered, |_| {
        let error = beskid_abi::compiler_driver::installed_compiler_driver().err().expect("tampered driver admitted");
        assert!(error.contains("installed compiler driver changed"), "{error}");
    });
}

#[test]
fn v06_glue_driver_denied_when_sibling_missing() {
    run_in_private_prefix("v06_glue_driver_denied_when_sibling_missing", DriverStaging::Missing, |_| {
        let error = beskid_abi::compiler_driver::installed_compiler_driver().err().expect("missing driver admitted");
        assert!(!error.contains("independently qualified native tool driver"), "unqualified test build: {error}");
    });
}

#[cfg(unix)]
#[test]
fn v06_glue_driver_denied_when_sibling_escapes_prefix() {
    run_in_private_prefix("v06_glue_driver_denied_when_sibling_escapes_prefix", DriverStaging::Symlinked, |_| {
        let error = beskid_abi::compiler_driver::installed_compiler_driver().err().expect("symlinked driver admitted");
        // A symlinked sibling is not a regular file, so admission denies it before any prefix resolution.
        assert!(error.contains("installed compiler driver must be a bounded regular file"), "{error}");
    });
}

// ---------------------------------------------------------------------------
// Actual project -> CompilePlan -> prepared front end -> Glue producers.

/// Copies the owned fixture project into `root`, binding its corelib path
/// dependency to this checkout. Fixture sources stay unmodified.
fn materialize_owned_project(root: &Path) -> PathBuf {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/glue/manual/owned");
    let project = root.join("manual_owned");
    fs::create_dir_all(&project).unwrap();
    for name in ["ManualTypes.bd", "ManualImport.bd", "ManualExport.bd"] {
        fs::copy(fixture.join(name), project.join(name)).unwrap();
    }
    fs::create_dir_all(project.join("rust")).unwrap();
    fs::copy(fixture.join("rust/implementation.rs"), project.join("rust/implementation.rs")).unwrap();
    let relative = "\"../../../../../../corelib/beskid_corelib\"";
    let manifest = fs::read_to_string(fixture.join("manual_owned.bproj")).unwrap();
    assert!(manifest.contains(relative), "owned manifest corelib dependency changed");
    let corelib = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib/beskid_corelib")).unwrap();
    let corelib = corelib.to_str().expect("UTF-8 corelib path").replace('\\', "/");
    fs::write(project.join("manual_owned.bproj"), manifest.replace(relative, &format!("{corelib:?}"))).unwrap();
    project.join("manual_owned.bproj")
}

/// Actual CompilePlan and prepare spine, then the registered codegen input.
fn with_owned_glue_input<T>(
    manifest: &Path,
    execute: impl FnOnce(&CodegenInput<'_>, &[SyntaxModuleItem]) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let resolved = beskid_analysis::services::resolve_input(
        None,
        Some(&manifest.to_path_buf()),
        Some("ManualOwned"),
        None,
        beskid_analysis::projects::WorkspacePrepareOptions { offline: true, ..Default::default() },
    )?;
    let plan = resolved.compile_plan.as_ref().ok_or_else(|| anyhow::anyhow!("owned fixture has no CompilePlan"))?;
    anyhow::ensure!(plan.project_name == "manual_owned" && plan.target.name == "ManualOwned");
    let front = beskid_queries::prepare_compilation(
        &resolved,
        beskid_analysis::services::PrepareOptions {
            front_end: beskid_analysis::services::FrontEndOptions {
                with_semantic_diagnostics: true,
                ..Default::default()
            },
            ..Default::default()
        },
        None,
    )?
    .into_executable()?;
    beskid_aot::with_prepared_glue_input(&front, host_target(), execute)
}

/// Imports the fixture declares `[RustOwner(Fallible:true)]`; the source-derived tables must agree.
const EXPECTED_FALLIBLE: &[&str] = &[
    "glue_bytes",
    "glue_utf8",
    "glue_failure",
    "glue_utf8_exact",
    "glue_bytes_exact",
    "make_manual_owned",
    "check_manual_owned",
];
const IMPORT_SYMBOLS: &[&str] = &[
    "glue_i8",
    "glue_i16",
    "glue_i32",
    "glue_i64",
    "glue_u8",
    "glue_u16",
    "glue_u32",
    "glue_u64",
    "glue_f32",
    "glue_f64",
    "glue_bool",
    "glue_char",
    "glue_utf8",
    "glue_bytes",
    "glue_native_width",
    "glue_unit",
    "make_manual_owned",
    "check_manual_owned",
    "glue_failure",
    "glue_f32_bits",
    "glue_f64_bits",
    "glue_f32_from_bits",
    "glue_f64_from_bits",
    "glue_utf8_exact",
    "glue_bytes_exact",
];

#[derive(Clone, Copy)]
enum OwnerVariant {
    Complete,
    /// Omits one import row; the consumer must not admit its imports.
    MissingRow(&'static str),
    /// Declares an owner type whose brand is not in the source signature.
    ForeignBrand,
}

struct OwnedBuild {
    owner: PreparedRustOwner,
    consumer: PreparedGlueArtifact,
    packet: GlueArtifact,
}

fn control() -> NativeExecutionControl {
    NativeExecutionControl::new(Instant::now() + Duration::from_secs(900), Arc::new(|| false))
}

fn runtime_request(prefix: &Path) -> RuntimeBuildRequest {
    RuntimeBuildRequest {
        kit: RuntimeKitRequest { prefix: prefix.to_owned(), target: host_target(), profile: qualified_profile() },
        linkage: RuntimeLinkage::GlueSharedProviderV1,
    }
}

/// Owner sources from the manifest `glue "glue_manual"` block of the materialized project.
fn manifest_owner_sources(manifest: &Path) -> anyhow::Result<Vec<RustOwnerSource>> {
    let project = beskid_analysis::projects::load_manifest_from_path(manifest)?;
    let owner = project
        .glue
        .iter()
        .find(|owner| owner.library == OWNER_LIBRARY)
        .ok_or_else(|| anyhow::anyhow!("owned fixture manifest lacks glue \"{OWNER_LIBRARY}\""))?;
    let root = manifest.parent().ok_or_else(|| anyhow::anyhow!("manifest has no parent directory"))?;
    Ok(beskid_analysis::projects::collect_glue_owner_sources(root, owner)?
        .files
        .into_iter()
        .map(|file| RustOwnerSource { relative_path: file.relative_path, bytes: file.bytes })
        .collect())
}

fn build_owner(
    input: &CodegenInput<'_>,
    imports: &[GlueDeclaration],
    sources: &[RustOwnerSource],
    prefix: &Path,
    tools: &Tools,
    output: &Path,
    variant: OwnerVariant,
) -> anyhow::Result<PreparedRustOwner> {
    let selected = imports.iter().map(|declaration| declaration.item.clone()).collect::<Vec<_>>();
    let packet = beskid_codegen::glue::emit(input, &selected, OWNER_LIBRARY)?;
    packet.validate()?;
    let imported = packet.manifest.bindings.iter().filter(|binding| binding.direction == "import").collect::<Vec<_>>();
    let mut symbols = imported.iter().map(|binding| binding.symbol.as_str()).collect::<Vec<_>>();
    symbols.sort_unstable();
    let mut expected = IMPORT_SYMBOLS.to_vec();
    expected.sort_unstable();
    anyhow::ensure!(symbols == expected, "owned import rows differ from the fixture: {symbols:?}");
    let factory = imported
        .iter()
        .find(|binding| binding.symbol == "make_manual_owned")
        .and_then(|binding| binding.result.opaque.as_ref())
        .ok_or_else(|| anyhow::anyhow!("ManualOpaque factory has no branded opaque result"))?;
    anyhow::ensure!(factory.library == OWNER_LIBRARY && !factory.nullable);

    let tables = rust_owner_source_tables(input, OWNER_LIBRARY, imports, &packet)?;
    anyhow::ensure!(tables.library == OWNER_LIBRARY);
    anyhow::ensure!(
        tables.types.len() == 1
            && tables.types[0].brand_sha256 == factory.brand_sha256
            && tables.types[0].rust_type_path == OWNER_TYPE_PATH,
        "source-derived owner types differ from the fixture: {:?}",
        tables.types
    );
    let symbol_of = |identity: &str| {
        imported
            .iter()
            .find(|binding| binding.identity_sha256 == identity)
            .map(|binding| binding.symbol.as_str())
            .unwrap_or_else(|| panic!("owner table row {identity} is not an issued import"))
    };
    anyhow::ensure!(tables.callables.len() == imported.len(), "owner tables do not cover every import row");
    for row in &tables.callables {
        let symbol = symbol_of(&row.binding_identity_sha256);
        anyhow::ensure!(
            row.rust_callable_path == format!("implementation::{symbol}"),
            "{symbol}: default RustOwner path expected, got {}",
            row.rust_callable_path
        );
        anyhow::ensure!(
            row.fallible == EXPECTED_FALLIBLE.contains(&symbol),
            "{symbol}: source fallibility {} differs from the fixture",
            row.fallible
        );
    }

    // Negative variants perturb the source-derived tables; they never supply mapping data.
    let types = tables
        .types
        .into_iter()
        .map(|row| RustOwnerType {
            brand_sha256: match variant {
                OwnerVariant::ForeignBrand => "0".repeat(64),
                _ => row.brand_sha256,
            },
            rust_type_path: row.rust_type_path,
        })
        .collect::<Vec<_>>();
    let callables = tables
        .callables
        .into_iter()
        .filter(|row| {
            !matches!(variant, OwnerVariant::MissingRow(missing) if missing == symbol_of(&row.binding_identity_sha256))
        })
        .map(RustOwnerCallable::from)
        .collect::<Vec<_>>();
    let runtime = runtime_request(prefix);
    let control = control();
    Ok(build_rust_owner(&RustOwnerBuildRequest {
        input,
        selected: &selected,
        native_library: OWNER_LIBRARY,
        expected: &packet,
        sources,
        types: &types,
        callables: &callables,
        runtime: &runtime,
        cargo: &tools.cargo,
        rustc: &tools.rustc,
        linker: &tools.linker,
        release: false,
        output_path: output,
        control: &control,
    })?)
}

fn build_consumer(
    input: &CodegenInput<'_>,
    all_items: &[SyntaxModuleItem],
    selected: &[SyntaxModuleItem],
    prefix: &Path,
    output: &Path,
    owner: &PreparedRustOwner,
) -> anyhow::Result<(PreparedGlueArtifact, GlueArtifact)> {
    let packet = beskid_codegen::glue::emit(input, selected, CONSUMER_LIBRARY)?;
    packet.validate()?;
    let runtime = runtime_request(prefix);
    let control = control();
    let artifact = build_glue_artifact(&GlueArtifactBuildRequest {
        input,
        all_items,
        selected,
        native_library: CONSUMER_LIBRARY,
        expected: &packet,
        runtime: &runtime,
        output_path: output,
        control: &control,
        owners: &[owner],
    })?;
    Ok((artifact, packet))
}

fn native_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{stem}.dylib")
    } else {
        format!("lib{stem}.so")
    }
}

fn build_owned(prefix: &Path, work: &Path, variant: OwnerVariant) -> anyhow::Result<OwnedBuild> {
    let tools = require_tools();
    fs::create_dir_all(work)?;
    let manifest = materialize_owned_project(work);
    let sources = manifest_owner_sources(&manifest)?;
    with_owned_glue_input(&manifest, |input, items| {
        let declarations = glue_declarations(input, items, &[OWNER_LIBRARY.to_owned()])?;
        let imports =
            declarations.iter().filter(|declaration| declaration.is_import_of(OWNER_LIBRARY)).cloned().collect::<Vec<_>>();
        let selected = declarations.iter().map(|declaration| declaration.item.clone()).collect::<Vec<_>>();
        let owner =
            build_owner(input, &imports, &sources, prefix, &tools, &work.join(native_name(OWNER_LIBRARY)), variant)?;
        let (consumer, packet) =
            build_consumer(input, items, &selected, prefix, &work.join(native_name(CONSUMER_LIBRARY)), &owner)?;
        Ok(OwnedBuild { owner, consumer, packet })
    })
}

// ---------------------------------------------------------------------------
// Native call helpers over the admitted process. Only issued symbols resolve.

#[repr(C)]
struct GlueOwnedView {
    pointer: *const u8,
    length: usize,
    token: u64,
}
impl GlueOwnedView {
    fn empty() -> Self {
        Self { pointer: std::ptr::null(), length: 0, token: 0 }
    }
}

fn export_binding<'a>(packet: &'a GlueArtifact, symbol: &str) -> &'a beskid_codegen::glue::GlueBindingManifest {
    packet
        .manifest
        .bindings
        .iter()
        .find(|binding| binding.direction == "export" && binding.symbol == symbol)
        .unwrap_or_else(|| panic!("export row {symbol} absent from the issued packet"))
}

/// # Safety
/// `F` must be the checked status/out signature issued for `symbol`.
unsafe fn checked<F: Copy + 'static>(process: &GlueProcess, packet: &GlueArtifact, symbol: &str) -> F {
    let name = CString::new(checked_invocation_symbol(export_binding(packet, symbol))).unwrap();
    *unsafe { process.export::<F>(0, &name) }.unwrap_or_else(|error| panic!("{symbol}: {error}"))
}

fn checked_error(process: &GlueProcess, packet: &GlueArtifact, symbol: &str) -> String {
    let name = CString::new(checked_invocation_symbol(export_binding(packet, symbol))).unwrap();
    match unsafe { process.export::<unsafe extern "C" fn()>(0, &name) } {
        Ok(_) => panic!("{symbol} resolved without admitted dependencies"),
        Err(error) => error.to_string(),
    }
}

/// # Safety
/// `F` must be the generated Rust owner signature for import `symbol`.
unsafe fn owner_entry<F: Copy + 'static>(
    process: &GlueProcess,
    image: usize,
    packet: &GlueArtifact,
    symbol: &str,
) -> F {
    let binding = packet
        .manifest
        .bindings
        .iter()
        .find(|binding| binding.direction == "import" && binding.symbol == symbol)
        .unwrap_or_else(|| panic!("import row {symbol} absent"));
    *unsafe { process.rust_owner_export::<F>(image, &binding.identity_sha256) }
        .unwrap_or_else(|error| panic!("{symbol}: {error}"))
}

fn scalar_rows<T: Copy + std::fmt::Debug + 'static>(
    process: &GlueProcess,
    packet: &GlueArtifact,
    symbols: [&str; 2],
    values: &[T],
    zero: T,
    same: impl Fn(T, T) -> bool,
) {
    for symbol in symbols {
        let call: unsafe extern "C" fn(T, *mut T) -> i32 = unsafe { checked(process, packet, symbol) };
        for value in values {
            let mut out = zero;
            let status = unsafe { call(*value, &mut out) };
            assert_eq!(status, 0, "{symbol}({value:?}) status");
            assert!(same(*value, out), "{symbol}({value:?}) returned {out:?}");
        }
    }
}

fn rejected_rows<T: Copy + std::fmt::Debug + 'static>(
    process: &GlueProcess,
    packet: &GlueArtifact,
    symbols: [&str; 2],
    values: &[T],
    zero: T,
) {
    for symbol in symbols {
        let call: unsafe extern "C" fn(T, *mut T) -> i32 = unsafe { checked(process, packet, symbol) };
        for value in values {
            let mut out = zero;
            assert_ne!(unsafe { call(*value, &mut out) }, 0, "{symbol} accepted invalid {value:?}");
        }
    }
}

fn view_rows(process: &GlueProcess, packet: &GlueArtifact, symbols: [&str; 2], valid: &[&[u8]], invalid: &[&[u8]]) {
    type View = unsafe extern "C" fn(*const u8, usize, *mut GlueOwnedView) -> i32;
    type Release = unsafe extern "C" fn(u64) -> i32;
    for symbol in symbols {
        let call: View = unsafe { checked(process, packet, symbol) };
        let release_name = CString::new(owned_release_symbol(&export_binding(packet, symbol).library)).unwrap();
        let release: Release =
            *unsafe { process.export::<Release>(0, &release_name) }.unwrap_or_else(|error| panic!("{error}"));
        for value in valid {
            let pointer = if value.is_empty() { std::ptr::null() } else { value.as_ptr() };
            let mut out = GlueOwnedView::empty();
            assert_eq!(unsafe { call(pointer, value.len(), &mut out) }, 0, "{symbol}({value:?})");
            assert_ne!(out.token, 0, "{symbol} owned result has no release token");
            let copied = if out.length == 0 {
                Vec::new()
            } else {
                unsafe { std::slice::from_raw_parts(out.pointer, out.length) }.to_vec()
            };
            assert_eq!(copied.as_slice(), *value, "{symbol} roundtrip");
            assert_eq!(unsafe { release(out.token) }, 0, "{symbol} release");
            assert_ne!(unsafe { release(out.token) }, 0, "{symbol} released twice");
        }
        for value in invalid {
            let mut out = GlueOwnedView::empty();
            assert_ne!(unsafe { call(value.as_ptr(), value.len(), &mut out) }, 0, "{symbol} accepted {value:?}");
            assert_eq!(out.token, 0, "{symbol} leaked a token on rejection");
        }
        let mut out = GlueOwnedView::empty();
        assert_ne!(unsafe { call(std::ptr::null(), 1, &mut out) }, 0, "{symbol} accepted null with length");
    }
}

type Create = unsafe extern "C" fn(usize, *mut u64) -> i32;
type Inspect = unsafe extern "C" fn(u64, *mut usize) -> i32;
type Handle = unsafe extern "C" fn(u64, *mut u64) -> i32;
type Release = unsafe extern "C" fn(u64) -> i32;

fn opaque_release(process: &GlueProcess, packet: &GlueArtifact) -> Release {
    let binding = export_binding(packet, "manual_ImportCreate");
    let brand = &binding.result.opaque.as_ref().expect("ImportCreate returns an opaque").brand_sha256;
    let name = CString::new(format!("beskid_glue_opaque_{brand}_release")).unwrap();
    *unsafe { process.export::<Release>(0, &name) }.unwrap_or_else(|error| panic!("{error}"))
}

fn open_owned(build: OwnedBuild) -> (GlueProcess, usize, GlueArtifact) {
    let OwnedBuild { owner, consumer, packet } = build;
    assert_eq!(consumer.packet(), &packet);
    let mut process = GlueProcess::open(consumer).unwrap_or_else(|error| panic!("open consumer: {error}"));
    let image = process.attach_rust_owner(owner).unwrap_or_else(|error| panic!("attach owner: {error}"));
    (process, image, packet)
}

/// The manual owned results, through an admitted consumer image 0 and Rust owner image `owner`.
/// Shared by the in-process producer route and the published `beskid build` route.
fn assert_owned_roundtrip(process: &GlueProcess, owner: usize, packet: &GlueArtifact) {
    scalar_rows(process, packet, ["manual_ImportI8", "beskid_i8"], &[i8::MIN, -1, 0, i8::MAX], 0, |a, b| a == b);
    scalar_rows(process, packet, ["manual_ImportI16", "beskid_i16"], &[i16::MIN, 0, i16::MAX], 0, |a, b| a == b);
    scalar_rows(process, packet, ["manual_ImportI32", "beskid_i32"], &[i32::MIN, 0, i32::MAX], 0, |a, b| a == b);
    scalar_rows(process, packet, ["manual_ImportI64", "beskid_i64"], &[i64::MIN, 0, i64::MAX], 0, |a, b| a == b);
    scalar_rows(process, packet, ["manual_ImportU8", "beskid_u8"], &[0, 128, u8::MAX], 0, |a, b| a == b);
    scalar_rows(process, packet, ["manual_ImportU16", "beskid_u16"], &[0, u16::MAX], 0, |a, b| a == b);
    scalar_rows(process, packet, ["manual_ImportU32", "beskid_u32"], &[0, u32::MAX], 0, |a, b| a == b);
    scalar_rows(process, packet, ["manual_ImportU64", "beskid_u64"], &[0, u64::MAX], 0, |a, b| a == b);
    let f32_values = [0.0, -0.0, f32::MIN, f32::MAX, f32::INFINITY, f32::NEG_INFINITY, f32::from_bits(0x7fc0_1234)];
    scalar_rows(process, packet, ["manual_ImportF32", "beskid_f32"], &f32_values, 0.0, |a, b| {
        a.to_bits() == b.to_bits()
    });
    let f64_values =
        [0.0, -0.0, f64::MIN, f64::MAX, f64::INFINITY, f64::NEG_INFINITY, f64::from_bits(0x7ff8_0000_0000_1234)];
    scalar_rows(process, packet, ["manual_ImportF64", "beskid_f64"], &f64_values, 0.0, |a, b| {
        a.to_bits() == b.to_bits()
    });
    scalar_rows::<u8>(process, packet, ["manual_ImportBool", "beskid_bool"], &[0, 1], 0, |a, b| a == b);
    rejected_rows::<u8>(process, packet, ["manual_ImportBool", "beskid_bool"], &[2, 255], 0);
    scalar_rows::<u32>(
        process,
        packet,
        ["manual_ImportChar", "beskid_char"],
        &[0, 0x41, 0xd7ff, 0xe000, 0x1f600, 0x10ffff],
        0,
        |a, b| a == b,
    );
    rejected_rows::<u32>(process, packet, ["manual_ImportChar", "beskid_char"], &[0xd800, 0xdfff, 0x110000], 0);
    scalar_rows::<usize>(
        process,
        packet,
        ["manual_ImportNativeWidth", "beskid_native_width"],
        &[0, usize::MAX],
        0,
        |a, b| a == b,
    );
    for symbol in ["manual_ImportUnit", "beskid_unit"] {
        let call: unsafe extern "C" fn() -> i32 = unsafe { checked(process, packet, symbol) };
        assert_eq!(unsafe { call() }, 0, "{symbol}");
    }
    view_rows(
        process,
        packet,
        ["manual_ImportUtf8", "beskid_utf8"],
        &[b"A\0\xf0\x9f\x98\x80".as_slice(), b"".as_slice()],
        &[b"\xff".as_slice(), b"\xed\xa0\x80".as_slice()],
    );
    view_rows(
        process,
        packet,
        ["manual_ImportBytes", "beskid_bytes"],
        &[[0u8, 128, 255, 65].as_slice(), [].as_slice()],
        &[],
    );

    // Opaque handle: owner factory, borrowed reader, Beskid identity export, consumer release.
    let create: Create = unsafe { checked(process, packet, "manual_ImportCreate") };
    let inspect: Inspect = unsafe { checked(process, packet, "manual_ImportInspect") };
    let identity: Handle = unsafe { checked(process, packet, "beskid_handle") };
    let release = opaque_release(process, packet);
    let mut token = 0;
    assert_eq!(unsafe { create(16, &mut token) }, 0);
    assert_ne!(token, 0);
    let mut length = 0;
    assert_eq!(unsafe { inspect(token, &mut length) }, 0);
    assert_eq!(length, 16);
    let mut returned = 0;
    assert_eq!(unsafe { identity(token, &mut returned) }, 0);
    assert_ne!(returned, 0);
    let mut length = 0;
    assert_eq!(unsafe { inspect(returned, &mut length) }, 0);
    assert_eq!(length, 16);
    let mut rejected = u64::MAX;
    assert_eq!(unsafe { create(OWNED_LIMIT + 1, &mut rejected) }, 1, "owner limit error status");
    assert_eq!(rejected, 0, "rejected factory leaked a token");
    assert_eq!(unsafe { release(token) }, 0);
    assert_ne!(unsafe { inspect(token, &mut length) }, 0, "stale token accepted");
    assert_ne!(unsafe { release(token) }, 0, "double release accepted");
    assert_ne!(unsafe { identity(token, &mut returned) }, 0, "stale token accepted by export");

    // Rows without a Beskid wrapper run through the owner's issued entry.
    let failure: unsafe extern "C" fn(u32, *mut i32) -> i32 =
        unsafe { owner_entry(process, owner, packet, "glue_failure") };
    let mut out = -1;
    assert_eq!(unsafe { failure(0, &mut out) }, 0);
    assert_eq!(out, 0);
    assert_eq!(unsafe { failure(1, &mut out) }, 6, "declared error status");
    assert_eq!(unsafe { failure(2, &mut out) }, 7, "contained panic status");
    let f32_bits: unsafe extern "C" fn(f32, *mut u32) -> i32 =
        unsafe { owner_entry(process, owner, packet, "glue_f32_bits") };
    let mut bits = 0;
    assert_eq!(unsafe { f32_bits(-0.0, &mut bits) }, 0);
    assert_eq!(bits, 0x8000_0000);
    let f64_from_bits: unsafe extern "C" fn(u64, *mut f64) -> i32 =
        unsafe { owner_entry(process, owner, packet, "glue_f64_from_bits") };
    let mut value = 0.0;
    assert_eq!(unsafe { f64_from_bits(0x7ff8_0000_0000_1234, &mut value) }, 0);
    assert_eq!(value.to_bits(), 0x7ff8_0000_0000_1234);
    let utf8_exact: unsafe extern "C" fn(*const u8, usize, *mut i32) -> i32 =
        unsafe { owner_entry(process, owner, packet, "glue_utf8_exact") };
    let exact = b"A\0\xf0\x9f\x98\x80";
    assert_eq!(unsafe { utf8_exact(exact.as_ptr(), exact.len(), &mut out) }, 0);
    assert_eq!(unsafe { utf8_exact(b"\xff".as_ptr(), 1, &mut out) }, 1, "invalid UTF-8 reached the owner");
    let bytes_exact: unsafe extern "C" fn(*const u8, usize, *mut i32) -> i32 =
        unsafe { owner_entry(process, owner, packet, "glue_bytes_exact") };
    assert_eq!(unsafe { bytes_exact([0u8, 128, 255, 65].as_ptr(), 4, &mut out) }, 0);
}

// ---------------------------------------------------------------------------
// End-to-end behaviour.

#[test]
fn v06_glue_owned_roundtrip_all_representations() {
    run_in_private_prefix("v06_glue_owned_roundtrip_all_representations", DriverStaging::Exact, |prefix| {
        let build = build_owned(prefix, &prefix.join("work"), OwnerVariant::Complete)
            .unwrap_or_else(|error| panic!("{error:#}"));
        let (process, owner, packet) = open_owned(build);
        assert_owned_roundtrip(&process, owner, &packet);
        drop(process);
    });
}

#[test]
fn v06_glue_owned_opaque_borrow_survives_release() {
    run_in_private_prefix("v06_glue_owned_opaque_borrow_survives_release", DriverStaging::Exact, |prefix| {
        let gate = PathBuf::from(std::env::var_os(GATE_ENV).expect("parent supplies the borrow gate"));
        let build = build_owned(prefix, &prefix.join("work"), OwnerVariant::Complete)
            .unwrap_or_else(|error| panic!("{error:#}"));
        let (process, _owner, packet) = open_owned(build);
        let create: Create = unsafe { checked(&process, &packet, "manual_ImportCreate") };
        let inspect: Inspect = unsafe { checked(&process, &packet, "manual_ImportInspect") };
        let release = opaque_release(&process, &packet);
        let mut token = 0;
        assert_eq!(unsafe { create(BORROW_GATE_LENGTH, &mut token) }, 0);
        let entered = gate.with_extension("entered");
        let releaser = {
            let gate = gate.clone();
            std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(60);
                while !entered.exists() {
                    assert!(Instant::now() < deadline, "owner borrow never became live");
                    std::thread::sleep(Duration::from_millis(5));
                }
                // The borrow lease is live inside the owner; release must not tear it down.
                let first = unsafe { release(token) };
                let second = unsafe { release(token) };
                fs::write(&gate, b"").unwrap();
                (first, second)
            })
        };
        let mut length = 0;
        let status = unsafe { inspect(token, &mut length) };
        let (first, second) = releaser.join().expect("releaser thread");
        assert_eq!(status, 0, "borrowed call failed after a concurrent release attempt");
        assert_eq!(length, BORROW_GATE_LENGTH, "borrowed payload changed under release");
        assert_eq!(first, PROVIDER_BUSY, "release during a live borrow must report busy");
        assert_eq!(second, PROVIDER_BUSY, "repeated release during a live borrow must report busy");
        // The lease ended with the call; the owner may now release exactly once.
        assert_eq!(unsafe { release(token) }, 0);
        assert_ne!(unsafe { inspect(token, &mut length) }, 0, "released token still borrowable");
        assert_ne!(unsafe { release(token) }, 0, "double release accepted");
        drop(process);
    });
}

#[test]
fn v06_glue_owned_denies_missing_import_row() {
    run_in_private_prefix("v06_glue_owned_denies_missing_import_row", DriverStaging::Exact, |prefix| {
        let build = build_owned(prefix, &prefix.join("work"), OwnerVariant::MissingRow("glue_unit"))
            .unwrap_or_else(|error| panic!("{error:#}"));
        let (process, owner, packet) = open_owned(build);
        for symbol in ["manual_ImportI8", "manual_ImportUnit", "beskid_i8", "manual_ImportCreate"] {
            let error = checked_error(&process, &packet, symbol);
            assert!(error.contains("not privately admitted"), "{symbol}: {error}");
        }
        let binding = packet
            .manifest
            .bindings
            .iter()
            .find(|binding| binding.direction == "import" && binding.symbol == "glue_unit")
            .unwrap();
        let missing =
            unsafe { process.rust_owner_export::<unsafe extern "C" fn() -> i32>(owner, &binding.identity_sha256) };
        assert!(missing.is_err(), "owner exposed a row outside its issued closure");
    });
}

#[test]
fn v06_glue_owned_denies_consumer_without_owner_domain() {
    run_in_private_prefix("v06_glue_owned_denies_consumer_without_owner_domain", DriverStaging::Exact, |prefix| {
        let OwnedBuild { owner, consumer, packet } = build_owned(prefix, &prefix.join("work"), OwnerVariant::Complete)
            .unwrap_or_else(|error| panic!("{error:#}"));
        drop(owner);
        let process = GlueProcess::open(consumer).unwrap_or_else(|error| panic!("open consumer: {error}"));
        for symbol in ["manual_ImportCreate", "manual_ImportInspect", "beskid_handle", "beskid_i8"] {
            let error = checked_error(&process, &packet, symbol);
            assert!(error.contains("not privately admitted"), "{symbol}: {error}");
        }
    });
}

#[test]
fn v06_glue_owned_denies_duplicate_owner_library() {
    run_in_private_prefix("v06_glue_owned_denies_duplicate_owner_library", DriverStaging::Exact, |prefix| {
        let build = build_owned(prefix, &prefix.join("work"), OwnerVariant::Complete)
            .unwrap_or_else(|error| panic!("{error:#}"));
        let (mut process, _owner, _packet) = open_owned(build);
        let again = build_owned(prefix, &prefix.join("second"), OwnerVariant::Complete);
        let OwnedBuild { owner, .. } = match again {
            Ok(build) => build,
            Err(error) => panic!("{error:#}"),
        };
        let error = process.attach_rust_owner(owner).err().expect("second owner for one library admitted");
        assert!(error.to_string().contains("already admitted"), "{error}");
    });
}

#[test]
fn v06_glue_owned_rejects_foreign_owner_brand() {
    run_in_private_prefix("v06_glue_owned_rejects_foreign_owner_brand", DriverStaging::Exact, |prefix| {
        let error = match build_owned(prefix, &prefix.join("work"), OwnerVariant::ForeignBrand) {
            Ok(_) => panic!("owner with a brand outside the source signature was admitted"),
            Err(error) => format!("{error:#}"),
        };
        assert!(error.contains("brand absent from current canonical signature"), "{error}");
    });
}

/// From the build tree there is no qualified driver sibling: the private Rust
/// owner producer must fail closed even with every other input valid.
#[test]
fn v06_glue_owned_rust_owner_fails_closed_outside_prefix() {
    let qualified = require_qualified_prefix();
    let work = tempfile::Builder::new().prefix("beskid-v06-glue-unqualified-").tempdir().unwrap();
    let staged = work.path().join("kit");
    copy_tree(&qualified.join(kit_relative()), &staged.join(kit_relative()));
    let error = match build_owned(&staged, &work.path().join("work"), OwnerVariant::Complete) {
        Ok(_) => panic!("Rust owner built without a qualified driver receipt"),
        Err(error) => format!("{error:#}"),
    };
    assert!(
        error.contains("not installed under `<prefix>/bin`")
            || error.contains("independently qualified native tool driver"),
        "unexpected failure (must be driver admission): {error}"
    );
}

// ---------------------------------------------------------------------------
// `beskid build --backend glue-rust` through the qualified installed CLI alone.
//
// These cases run `<qualified prefix>/bin/beskid` as a separate process. The harness only
// materializes the fixture project and inspects the published files; it constructs no owner
// tables, sources or witnesses. They fail, never skip, when the qualified prefix or the explicit
// Rust tools are absent.

const STAGE_PREFIX: &str = ".beskid-glue-build-";

fn installed_cli(qualified: &Path) -> PathBuf {
    let cli = qualified.join("bin").join(if cfg!(windows) { "beskid.exe" } else { "beskid" });
    let metadata =
        fs::symlink_metadata(&cli).unwrap_or_else(|error| panic!("qualified prefix lacks CLI {}: {error}", cli.display()));
    assert!(metadata.is_file(), "qualified CLI {} must be a regular file", cli.display());
    cli
}

/// One materialized owned project with its published output paths.
struct CliProject {
    _work: tempfile::TempDir,
    manifest: PathBuf,
    out: PathBuf,
}

impl CliProject {
    fn new() -> Self {
        let work = tempfile::Builder::new().prefix("beskid-v06-glue-cli-").tempdir().unwrap();
        let manifest = materialize_owned_project(work.path());
        let out = work.path().join("out");
        Self { _work: work, manifest, out }
    }

    fn consumer(&self) -> PathBuf {
        self.out.join(native_name(CONSUMER_LIBRARY))
    }

    fn owner(&self) -> PathBuf {
        self.out.join(native_name(OWNER_LIBRARY))
    }

    fn owner_source(&self) -> PathBuf {
        self.manifest.parent().unwrap().join("rust/implementation.rs")
    }

    /// Runs the installed CLI with explicit tools. `environment` adds or overrides variables.
    fn build(&self, environment: &[(&str, std::ffi::OsString)]) -> std::process::Output {
        let qualified = require_qualified_prefix();
        let tools = require_tools();
        let corelib = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib")).unwrap();
        let mut command = Command::new(installed_cli(&qualified));
        command
            .args(["build", "--plain", "--project"])
            .arg(&self.manifest)
            .args(["--target", "ManualOwned", "--backend", "glue-rust", "--cargo"])
            .arg(&tools.cargo)
            .arg("--rustc")
            .arg(&tools.rustc)
            .arg("--linker")
            .arg(&tools.linker)
            .arg("--output")
            .arg(self.consumer())
            .env(beskid_abi::runtime_kit::ENV_CORELIB_ROOT, corelib)
            .env_remove(beskid_abi::runtime_kit::ENV_RUNTIME_PREFIX);
        if qualified_profile() == beskid_abi::runtime_kit::BuildProfile::Release {
            command.arg("--release");
        }
        for (name, value) in environment {
            command.env(name, value);
        }
        command.output().unwrap_or_else(|error| panic!("run installed beskid build: {error}"))
    }

    fn assert_no_staging_left(&self) {
        let Ok(entries) = fs::read_dir(&self.out) else { return };
        for entry in entries {
            let name = entry.unwrap().file_name();
            let name = name.to_string_lossy();
            assert!(!name.starts_with(STAGE_PREFIX) && !name.starts_with(".beskid-glue-"), "staging left: {name}");
        }
    }

    fn images(&self) -> (Vec<u8>, Vec<u8>) {
        (fs::read(self.consumer()).unwrap(), fs::read(self.owner()).unwrap())
    }
}

fn output_text(output: &std::process::Output) -> String {
    format!("stdout:\n{}\nstderr:\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

fn assert_cli_success(output: &std::process::Output) {
    assert!(output.status.success(), "installed beskid build failed\n{}", output_text(output));
}

/// Gate WEB-GLUE02: a published image names the canonical provider by loader identity and finds it
/// in the installed kit directory, never in a disposable staging directory.
fn assert_installed_provider_search_path(image: &[u8], label: &str) {
    if cfg!(windows) {
        // PE imports carry only the DLL basename; the process loader pins the installed provider
        // first, and a foreign host adds the kit directory to its DLL search path.
        return;
    }
    let qualified = require_qualified_prefix();
    let provider = beskid_abi::runtime_kit::resolve_glue_shared_provider(&qualified, &host_target(), qualified_profile())
        .unwrap_or_else(|error| panic!("qualified shared provider: {error}"));
    let directory = provider.shared_library.parent().expect("provider directory").to_owned();
    let canonical = fs::canonicalize(&directory).unwrap();
    let contains = |needle: &Path| {
        let needle = needle.to_str().expect("UTF-8 kit path").as_bytes();
        image.windows(needle.len()).any(|window| window == needle)
    };
    assert!(
        contains(&directory) || contains(&canonical),
        "{label} image has no loader search path to the installed provider directory {}",
        directory.display()
    );
}

/// The digest that `beskid build --backend glue-rust` printed for `path` (`sha256 <digest>  <path>`).
fn reported_sha256(output: &std::process::Output, path: &Path) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let suffix = format!("  {}", path.display());
    let digests = stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("sha256 ") && line.ends_with(&suffix))
        .map(|line| line["sha256".len()..line.len() - suffix.len()].trim().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(digests.len(), 1, "one sha256 line for {}\n{}", path.display(), output_text(output));
    digests.into_iter().next().unwrap()
}

/// Admission through the installed kit of the qualified prefix only.
fn admit_published(consumer: &Path, consumer_sha256: &str, owners: &[PathBuf]) -> Result<PublishedGlue, String> {
    let runtime = runtime_request(&require_qualified_prefix());
    admit_published_glue_images(&PublishedGlueRequest { runtime: &runtime, consumer, consumer_sha256, owners })
        .map_err(|error| error.to_string())
}

fn read_u16(bytes: &[u8], at: usize) -> usize {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) as usize
}
fn read_u32(bytes: &[u8], at: usize) -> usize {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
}
fn read_u64(bytes: &[u8], at: usize) -> usize {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()) as usize
}

/// File offset and size of the executable code section (`.text` or `__TEXT,__text`) of a
/// 64-bit little-endian ELF, Mach-O or PE image.
fn code_section(image: &[u8]) -> (usize, usize) {
    if image.starts_with(b"\x7fELF") {
        let (table, entry, count, names) =
            (read_u64(image, 0x28), read_u16(image, 0x3a), read_u16(image, 0x3c), read_u16(image, 0x3e));
        let names_offset = read_u64(image, table + names * entry + 0x18);
        for index in 0..count {
            let header = table + index * entry;
            let name = names_offset + read_u32(image, header);
            if image[name..].starts_with(b".text\0") {
                return (read_u64(image, header + 0x18), read_u64(image, header + 0x20));
            }
        }
    } else if image.starts_with(&[0xcf, 0xfa, 0xed, 0xfe]) {
        let mut command = 32;
        for _ in 0..read_u32(image, 16) {
            let (kind, size) = (read_u32(image, command), read_u32(image, command + 4));
            if kind == 0x19 && image[command + 8..].starts_with(b"__TEXT\0") {
                for section in 0..read_u32(image, command + 64) {
                    let header = command + 72 + section * 80;
                    if image[header..].starts_with(b"__text\0") {
                        return (read_u32(image, header + 48), read_u64(image, header + 40));
                    }
                }
            }
            command += size;
        }
    } else if image.starts_with(b"MZ") {
        let coff = read_u32(image, 0x3c) + 4;
        let table = coff + 20 + read_u16(image, coff + 16);
        for index in 0..read_u16(image, coff + 2) {
            let header = table + index * 40;
            if image[header..].starts_with(b".text\0") {
                return (read_u32(image, header + 20), read_u32(image, header + 16));
            }
        }
    }
    panic!("image has no recognizable code section");
}

/// Copies `source` to `destination` with one byte in the middle of its code section inverted.
fn flipped_code_copy(source: &Path, destination: &Path) {
    let mut image = fs::read(source).unwrap();
    let (offset, size) = code_section(&image);
    assert!(size > 0 && offset + size <= image.len(), "code section out of file bounds");
    image[offset + size / 2] ^= 0xff;
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(destination, image).unwrap();
}

fn native_magic(image: &[u8]) -> bool {
    const MACHO: [[u8; 4]; 2] = [[0xcf, 0xfa, 0xed, 0xfe], [0xfe, 0xed, 0xfa, 0xcf]];
    image.starts_with(b"\x7fELF") || image.starts_with(b"MZ") || MACHO.iter().any(|magic| image.starts_with(magic))
}

#[test]
fn v06_glue_cli_builds_owned_fixture_alone() {
    let project = CliProject::new();
    let output = project.build(&[]);
    assert_cli_success(&output);
    let (consumer, owner) = project.images();
    assert!(native_magic(&consumer), "consumer is not a native image");
    assert!(native_magic(&owner), "owner is not a native image");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for path in [project.consumer(), project.owner()] {
        assert!(stdout.contains(&path.display().to_string()), "{} not reported\n{}", path.display(), output_text(&output));
    }
    assert_installed_provider_search_path(&consumer, "consumer");
    assert_installed_provider_search_path(&owner, "owner");
    project.assert_no_staging_left();

    // Gate WEB-GLUE03: the published files alone, pinned by the consumer digest the CLI printed,
    // load into one process and match the manual results of the in-process producer route.
    let consumer_sha256 = reported_sha256(&output, &project.consumer());
    assert_eq!(consumer_sha256, sha256_file(&project.consumer()), "reported consumer digest");
    assert_eq!(reported_sha256(&output, &project.owner()), sha256_file(&project.owner()), "reported owner digest");
    let published = admit_published(&project.consumer(), &consumer_sha256, &[project.owner()])
        .unwrap_or_else(|error| panic!("admit published images: {error}"));
    let owner_image = published.owner_image(OWNER_LIBRARY).expect("published owner image admitted");
    assert_owned_roundtrip(published.process(), owner_image, published.packet());
    drop(published);
}

/// Gate WEB-GLUE03: published admission fails closed on every substitution, before any image runs.
#[test]
fn v06_glue_cli_published_images_fail_closed_on_substitution() {
    let first = CliProject::new();
    let first_output = first.build(&[]);
    assert_cli_success(&first_output);
    let first_pin = reported_sha256(&first_output, &first.consumer());
    // A second build whose owner source differs: its owner image is a valid owner of the same
    // library, but no record of the first build pins it.
    let second = CliProject::new();
    let source = second.owner_source();
    let mut changed = fs::read_to_string(&source).unwrap();
    changed.push_str("\npub fn beskid_v06_substitute_marker() -> u32 { 6 }\n");
    fs::write(&source, changed).unwrap();
    let second_output = second.build(&[]);
    assert_cli_success(&second_output);
    let second_pin = reported_sha256(&second_output, &second.consumer());
    assert_ne!(sha256_file(&first.owner()), sha256_file(&second.owner()), "substitute owner must differ");

    let tamper = tempfile::Builder::new().prefix("beskid-v06-glue-tamper-").tempdir().unwrap();
    fn rejected(consumer: &Path, pin: &str, owners: &[PathBuf], expected: &str) {
        match admit_published(consumer, pin, owners) {
            Ok(_) => panic!("substituted published images were admitted (expected `{expected}`)"),
            Err(error) => assert!(error.contains(expected), "expected `{expected}`, got: {error}"),
        }
    }

    // One flipped code byte in the consumer: the host pin no longer matches.
    let consumer = tamper.path().join("flipped").join(native_name(CONSUMER_LIBRARY));
    flipped_code_copy(&first.consumer(), &consumer);
    rejected(&consumer, &first_pin, &[first.owner()], "differs from its pinned digest");

    // One flipped code byte in the owner: the consumer record no longer pins it.
    let owner = tamper.path().join("flipped").join(native_name(OWNER_LIBRARY));
    flipped_code_copy(&first.owner(), &owner);
    rejected(&first.consumer(), &first_pin, &[owner], "is not pinned by the consumer record");

    // A swapped owner image from another build of the same library.
    rejected(&first.consumer(), &first_pin, &[second.owner()], "is not pinned by the consumer record");

    // A foreign record: the second consumer, correctly pinned, cannot admit the first owner.
    rejected(&second.consumer(), &second_pin, &[first.owner()], "is not pinned by the consumer record");

    // A missing or duplicated owner, and a pin that is not a lowercase digest.
    rejected(&first.consumer(), &first_pin, &[], "pins 1 owner images, but 0 were supplied");
    rejected(&first.consumer(), &first_pin, &[first.owner(), first.owner()], "pins 1 owner images, but 2");
    rejected(&first.consumer(), &first_pin.to_uppercase(), &[first.owner()], "lowercase SHA-256");

    // The untouched pair still admits after every rejection above.
    let published = admit_published(&first.consumer(), &first_pin, &[first.owner()])
        .unwrap_or_else(|error| panic!("untouched published images: {error}"));
    drop(published);
}

#[test]
fn v06_glue_cli_ignores_inherited_rust_environment() {
    let project = CliProject::new();
    let bogus = std::ffi::OsString::from("/bogus/beskid-v06-not-a-tool");
    let output = project.build(&[
        ("RUSTC", bogus.clone()),
        ("CARGO", bogus.clone()),
        ("RUSTC_WRAPPER", bogus.clone()),
        ("RUSTUP_TOOLCHAIN", "beskid-v06-bogus-toolchain".into()),
        ("CARGO_BUILD_RUSTFLAGS", "-C link-arg=-beskid-v06-bogus".into()),
        ("RUSTFLAGS", "-C link-arg=-beskid-v06-bogus".into()),
        ("CARGO_ENCODED_RUSTFLAGS", "-Cbogus".into()),
    ]);
    assert_cli_success(&output);
    let (consumer, owner) = project.images();
    assert!(native_magic(&consumer) && native_magic(&owner));
    project.assert_no_staging_left();
}

#[test]
fn v06_glue_cli_rejects_ancestor_cargo_config() {
    let project = CliProject::new();
    let root = tempfile::Builder::new().prefix("beskid-v06-glue-ambient-").tempdir().unwrap();
    fs::create_dir_all(root.path().join(".cargo")).unwrap();
    fs::write(root.path().join(".cargo/config.toml"), "[build]\nrustflags = [\"-C\", \"link-arg=-beskid-v06-bogus\"]\n")
        .unwrap();
    let temporary = root.path().join("tmp");
    fs::create_dir_all(&temporary).unwrap();
    let output = project.build(&[
        ("TMPDIR", temporary.clone().into_os_string()),
        ("TMP", temporary.clone().into_os_string()),
        ("TEMP", temporary.into_os_string()),
    ]);
    assert!(!output.status.success(), "ancestor Cargo config was not rejected\n{}", output_text(&output));
    assert!(output_text(&output).contains("ambient Cargo input"), "{}", output_text(&output));
    assert!(!project.consumer().exists() && !project.owner().exists(), "rejected build published an image");
    project.assert_no_staging_left();
}

#[test]
fn v06_glue_cli_rebuild_replaces_and_failed_rebuild_preserves() {
    let project = CliProject::new();
    assert_cli_success(&project.build(&[]));
    #[cfg(unix)]
    let first_inodes = {
        use std::os::unix::fs::MetadataExt;
        (fs::metadata(project.consumer()).unwrap().ino(), fs::metadata(project.owner()).unwrap().ino())
    };

    // A rebuild over existing outputs succeeds and replaces both images.
    assert_cli_success(&project.build(&[]));
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let second = (fs::metadata(project.consumer()).unwrap().ino(), fs::metadata(project.owner()).unwrap().ino());
        assert_ne!(first_inodes.0, second.0, "consumer image was not replaced");
        assert_ne!(first_inodes.1, second.1, "owner image was not replaced");
    }
    project.assert_no_staging_left();
    let published = project.images();

    // An owner source that no longer compiles fails the rebuild and leaves both images untouched.
    let source = project.owner_source();
    let mut broken = fs::read_to_string(&source).unwrap();
    broken.push_str("\ncompile_error!(\"beskid v06 broken owner\");\n");
    fs::write(&source, broken).unwrap();
    let output = project.build(&[]);
    assert!(!output.status.success(), "broken owner rebuild succeeded\n{}", output_text(&output));
    assert_eq!(project.images(), published, "failed rebuild changed the published images");
    project.assert_no_staging_left();
}

/// Records one label per emitted object work unit.
struct EmittedUnits(std::sync::Mutex<Vec<String>>);

impl beskid_pipeline::PipelineObserver for EmittedUnits {
    fn on_event(&self, event: beskid_pipeline::PipelineEvent) {
        if let beskid_pipeline::PipelineEvent::WorkUnit { id: beskid_pipeline::phases::AOT_EMIT_OBJECT, label, .. } =
            event
        {
            self.0.lock().unwrap().push(label.into_owned());
        }
    }
}

/// A native test object needs the exact installed runtime kit, so it runs from a private qualified
/// prefix (absence of `BESKID_V06_QUALIFIED_PREFIX` fails loudly in the parent).
#[test]
fn v06_native_test_object_compiles_shared_helper_once_and_links_distinct_entries() {
    run_in_private_prefix(
        "v06_native_test_object_compiles_shared_helper_once_and_links_distinct_entries",
        DriverStaging::Exact,
        |_| {
            use beskid_analysis::services::{
                FrontEndOptions, resolved_input_from_plan, synthetic_compile_plan_for_source,
            };
            let directory = tempfile::tempdir().unwrap();
            let source = "unit Shared() { return; }\ntest One { Shared(); }\ntest Two { Shared(); }\nunit Unreachable() { return; }";
            let path = directory.path().join("Main.bd");
            fs::write(&path, source).unwrap();
            let plan = synthetic_compile_plan_for_source(&path);
            let resolved = resolved_input_from_plan(path, source.into(), plan, None, None);
            let front =
                beskid_queries::compile_front_end_from_resolved_input(&resolved, FrontEndOptions::default(), None)
                    .unwrap();
            let profile = match qualified_profile() {
                beskid_abi::runtime_kit::BuildProfile::Debug => beskid_aot::BuildProfile::Debug,
                beskid_abi::runtime_kit::BuildProfile::Release => beskid_aot::BuildProfile::Release,
            };
            let kit = beskid_aot::default_runtime_strategy(profile, None).unwrap();
            let prepared = beskid_aot::prepared_syntax::lower_prepared_syntax_entrypoints(
                &front,
                &["One".into(), "Two".into()],
                kit.target.clone(),
            )
            .unwrap();
            let counts = EmittedUnits(std::sync::Mutex::new(Vec::new()));
            let object = beskid_aot::api::emit_native_test_object(
                prepared,
                directory.path(),
                profile,
                kit,
                vec![],
                vec![],
                Some(&counts),
                NativeExecutionControl::new(Instant::now() + Duration::from_secs(300), Arc::new(|| false)),
            )
            .unwrap();
            let labels = counts.0.lock().unwrap();
            assert_eq!(labels.iter().filter(|name| name.starts_with("Shared#")).count(), 1);
            assert!(!labels.iter().any(|name| name.starts_with("Unreachable#")));
            drop(labels);
            for name in ["One", "Two"] {
                let executable = object.link_entry(name, &directory.path().join(name)).unwrap();
                assert!(Command::new(executable).status().unwrap().success());
            }
            assert!(object.link_entry("Unreachable", &directory.path().join("unselected")).is_err());
        },
    );
}
