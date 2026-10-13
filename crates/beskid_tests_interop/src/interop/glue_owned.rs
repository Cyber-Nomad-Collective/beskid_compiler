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
    ffi::{CString, OsStr, OsString},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

const QUALIFIED_PREFIX_ENV: &str = "BESKID_V06_QUALIFIED_PREFIX";
const QUALIFIED_PROFILE_ENV: &str = "BESKID_V06_QUALIFIED_PROFILE";
const CARGO_ENV: &str = "BESKID_V06_GLUE_CARGO";
const RUSTC_ENV: &str = "BESKID_V06_GLUE_RUSTC";
const LINKER_ENV: &str = "BESKID_V06_GLUE_LINKER";
const CHILD_ENV: &str = "BESKID_V06_GLUE_CHILD";
const GATE_ENV: &str = "BESKID_V06_GLUE_BORROW_GATE";
/// Warmed owned project of this test-binary run, handed to private-prefix children.
const TEMPLATE_PROJECT_ENV: &str = "BESKID_V06_GLUE_TEMPLATE_PROJECT";
/// Owner image cache (`build_rust_owner` `image_cache`) handed to the private-prefix children of
/// the owned cases. They assert owner admission and the owned ABI, not a cold tool chain; the
/// cases that assert tool isolation run the installed CLI, which never uses a cache.
const OWNER_IMAGE_CACHE_ENV: &str = "BESKID_V06_GLUE_OWNER_IMAGE_CACHE";
/// Codegen worker bound (item emission and machine-code compilation) for every compiler this
/// harness runs. Inherited when the caller sets it; otherwise [`DEFAULT_CODEGEN_THREADS`].
const CODEGEN_THREADS_ENV: &str = "BESKID_CODEGEN_THREADS";
/// Bounded so that concurrent tests stay within the builder: tests times workers is the core
/// budget (two tests with four workers each on a shared 16-core builder). The unbounded default
/// (5 emission plus 16 compilation workers per compilation, per test) drove the load average past
/// 800 when several interop tests ran in parallel.
const DEFAULT_CODEGEN_THREADS: &str = "4";
const OWNER_LIBRARY: &str = "glue_manual";
/// Consumer native library identity: the target name, as `beskid build --backend glue-rust` uses.
const CONSUMER_LIBRARY: &str = "ManualOwned";
/// `RustOwner` type path the fixture declares on `ManualOpaque`.
const OWNER_TYPE_PATH: &str = "implementation::ManualOwned";
const BORROW_GATE_LENGTH: usize = 4093;
/// Fixture length whose borrow releases its own handle on the same thread (`<gate>.reentrant`).
const REENTRANT_RELEASE_LENGTH: usize = 4091;
const OWNED_LIMIT: usize = 16 * 1024 * 1024;
const PROVIDER_FOREIGN: i32 = 2;
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

/// Hard link `source` at `destination`; a link across file systems falls back to a copy. Every
/// linked file is read-only input to its consumer (kit files, materialized dependency sources,
/// a session snapshot that is replaced by rename, Mod artifacts that are qualified by digest), so
/// sharing an inode with the template never lets one test observe another.
fn link_or_copy(source: &Path, destination: &Path) {
    if fs::hard_link(source, destination).is_ok() {
        return;
    }
    fs::copy(source, destination)
        .unwrap_or_else(|error| panic!("copy {} to {}: {error}", source.display(), destination.display()));
}

/// [`copy_tree`] by hard links ([`link_or_copy`]).
fn link_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap_or_else(|error| panic!("read {}: {error}", source.display())) {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            link_tree(&entry.path(), &target);
        } else if kind.is_file() {
            link_or_copy(&entry.path(), &target);
        } else {
            panic!("template contains a non-regular entry: {}", entry.path().display());
        }
    }
}

fn codegen_threads() -> std::ffi::OsString {
    std::env::var_os(CODEGEN_THREADS_ENV).unwrap_or_else(|| DEFAULT_CODEGEN_THREADS.into())
}

/// Work root of this run. Under Cargo it is `<target>/<profile>/beskid-v06-glue`, beside the
/// `deps/` directory this test executable runs from (the layout `beskid_tests_mod_host` uses). The
/// release producer runs a retained copy of the executable outside any build tree; its root is
/// `beskid-v06-glue` in the system temporary directory. Private prefixes, projects and the
/// template live under one root, on one file system, so per-test copies are hard links.
fn harness_root() -> PathBuf {
    let executable = std::env::current_exe().unwrap();
    let directory = executable.parent().unwrap();
    let base = if directory.file_name() == Some(OsStr::new("deps")) {
        directory.parent().unwrap().to_owned()
    } else {
        std::env::temp_dir()
    };
    let root = base.join("beskid-v06-glue");
    fs::create_dir_all(&root).unwrap();
    root
}

/// A temporary directory under [`harness_root`], removed when dropped like any other.
fn harness_tempdir(prefix: &str) -> tempfile::TempDir {
    tempfile::Builder::new().prefix(prefix).tempdir_in(harness_root()).unwrap()
}

/// Exclusive OS file lock, released on drop or process exit.
struct FileLock(fs::File);

impl FileLock {
    fn acquire(path: &Path) -> Self {
        let file = fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path).unwrap();
        file.lock().unwrap_or_else(|error| panic!("lock {}: {error}", path.display()));
        Self(file)
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// Content key of everything the template is derived from: the qualified CLI and driver bytes,
/// the kit ABI metadata, the owned fixture and every Corelib source of this checkout. A change to
/// any of them yields a new template; consumers still qualify what they read from it.
fn template_key(qualified: &Path) -> String {
    let mut digest = Sha256::new();
    for path in [installed_cli(qualified), qualified.join("bin").join(driver_name()), qualified.join(kit_relative()).join("abi.json")] {
        digest.update(fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display())));
    }
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/glue/manual/owned");
    for name in ["ManualTypes.bd", "ManualImport.bd", "ManualExport.bd", "manual_owned.bproj", "rust/implementation.rs"] {
        digest.update(fs::read(fixture.join(name)).unwrap());
    }
    let corelib = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib")).unwrap();
    digest.update(corelib.to_string_lossy().as_bytes());
    let mut sources = Vec::new();
    collect_corelib_sources(&corelib, &mut sources);
    sources.sort();
    for path in sources {
        digest.update(path.strip_prefix(&corelib).unwrap().to_string_lossy().as_bytes());
        digest.update(fs::read(&path).unwrap());
    }
    format!("{:x}", digest.finalize())[..16].to_owned()
}

fn collect_corelib_sources(directory: &Path, sources: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap_or_else(|error| panic!("read {}: {error}", directory.display())) {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = entry.file_name();
        if entry.file_type().unwrap().is_dir() {
            if !matches!(name.to_str(), Some("obj" | ".beskid" | ".git" | "target")) {
                collect_corelib_sources(&path, sources);
            }
        } else if path.extension().is_some_and(|extension| matches!(extension.to_str(), Some("bd" | "bproj" | "bws" | "bsol")))
            || name == "Project.lock"
        {
            sources.push(path);
        }
    }
}

/// One copy of the qualified prefix's runtime kit and one owned project warmed by the installed
/// CLI, under `<harness root>/template-<key>`, shared by every test of this binary and reused by
/// later runs while [`template_key`] holds. Private prefixes hard link the kit instead of copying
/// it per test; projects start from the warmed `obj/`. Creation is serialized across processes
/// and published by a completion marker; the two newest templates are kept.
///
/// The template project lives at `<harness root>/template-<key>/manual_owned`, at the same depth
/// as every seeded project (`<harness root>/<temporary directory>/manual_owned`): materialized
/// dependency directories are named by the dependency's path relative to the project, so only a
/// project at the same depth finds the seeded copies under their own names.
struct Template {
    kit: PathBuf,
    root: PathBuf,
}

fn template() -> &'static Template {
    // A failed warm-up is recorded once: every later case reports the same failure instead of
    // repeating the build.
    static TEMPLATE: OnceLock<Result<Template, String>> = OnceLock::new();
    let template = TEMPLATE.get_or_init(|| {
        std::panic::catch_unwind(build_template).map_err(|payload| {
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|text| (*text).to_owned()))
                .unwrap_or_else(|| "template preparation panicked".to_owned())
        })
    });
    match template {
        Ok(template) => template,
        Err(error) => panic!("template preparation failed earlier in this run:\n{error}"),
    }
}

fn build_template() -> Template {
    {
        let qualified = require_qualified_prefix();
        let harness = harness_root();
        let _lock = FileLock::acquire(&harness.join("template.lock"));
        let root = harness.join(format!("template-{}", template_key(&qualified)));
        let kit = root.join("kit");
        if !root.join(".kit-complete").is_file() {
            if root.exists() {
                fs::remove_dir_all(&root).unwrap();
            }
            copy_tree(&qualified.join(kit_relative()), &kit.join(kit_relative()));
            fs::write(root.join(".kit-complete"), b"").unwrap();
            prune_templates(&harness, &root);
        }
        Template { kit, root }
    }
}

/// The warmed owned project of this template ([`warm_template_project`]), built on first use by
/// the cases that need one; driver-admission cases never wait for it. A failed warm-up is
/// recorded once for the run.
fn template_project() -> &'static Path {
    static PROJECT: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    let project = PROJECT.get_or_init(|| {
        std::panic::catch_unwind(|| {
            let template = template();
            let _lock = FileLock::acquire(&harness_root().join("template.lock"));
            let project = template.root.join("manual_owned");
            if !template.root.join(".project-complete").is_file() {
                if project.exists() {
                    fs::remove_dir_all(&project).unwrap();
                }
                warm_template_project(&template.root);
                fs::write(template.root.join(".project-complete"), b"").unwrap();
            }
            project
        })
        .map_err(|payload| {
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|text| (*text).to_owned()))
                .unwrap_or_else(|| "template project preparation panicked".to_owned())
        })
    });
    match project {
        Ok(project) => project,
        Err(error) => panic!("template project preparation failed earlier in this run:\n{error}"),
    }
}

/// Remove every template but `keep` and the newest other one.
fn prune_templates(harness: &Path, keep: &Path) {
    let mut others = fs::read_dir(harness)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path != keep
                && path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with("template-"))
        })
        .collect::<Vec<_>>();
    others.sort_by_key(|path| fs::metadata(path).and_then(|metadata| metadata.modified()).ok());
    for stale in others.iter().rev().skip(1) {
        let _ = fs::remove_dir_all(stale);
    }
}

/// Materialize the owned fixture under `root` and build it once with the installed CLI, exactly
/// as the CLI cases do. Its `obj/beskid/deps` (materialized dependencies and the Mod artifacts
/// built there) and `obj/beskid/cache` (AST store, session snapshot) seed every later project;
/// every consumer still qualifies what it finds there (dependency sources by mtime and content,
/// Mod artifacts by descriptor digests against the kit, the snapshot by compiler and Corelib
/// identity and payload digest), so a tampered or stale seed is rebuilt, never trusted. The
/// lockfile is not seeded: the CLI regenerates it for each project.
fn warm_template_project(work: &Path) {
    fs::create_dir_all(work).unwrap();
    let manifest = materialize_owned_project(work);
    let output = run_installed_build(&manifest, &work.join("out").join(native_name(CONSUMER_LIBRARY)), &[]);
    assert!(output.status.success(), "template build with the installed CLI failed\n{}", output_text(&output));
}

/// Seed a freshly materialized project at `project` from the warmed template project, when one
/// is available: the dependency and cache trees of `obj/` by hard links.
fn seed_from_template(project: &Path) {
    let explicit = TEMPLATE_PROJECT.with(|cell| cell.borrow().clone());
    let Some(template) = explicit.or_else(|| std::env::var_os(TEMPLATE_PROJECT_ENV).map(PathBuf::from)) else {
        return;
    };
    assert!(template.is_dir(), "{TEMPLATE_PROJECT_ENV}={} is not a directory", template.display());
    // The project's own materialized sources (`obj/beskid/root`) are not seeded: they are
    // rewritten from the fresh fixture copy, and a case that edits its owner source must not
    // reach the template through a shared inode.
    for relative in ["obj/beskid/deps", "obj/beskid/cache"] {
        let source = template.join(relative);
        if source.is_dir() {
            link_tree(&source, &project.join(relative));
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
    let staged = harness_tempdir("beskid-v06-glue-prefix-");
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
    link_tree(&template().kit.join(kit_relative()), &staged.path().join(kit_relative()));
    let executable = std::env::current_exe().unwrap();
    link_or_copy(&executable, &bin.join(executable.file_name().unwrap()));
    staged
}

fn test_path(name: &str) -> String {
    let module = module_path!().split_once("::").map(|(_, rest)| rest).unwrap_or(module_path!());
    format!("{module}::{name}")
}

/// Runs `body` inside a child copy of this test executable placed in a private
/// qualified prefix. In the parent this stages the prefix and requires that the
/// child really executed exactly this test and passed.
/// Whether a private-prefix child starts its owned project from the warmed template.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ChildProject {
    /// Driver-admission cases materialize no project.
    None,
    /// Owned cases: materialized dependencies, Mod artifacts and the session snapshot are seeded.
    Warmed,
}

fn run_in_private_prefix(name: &str, staging: DriverStaging, project: ChildProject, body: impl FnOnce(&Path)) {
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
    let mut command = Command::new(&executable);
    if let ChildProject::Warmed = project {
        command.env(TEMPLATE_PROJECT_ENV, template_project());
        command.env(OWNER_IMAGE_CACHE_ENV, harness_root().join("owner-images"));
    }
    let output = command
        .args([path.as_str(), "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, &path)
        .env(CODEGEN_THREADS_ENV, codegen_threads())
        .env(GATE_ENV, staged.path().join("borrow.gate"))
        .env(beskid_abi::runtime_kit::ENV_CORELIB_ROOT, fs::canonicalize(corelib).unwrap())
        .env_remove(beskid_abi::runtime_kit::ENV_RUNTIME_PREFIX)
        // Native Mods are qualified and loaded for the profile of the kit the child runs on.
        .env("BESKID_RUNTIME_KIT_PROFILE", beskid_abi::runtime_kit::profile_directory_name(qualified_profile()))
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
    run_in_private_prefix("v06_glue_driver_admitted_only_from_private_prefix", DriverStaging::Exact, ChildProject::None, |prefix| {
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
    run_in_private_prefix("v06_glue_driver_denied_when_sibling_tampered", DriverStaging::Tampered, ChildProject::None, |_| {
        let error = beskid_abi::compiler_driver::installed_compiler_driver().err().expect("tampered driver admitted");
        assert!(error.contains("installed compiler driver changed"), "{error}");
    });
}

#[test]
fn v06_glue_driver_denied_when_sibling_missing() {
    run_in_private_prefix("v06_glue_driver_denied_when_sibling_missing", DriverStaging::Missing, ChildProject::None, |_| {
        let error = beskid_abi::compiler_driver::installed_compiler_driver().err().expect("missing driver admitted");
        assert!(!error.contains("independently qualified native tool driver"), "unqualified test build: {error}");
    });
}

#[cfg(unix)]
#[test]
fn v06_glue_driver_denied_when_sibling_escapes_prefix() {
    run_in_private_prefix("v06_glue_driver_denied_when_sibling_escapes_prefix", DriverStaging::Symlinked, ChildProject::None, |_| {
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
    seed_from_template(&project);
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
    // Corelib depends on the native Serialization Mod: qualify it against this private prefix's kit.
    let native_mods = beskid_tests_mod_host::installed_runtime_mod_host(plan)?;
    let front = beskid_queries::prepare_compilation(
        &resolved,
        beskid_analysis::services::PrepareOptions {
            front_end: beskid_analysis::services::FrontEndOptions {
                with_semantic_diagnostics: true,
                ..Default::default()
            },
            ..beskid_analysis::services::PrepareOptions::with_mod_host(native_mods)
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
    let image_cache = std::env::var_os(OWNER_IMAGE_CACHE_ENV).map(PathBuf::from);
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
        image_cache: image_cache.as_deref(),
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
    run_in_private_prefix("v06_glue_owned_roundtrip_all_representations", DriverStaging::Exact, ChildProject::Warmed, |prefix| {
        let build = build_owned(prefix, prefix, OwnerVariant::Complete)
            .unwrap_or_else(|error| panic!("{error:#}"));
        let (process, owner, packet) = open_owned(build);
        assert_owned_roundtrip(&process, owner, &packet);
        drop(process);
    });
}

/// A live borrow lease survives every release attempt. Owner records are bound to the thread
/// that opened the runtime (the loader's opening-thread restriction, as a Rust `!Send` owner):
/// on that thread a release during the borrow reports busy, while a foreign OS thread cannot
/// observe borrow state at all and is denied as foreign. Neither runs the destructor.
#[test]
fn v06_glue_owned_opaque_borrow_survives_release() {
    run_in_private_prefix("v06_glue_owned_opaque_borrow_survives_release", DriverStaging::Exact, ChildProject::Warmed, |prefix| {
        let gate = PathBuf::from(std::env::var_os(GATE_ENV).expect("parent supplies the borrow gate"));
        let build = build_owned(prefix, prefix, OwnerVariant::Complete)
            .unwrap_or_else(|error| panic!("{error:#}"));
        let (process, _owner, packet) = open_owned(build);
        assert_borrow_survives_release(&process, &packet, &gate);
        drop(process);
    });
}

/// The borrow assertions of [`v06_glue_owned_opaque_borrow_survives_release`] over an admitted
/// process. `gate` is the path the owner fixture reads from `BESKID_V06_GLUE_BORROW_GATE` of this
/// process (R4-OWN-14 same-thread busy, R4-OWN-19 foreign-thread denial, borrowed call).
fn assert_borrow_survives_release(process: &GlueProcess, packet: &GlueArtifact, gate: &Path) {
    let create: Create = unsafe { checked(process, packet, "manual_ImportCreate") };
    let inspect: Inspect = unsafe { checked(process, packet, "manual_ImportInspect") };
    let release = opaque_release(process, packet);

    // Same thread: the owner releases its borrowed handle twice from inside the borrow.
    let mut reentrant = 0;
    assert_eq!(unsafe { create(REENTRANT_RELEASE_LENGTH, &mut reentrant) }, 0);
    fs::write(gate.with_extension("reentrant"), format!("{:x}:{reentrant:x}", release as usize)).unwrap();
    let mut length = 0;
    let status = unsafe { inspect(reentrant, &mut length) };
    assert_eq!(
        status,
        0,
        "same-thread release during a live borrow must report busy (owner observed status {})",
        status - 100
    );
    assert_eq!(length, REENTRANT_RELEASE_LENGTH, "borrowed payload changed under same-thread release");
    assert_eq!(unsafe { release(reentrant) }, 0, "busy releases must not consume the token");
    assert_ne!(unsafe { release(reentrant) }, 0, "double release accepted");

    // Foreign OS thread: denied as foreign while the borrow is live.
    let mut token = 0;
    assert_eq!(unsafe { create(BORROW_GATE_LENGTH, &mut token) }, 0);
    let entered = gate.with_extension("entered");
    let releaser = {
        let gate = gate.to_owned();
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(60);
            while !entered.exists() {
                assert!(Instant::now() < deadline, "owner borrow never became live");
                std::thread::sleep(Duration::from_millis(5));
            }
            // The borrow lease is live inside the owner on the opening thread; this thread
            // is foreign to the owner records, so release is denied without effects.
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
    assert_eq!(first, PROVIDER_FOREIGN, "release from a foreign thread must be denied as foreign");
    assert_eq!(second, PROVIDER_FOREIGN, "repeated foreign-thread release must be denied as foreign");
    // The lease ended with the call; the owner may now release exactly once.
    assert_eq!(unsafe { release(token) }, 0);
    assert_ne!(unsafe { inspect(token, &mut length) }, 0, "released token still borrowable");
    assert_ne!(unsafe { release(token) }, 0, "double release accepted");
}

#[test]
fn v06_glue_owned_denies_missing_import_row() {
    run_in_private_prefix("v06_glue_owned_denies_missing_import_row", DriverStaging::Exact, ChildProject::Warmed, |prefix| {
        let build = build_owned(prefix, prefix, OwnerVariant::MissingRow("glue_unit"))
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
    run_in_private_prefix("v06_glue_owned_denies_consumer_without_owner_domain", DriverStaging::Exact, ChildProject::Warmed, |prefix| {
        let OwnedBuild { owner, consumer, packet } = build_owned(prefix, prefix, OwnerVariant::Complete)
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
    run_in_private_prefix("v06_glue_owned_denies_duplicate_owner_library", DriverStaging::Exact, ChildProject::Warmed, |prefix| {
        let build = build_owned(prefix, prefix, OwnerVariant::Complete)
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
    run_in_private_prefix("v06_glue_owned_rejects_foreign_owner_brand", DriverStaging::Exact, ChildProject::Warmed, |prefix| {
        let error = match build_owned(prefix, prefix, OwnerVariant::ForeignBrand) {
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
    require_qualified_prefix();
    let work = harness_tempdir("beskid-v06-glue-unqualified-");
    let staged = work.path().join("kit");
    link_tree(&template().kit.join(kit_relative()), &staged.join(kit_relative()));
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
    /// A fresh project seeded from the warmed template ([`warm_template_project`]). The seed is
    /// Corelib-side state only (materialized dependencies, Mod artifacts, session snapshot); the
    /// Rust owner is always built cold by the CLI, so the cases that assert tool isolation keep
    /// their meaning.
    fn new() -> Self {
        let work = harness_tempdir("beskid-v06-glue-cli-");
        let manifest = with_template_project(template_project(), || materialize_owned_project(work.path()));
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
    fn build(&self, environment: &[(&str, OsString)]) -> std::process::Output {
        run_installed_build(&self.manifest, &self.consumer(), environment)
    }

    /// Runs `cli` (the installed CLI or a byte-identical link of it) with `tool_arguments` in
    /// place of the explicit tools.
    fn build_with(
        &self,
        cli: &Path,
        tool_arguments: &[OsString],
        environment: &[(&str, OsString)],
    ) -> std::process::Output {
        run_glue_build(cli, &self.manifest, &self.consumer(), tool_arguments, environment)
    }

    fn export_source(&self) -> PathBuf {
        self.manifest.parent().unwrap().join("ManualExport.bd")
    }

    /// The build failed and left neither a published image nor a staging entry.
    fn assert_rejected_without_publication(&self, output: &std::process::Output, label: &str) {
        assert!(!output.status.success(), "{label}: build succeeded\n{}", output_text(output));
        assert!(
            !self.consumer().exists() && !self.owner().exists(),
            "{label}: rejected build published an image\n{}",
            output_text(output)
        );
        self.assert_no_staging_left();
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

/// `<qualified prefix>/bin/beskid build --backend glue-rust` of `manifest` into `output`, with the
/// explicit Rust tools and the codegen worker bound. `environment` adds or overrides variables.
fn run_installed_build(manifest: &Path, output: &Path, environment: &[(&str, OsString)]) -> std::process::Output {
    let cli = installed_cli(&require_qualified_prefix());
    run_glue_build(&cli, manifest, output, &explicit_tool_arguments(&require_tools()), environment)
}

/// `--cargo <cargo> --rustc <rustc> --linker <linker>` of the explicit Rust tools.
fn explicit_tool_arguments(tools: &Tools) -> Vec<OsString> {
    vec![
        "--cargo".into(),
        tools.cargo.clone().into_os_string(),
        "--rustc".into(),
        tools.rustc.clone().into_os_string(),
        "--linker".into(),
        tools.linker.clone().into_os_string(),
    ]
}

/// `<cli> build --plain --project <manifest> --target ManualOwned --backend glue-rust <tool
/// arguments> --output <output>` with the codegen worker bound. `cli` is the installed CLI or a
/// byte-identical link of it in a private prefix. `environment` adds or overrides variables.
fn run_glue_build(
    cli: &Path,
    manifest: &Path,
    output: &Path,
    tool_arguments: &[OsString],
    environment: &[(&str, OsString)],
) -> std::process::Output {
    let corelib = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib")).unwrap();
    let mut command = Command::new(cli);
    command
        .args(["build", "--plain", "--project"])
        .arg(manifest)
        .args(["--target", "ManualOwned", "--backend", "glue-rust"])
        .args(tool_arguments)
        .arg("--output")
        .arg(output)
        .env(beskid_abi::runtime_kit::ENV_CORELIB_ROOT, corelib)
        .env(CODEGEN_THREADS_ENV, codegen_threads())
        .env_remove(beskid_abi::runtime_kit::ENV_RUNTIME_PREFIX);
    if qualified_profile() == beskid_abi::runtime_kit::BuildProfile::Release {
        command.arg("--release");
    }
    for (name, value) in environment {
        command.env(name, value);
    }
    prove_installed_invocation(cli);
    command.output().unwrap_or_else(|error| panic!("run installed beskid build: {error}"))
}

/// Writes the installed invocation proof line to the process stderr before `cli` runs:
/// `beskid-installed-invocation sha256=<digest of the executed CLI bytes> argv0=<file name>`. The
/// release producer marks a required test without such a line `not_executed`, and the release
/// reader requires the digest to be the installed candidate's. A link of the installed CLI in a
/// private prefix has the same bytes, so its line names the same digest. The line is written to
/// the stream directly, so libtest output capture does not hide it.
fn prove_installed_invocation(cli: &Path) {
    let name = cli.file_name().and_then(OsStr::to_str).expect("installed CLI file name");
    let line = format!("beskid-installed-invocation sha256={} argv0={name}\n", sha256_file(cli));
    let mut stderr = std::io::stderr().lock();
    stderr.write_all(line.as_bytes()).expect("write installed invocation proof");
    stderr.flush().expect("flush installed invocation proof");
}

/// Run `materialize` with [`TEMPLATE_PROJECT_ENV`] naming `template`, without touching the
/// process environment (tests of this binary run concurrently): the seed source is passed through
/// a thread-local instead.
fn with_template_project<T>(template: &Path, materialize: impl FnOnce() -> T) -> T {
    TEMPLATE_PROJECT.with(|cell| *cell.borrow_mut() = Some(template.to_path_buf()));
    let result = materialize();
    TEMPLATE_PROJECT.with(|cell| *cell.borrow_mut() = None);
    result
}

thread_local! {
    static TEMPLATE_PROJECT: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
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

// ---------------------------------------------------------------------------
// Release host cases over the installed CLI and its published images.
//
// Every case below runs `<qualified prefix>/bin/beskid` (or a byte-identical link of it in a
// private prefix) and so writes the installed invocation proof line. Cases that call into the
// published images in ways that need process-wide state (the owner's borrow gate) or that end the
// process (an owner abort) run those calls in a child copy of this executable.

const PUBLISHED_CONSUMER_ENV: &str = "BESKID_V06_GLUE_PUBLISHED_CONSUMER";
const PUBLISHED_OWNER_ENV: &str = "BESKID_V06_GLUE_PUBLISHED_OWNER";
const PUBLISHED_PIN_ENV: &str = "BESKID_V06_GLUE_PUBLISHED_PIN";
/// Printed by the abort child immediately before the aborting owner call.
const ABORT_MARKER: &str = "beskid-v06-glue-owner-abort-call";

/// A fresh owned project built by the installed CLI, with the consumer digest it printed.
fn published_project() -> (CliProject, String) {
    let project = CliProject::new();
    let output = project.build(&[]);
    assert_cli_success(&output);
    let pin = reported_sha256(&output, &project.consumer());
    (project, pin)
}

/// The admitted packet of a published consumer, taken out of a process that is closed again.
fn published_packet(project: &CliProject, pin: &str) -> GlueArtifact {
    let published = admit_published(&project.consumer(), pin, &[project.owner()])
        .unwrap_or_else(|error| panic!("admit published images: {error}"));
    let packet = published.packet().clone();
    drop(published);
    packet
}

/// In a child started by [`run_published_child`] for test `name`: admit the published images the
/// parent built with the installed CLI, run `body` over them and return `true`. In the parent:
/// return `false` without running `body`.
fn published_child_body(name: &str, body: impl FnOnce(&PublishedGlue)) -> bool {
    if std::env::var(CHILD_ENV).as_deref() != Ok(test_path(name).as_str()) {
        return false;
    }
    let variable =
        |key: &str| PathBuf::from(std::env::var_os(key).unwrap_or_else(|| panic!("parent supplies {key}")));
    let pin = std::env::var(PUBLISHED_PIN_ENV).expect("parent supplies the consumer pin");
    let published = admit_published(&variable(PUBLISHED_CONSUMER_ENV), &pin, &[variable(PUBLISHED_OWNER_ENV)])
        .unwrap_or_else(|error| panic!("admit published images: {error}"));
    body(&published);
    drop(published);
    true
}

/// Runs test `name` again in a child copy of this executable over the published images of
/// `project`, pinned by the consumer digest `pin` that the installed CLI printed. `environment`
/// (for example the owner fixture's borrow gate) reaches only the child, never sibling tests.
fn run_published_child(
    name: &str,
    project: &CliProject,
    pin: &str,
    environment: &[(&str, OsString)],
) -> std::process::Output {
    let path = test_path(name);
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([path.as_str(), "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, &path)
        .env(PUBLISHED_CONSUMER_ENV, project.consumer())
        .env(PUBLISHED_OWNER_ENV, project.owner())
        .env(PUBLISHED_PIN_ENV, pin);
    for (key, value) in environment {
        command.env(key, value);
    }
    command.output().unwrap_or_else(|error| panic!("spawn published-image child {path}: {error}"))
}

fn assert_child_passed(output: &std::process::Output, label: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed"),
        "{label}: published-image child failed\n{}",
        output_text(output)
    );
}

/// Whether the CLI output mentions `needle`, ignoring line wrapping and report gutters.
fn mentions(output: &std::process::Output, needle: &str) -> bool {
    let squash = |text: &str| text.chars().filter(|c| !c.is_whitespace() && *c != '│').collect::<String>();
    squash(&output_text(output)).contains(&squash(needle))
}

/// R4-CALL-01 and R4-CALL-02 on one admitted process. Import: the owner's issued checked entry,
/// called directly with its hand-written C signature (manual), against the Beskid call through the
/// generated import adapter (`manual_Import*`, generated). Export: the export's own C transport
/// symbol (manual) against its generated checked invocation. Both routes of a row return the same
/// results, and a value that one route rejects the other rejects too.
fn assert_manual_generated_equivalence(process: &GlueProcess, owner: usize, packet: &GlueArtifact) {
    #[allow(clippy::too_many_arguments)]
    fn import_row<T: Copy + std::fmt::Debug + 'static>(
        process: &GlueProcess,
        owner: usize,
        packet: &GlueArtifact,
        owner_symbol: &str,
        generated_symbol: &str,
        values: &[T],
        zero: T,
        same: impl Fn(T, T) -> bool,
    ) {
        let manual: unsafe extern "C" fn(T, *mut T) -> i32 = unsafe { owner_entry(process, owner, packet, owner_symbol) };
        let generated: unsafe extern "C" fn(T, *mut T) -> i32 = unsafe { checked(process, packet, generated_symbol) };
        for value in values {
            let (mut left, mut right) = (zero, zero);
            let manual_status = unsafe { manual(*value, &mut left) };
            let generated_status = unsafe { generated(*value, &mut right) };
            assert_eq!(
                manual_status == 0,
                generated_status == 0,
                "{owner_symbol}/{generated_symbol}({value:?}): manual status {manual_status}, generated status {generated_status}"
            );
            if manual_status == 0 {
                assert!(same(left, right), "{owner_symbol}/{generated_symbol}({value:?}): {left:?} differs from {right:?}");
            }
        }
    }
    fn export_row<T: Copy + std::fmt::Debug + 'static>(
        process: &GlueProcess,
        packet: &GlueArtifact,
        symbol: &str,
        values: &[T],
        zero: T,
        same: impl Fn(T, T) -> bool,
    ) {
        assert_eq!(
            beskid_codegen::glue::artifact::export_transport_symbol(export_binding(packet, symbol)),
            symbol,
            "{symbol} has a direct C transport"
        );
        let name = CString::new(symbol).unwrap();
        let direct: unsafe extern "C" fn(T) -> T = *unsafe { process.export::<unsafe extern "C" fn(T) -> T>(0, &name) }
            .unwrap_or_else(|error| panic!("{symbol}: {error}"));
        let generated: unsafe extern "C" fn(T, *mut T) -> i32 = unsafe { checked(process, packet, symbol) };
        for value in values {
            let manual = unsafe { direct(*value) };
            let mut out = zero;
            assert_eq!(unsafe { generated(*value, &mut out) }, 0, "{symbol}({value:?}) generated status");
            assert!(same(manual, out), "{symbol}({value:?}): manual {manual:?} differs from generated {out:?}");
        }
    }
    import_row::<i8>(process, owner, packet, "glue_i8", "manual_ImportI8", &[i8::MIN, -1, 0, i8::MAX], 0, |a, b| a == b);
    import_row::<i16>(process, owner, packet, "glue_i16", "manual_ImportI16", &[i16::MIN, 0, i16::MAX], 0, |a, b| a == b);
    import_row::<i32>(process, owner, packet, "glue_i32", "manual_ImportI32", &[i32::MIN, 0, i32::MAX], 0, |a, b| a == b);
    import_row::<i64>(process, owner, packet, "glue_i64", "manual_ImportI64", &[i64::MIN, 0, i64::MAX], 0, |a, b| a == b);
    import_row::<u8>(process, owner, packet, "glue_u8", "manual_ImportU8", &[0, 128, u8::MAX], 0, |a, b| a == b);
    import_row::<u16>(process, owner, packet, "glue_u16", "manual_ImportU16", &[0, u16::MAX], 0, |a, b| a == b);
    import_row::<u32>(process, owner, packet, "glue_u32", "manual_ImportU32", &[0, u32::MAX], 0, |a, b| a == b);
    import_row::<u64>(process, owner, packet, "glue_u64", "manual_ImportU64", &[0, u64::MAX], 0, |a, b| a == b);
    let f32_values = [0.0, -0.0, f32::MIN, f32::MAX, f32::INFINITY, f32::NEG_INFINITY, f32::from_bits(0x7fc0_1234)];
    let f64_values =
        [0.0, -0.0, f64::MIN, f64::MAX, f64::INFINITY, f64::NEG_INFINITY, f64::from_bits(0x7ff8_0000_0000_1234)];
    import_row::<f32>(process, owner, packet, "glue_f32", "manual_ImportF32", &f32_values, 0.0, |a, b| a.to_bits() == b.to_bits());
    import_row::<f64>(process, owner, packet, "glue_f64", "manual_ImportF64", &f64_values, 0.0, |a, b| a.to_bits() == b.to_bits());
    import_row::<u8>(process, owner, packet, "glue_bool", "manual_ImportBool", &[0, 1, 2, 255], 0, |a, b| a == b);
    let chars = [0, 0x41, 0xd7ff, 0xe000, 0x1f600, 0x10ffff, 0xd800, 0xdfff, 0x110000];
    import_row::<u32>(process, owner, packet, "glue_char", "manual_ImportChar", &chars, 0, |a, b| a == b);
    import_row::<usize>(process, owner, packet, "glue_native_width", "manual_ImportNativeWidth", &[0, usize::MAX], 0, |a, b| a == b);
    let manual_unit: unsafe extern "C" fn() -> i32 = unsafe { owner_entry(process, owner, packet, "glue_unit") };
    let generated_unit: unsafe extern "C" fn() -> i32 = unsafe { checked(process, packet, "manual_ImportUnit") };
    assert_eq!(unsafe { manual_unit() }, 0, "glue_unit manual status");
    assert_eq!(unsafe { generated_unit() }, 0, "manual_ImportUnit generated status");

    export_row::<i8>(process, packet, "beskid_i8", &[i8::MIN, -1, 0, i8::MAX], 0, |a, b| a == b);
    export_row::<i16>(process, packet, "beskid_i16", &[i16::MIN, 0, i16::MAX], 0, |a, b| a == b);
    export_row::<i32>(process, packet, "beskid_i32", &[i32::MIN, 0, i32::MAX], 0, |a, b| a == b);
    export_row::<i64>(process, packet, "beskid_i64", &[i64::MIN, 0, i64::MAX], 0, |a, b| a == b);
    export_row::<u8>(process, packet, "beskid_u8", &[0, 128, u8::MAX], 0, |a, b| a == b);
    export_row::<u16>(process, packet, "beskid_u16", &[0, u16::MAX], 0, |a, b| a == b);
    export_row::<u32>(process, packet, "beskid_u32", &[0, u32::MAX], 0, |a, b| a == b);
    export_row::<u64>(process, packet, "beskid_u64", &[0, u64::MAX], 0, |a, b| a == b);
    export_row::<f32>(process, packet, "beskid_f32", &f32_values, 0.0, |a, b| a.to_bits() == b.to_bits());
    export_row::<f64>(process, packet, "beskid_f64", &f64_values, 0.0, |a, b| a.to_bits() == b.to_bits());
    export_row::<u8>(process, packet, "beskid_bool", &[0, 1], 0, |a, b| a == b);
    export_row::<u32>(process, packet, "beskid_char", &[0, 0x41, 0xd7ff, 0xe000, 0x1f600, 0x10ffff], 0, |a, b| a == b);
    export_row::<usize>(process, packet, "beskid_native_width", &[0, usize::MAX], 0, |a, b| a == b);
    assert_eq!(
        beskid_codegen::glue::artifact::export_transport_symbol(export_binding(packet, "beskid_unit")),
        "beskid_unit",
        "beskid_unit has a direct C transport"
    );
    let name = CString::new("beskid_unit").unwrap();
    let direct_unit: unsafe extern "C" fn() = *unsafe { process.export::<unsafe extern "C" fn()>(0, &name) }
        .unwrap_or_else(|error| panic!("beskid_unit: {error}"));
    let generated_unit: unsafe extern "C" fn() -> i32 = unsafe { checked(process, packet, "beskid_unit") };
    unsafe { direct_unit() };
    assert_eq!(unsafe { generated_unit() }, 0, "beskid_unit generated status");
}

/// R4-CALL-01/02: manual and generated routes agree on the images the installed CLI published.
#[test]
fn v06_glue_cli_manual_and_generated_routes_agree() {
    const NAME: &str = "v06_glue_cli_manual_and_generated_routes_agree";
    if published_child_body(NAME, |published| {
        let owner = published.owner_image(OWNER_LIBRARY).expect("published owner image admitted");
        assert_manual_generated_equivalence(published.process(), owner, published.packet());
    }) {
        return;
    }
    let (project, pin) = published_project();
    assert_child_passed(&run_published_child(NAME, &project, &pin, &[]), NAME);
}

/// R4-ABI-10 borrowed call, R4-OWN-14 and R4-OWN-19 on the images `beskid build --backend
/// glue-rust` published. Only the child process sees the owner fixture's borrow gate.
#[test]
fn v06_glue_cli_published_borrow_survives_release() {
    const NAME: &str = "v06_glue_cli_published_borrow_survives_release";
    if published_child_body(NAME, |published| {
        let gate = PathBuf::from(std::env::var_os(GATE_ENV).expect("parent supplies the borrow gate"));
        assert_borrow_survives_release(published.process(), published.packet(), &gate);
    }) {
        return;
    }
    let (project, pin) = published_project();
    let gates = harness_tempdir("beskid-v06-glue-gate-");
    let gate = gates.path().join("borrow.gate").into_os_string();
    assert_child_passed(&run_published_child(NAME, &project, &pin, &[(GATE_ENV, gate)]), NAME);
}

/// R4-CALL-04: a Rust owner abort is not unwound into Beskid or the host. The process that
/// hosts the published images ends abnormally at the aborting call; the harness observes it as a
/// failed child process.
#[test]
fn v06_glue_cli_owner_abort_is_a_terminal_process_failure() {
    const NAME: &str = "v06_glue_cli_owner_abort_is_a_terminal_process_failure";
    if published_child_body(NAME, |published| {
        let owner = published.owner_image(OWNER_LIBRARY).expect("published owner image admitted");
        let failure: unsafe extern "C" fn(u32, *mut i32) -> i32 =
            unsafe { owner_entry(published.process(), owner, published.packet(), "glue_failure") };
        let mut out = -1;
        assert_eq!(unsafe { failure(0, &mut out) }, 0, "owner entry is live before the abort");
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(format!("{ABORT_MARKER}\n").as_bytes()).unwrap();
        stdout.flush().unwrap();
        drop(stdout);
        let status = unsafe { failure(3, &mut out) };
        panic!("owner abort returned status {status} to the caller");
    }) {
        return;
    }
    let (project, pin) = published_project();
    let output = run_published_child(NAME, &project, &pin, &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success(), "aborting owner call left the process successful\n{}", output_text(&output));
    assert!(stdout.contains(ABORT_MARKER), "child did not reach the aborting call\n{}", output_text(&output));
    assert!(!stdout.contains("test result:"), "abort must end the process, not a test\n{}", output_text(&output));
    assert!(!output_text(&output).contains("owner abort returned"), "abort returned to the caller\n{}", output_text(&output));
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(output.status.signal(), Some(6), "owner abort ends the host with SIGABRT\n{}", output_text(&output));
    }
}

/// R4-BIND-01/02/03, R4-ABI-11 and R4-ABI-21, R4-GEN-02/03 on the packet the installed CLI
/// published: the admitted packet validates, and every binding or packet field that the identity
/// covers cannot change without the issued identity, digest or profile check rejecting it.
#[test]
fn v06_glue_cli_published_packet_rejects_identity_profile_and_path_mutations() {
    use beskid_codegen::glue::artifact::GeneratedFile;
    let (project, pin) = published_project();
    let packet = published_packet(&project, &pin);
    packet.validate().unwrap_or_else(|error| panic!("published packet: {error}"));
    assert_eq!(packet.manifest.native_width_bits, 64, "selected-target word width");
    assert_eq!(packet.manifest.target, host_target().triple.as_str(), "selected target");
    let rejected = |mutated: &GlueArtifact, expected: &str, label: &str| match mutated.validate() {
        Ok(()) => panic!("{label}: mutated packet validated"),
        Err(error) => assert!(error.to_string().contains(expected), "{label}: expected `{expected}`, got `{error}`"),
    };
    let position = |symbol: &str| {
        packet
            .manifest
            .bindings
            .iter()
            .position(|binding| binding.direction == "export" && binding.symbol == symbol)
            .unwrap_or_else(|| panic!("export row {symbol} absent"))
    };
    let create = position("manual_ImportCreate");
    assert!(packet.manifest.bindings[create].fallible, "manual_ImportCreate reaches a fallible import");

    // Binding identity covers direction, fallibility, ownership and library.
    let mut direction = packet.clone();
    direction.manifest.bindings[create].direction = "import".into();
    rejected(&direction, "binding identity differs", "direction");
    let mut fallible = packet.clone();
    fallible.manifest.bindings[create].fallible = false;
    rejected(&fallible, "binding identity differs", "export fallibility");
    let factory = packet
        .manifest
        .bindings
        .iter()
        .position(|binding| binding.direction == "import" && binding.symbol == "make_manual_owned")
        .expect("import row make_manual_owned absent");
    assert!(packet.manifest.bindings[factory].fallible, "make_manual_owned is declared RustOwner(Fallible:true)");
    let mut import_fallible = packet.clone();
    import_fallible.manifest.bindings[factory].fallible = false;
    rejected(&import_fallible, "binding identity differs", "import fallibility (R4-ABI-21)");
    let utf8 = position("beskid_utf8");
    let mut ownership = packet.clone();
    let binding = &mut ownership.manifest.bindings[utf8];
    std::mem::swap(&mut binding.parameters[0], &mut binding.result);
    rejected(&ownership, "binding identity differs", "borrowed input and owned result swapped");
    let mut library = packet.clone();
    for binding in &mut library.manifest.bindings {
        binding.library = format!("z{}", binding.library);
    }
    rejected(&library, "binding identity differs", "library");

    // The packet profile covers the selected target and its word width.
    let mut target = packet.clone();
    target.manifest.target = if packet.manifest.target == "x86_64-unknown-linux-gnu" {
        "aarch64-apple-darwin".into()
    } else {
        "x86_64-unknown-linux-gnu".into()
    };
    rejected(&target, "differs", "target");
    let mut width = packet.clone();
    width.manifest.native_width_bits = 32;
    rejected(&width, "schema/target/runtime provider profile differs", "native width");

    // Generated paths are confined, unique and digest-bound; the packet digest covers them.
    for path in ["../escape.rs", "/absolute.rs", "nested/./file.rs", "C:/drive.rs", "back\\slash.rs"] {
        let mut escaping = packet.clone();
        escaping.files.push(GeneratedFile::new(path, b"escape".to_vec()));
        rejected(&escaping, "unsafe or duplicate generated path", path);
    }
    let mut duplicate = packet.clone();
    duplicate.files.push(duplicate.files[0].clone());
    rejected(&duplicate, "unsafe or duplicate generated path", "duplicate path");
    let generated = packet
        .files
        .iter()
        .position(|file| file.path != "glue-manifest.json" && !file.bytes.is_empty())
        .expect("packet carries a generated adapter file");
    let mut stale = packet.clone();
    stale.files[generated].bytes[0] ^= 0xff;
    rejected(&stale, "generated bytes exceed bound or digest differs", "file bytes under a stale digest");
    let mut rehashed = packet.clone();
    let path = rehashed.files[generated].path.clone();
    let mut bytes = rehashed.files[generated].bytes.clone();
    bytes[0] ^= 0xff;
    rehashed.files[generated] = GeneratedFile::new(path, bytes);
    rejected(&rehashed, "artifact digest differs", "rehashed file under the issued packet digest");
}

/// R4-GEN-01: two installed builds of equivalent projects at different paths issue the same
/// packet; machine-local paths do not reach the generated files or the binding identities.
#[test]
fn v06_glue_cli_equivalent_projects_issue_identical_packets() {
    let (first, first_pin) = published_project();
    let (second, second_pin) = published_project();
    assert_ne!(first.manifest, second.manifest, "projects live at different paths");
    let (left, right) = (published_packet(&first, &first_pin), published_packet(&second, &second_pin));
    let mut differences = Vec::new();
    if left.manifest.compiled_source_sha256 != right.manifest.compiled_source_sha256 {
        differences.push("compiled_source_sha256".to_owned());
    }
    for (a, b) in left.manifest.bindings.iter().zip(&right.manifest.bindings) {
        if a != b {
            differences.push(format!("binding {}: {:?} vs {:?}", a.symbol, a.source, b.source));
        }
    }
    let files = |packet: &GlueArtifact| packet.files.iter().map(|file| (file.path.clone(), file.sha256.clone())).collect::<Vec<_>>();
    if files(&left) != files(&right) {
        differences.push(format!("generated files {:?} vs {:?}", files(&left), files(&right)));
    }
    assert!(
        left == right && left.sha256 == right.sha256,
        "packets of equivalent projects differ ({} vs {}):\n{}",
        left.sha256,
        right.sha256,
        differences.join("\n")
    );
}

/// R4-BIND-04: closed boundary metadata. Two `Export` attributes, a repeated argument or an
/// unknown argument on a boundary function are rejected before generation, without publication.
#[test]
fn v06_glue_cli_rejects_closed_boundary_metadata_violations() {
    const ORIGINAL: &str = "[Export(Abi:\"C\", Symbol:\"beskid_i8\")]";
    for (label, replacement) in [
        ("two Export attributes", "[Export(Abi:\"C\", Symbol:\"beskid_i8\")]\n[Export(Abi:\"C\", Symbol:\"beskid_i8_again\")]"),
        ("repeated Symbol argument", "[Export(Abi:\"C\", Symbol:\"beskid_i8\", Symbol:\"beskid_i8\")]"),
        ("unknown argument", "[Export(Abi:\"C\", Symbol:\"beskid_i8\", Library:\"glue_manual\")]"),
    ] {
        let project = CliProject::new();
        let source = project.export_source();
        let text = fs::read_to_string(&source).unwrap();
        assert_eq!(text.matches(ORIGINAL).count(), 1, "fixture export declaration changed");
        fs::write(&source, text.replace(ORIGINAL, replacement)).unwrap();
        let output = project.build(&[]);
        project.assert_rejected_without_publication(&output, label);
        assert!(
            mentions(&output, "Export") || mentions(&output, "Glue"),
            "{label}: rejection does not name the boundary metadata\n{}",
            output_text(&output)
        );
    }
}

/// R4-TOOL-01: the installed CLI accepts only `--rust-toolchain <prefix>` XOR `--cargo` with
/// `--rustc`, plus `--linker`, and names the required form for every other combination.
#[test]
fn v06_glue_cli_tool_selection_names_the_required_form() {
    let tools = require_tools();
    let cli = installed_cli(&require_qualified_prefix());
    let path = |path: &PathBuf| path.clone().into_os_string();
    let project = CliProject::new();
    let cases: [(&str, Vec<OsString>, &str); 4] = [
        ("no Rust tools", vec!["--linker".into(), path(&tools.linker)], "requires explicit Rust tools"),
        (
            "toolchain and cargo",
            vec![
                "--rust-toolchain".into(),
                "beskid-v06-unused-toolchain".into(),
                "--cargo".into(),
                path(&tools.cargo),
                "--linker".into(),
                path(&tools.linker),
            ],
            "`--rust-toolchain` conflicts with `--cargo`/`--rustc`",
        ),
        (
            "cargo without rustc",
            vec!["--cargo".into(), path(&tools.cargo), "--linker".into(), path(&tools.linker)],
            "`--cargo` requires `--rustc`",
        ),
        (
            "no linker",
            vec!["--cargo".into(), path(&tools.cargo), "--rustc".into(), path(&tools.rustc)],
            "requires `--linker <path>`",
        ),
    ];
    for (label, arguments, expected) in cases {
        let output = project.build_with(&cli, &arguments, &[]);
        project.assert_rejected_without_publication(&output, label);
        assert!(mentions(&output, expected), "{label}: expected `{expected}`\n{}", output_text(&output));
        assert!(mentions(&output, "pass `--rust-toolchain <prefix>`"), "{label}: required form not named\n{}", output_text(&output));
    }
}

/// R4-TOOL-03: a missing, non-regular or non-executable tool fails the build before any image is
/// published.
#[test]
fn v06_glue_cli_unusable_tools_fail_before_publication() {
    let tools = require_tools();
    let cli = installed_cli(&require_qualified_prefix());
    let scratch = harness_tempdir("beskid-v06-glue-tools-");
    let suffix = std::env::consts::EXE_SUFFIX;
    let missing = scratch.path().join(format!("missing-cargo{suffix}"));
    let directory = scratch.path().join(format!("directory-rustc{suffix}"));
    fs::create_dir(&directory).unwrap();
    let text = scratch.path().join(format!("text-cargo{suffix}"));
    fs::write(&text, b"not an executable tool\n").unwrap();
    let arguments = |cargo: &Path, rustc: &Path| -> Vec<OsString> {
        vec![
            "--cargo".into(),
            cargo.as_os_str().to_owned(),
            "--rustc".into(),
            rustc.as_os_str().to_owned(),
            "--linker".into(),
            tools.linker.clone().into_os_string(),
        ]
    };
    let project = CliProject::new();
    let cases: [(&str, Vec<OsString>, Option<&str>); 3] = [
        ("missing cargo", arguments(missing.as_path(), tools.rustc.as_path()), Some("cannot be resolved")),
        ("directory rustc", arguments(tools.cargo.as_path(), directory.as_path()), Some("is not a regular file")),
        ("non-executable cargo", arguments(text.as_path(), tools.rustc.as_path()), None),
    ];
    for (label, arguments, expected) in cases {
        let output = project.build_with(&cli, &arguments, &[]);
        project.assert_rejected_without_publication(&output, label);
        if let Some(expected) = expected {
            assert!(mentions(&output, expected), "{label}: expected `{expected}`\n{}", output_text(&output));
        }
    }
}

/// A private prefix holding a link of the installed CLI, a tampered or missing driver sibling and
/// a link of the qualified runtime kit. Returns the prefix and its CLI path.
fn stage_cli_prefix(staging: DriverStaging) -> (tempfile::TempDir, PathBuf) {
    let qualified = require_qualified_prefix();
    let installed = installed_cli(&qualified);
    let staged = harness_tempdir("beskid-v06-glue-cli-prefix-");
    let bin = staged.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let cli = bin.join(installed.file_name().unwrap());
    link_or_copy(&installed, &cli);
    let driver = bin.join(driver_name());
    match staging {
        DriverStaging::Tampered => {
            let mut bytes = fs::read(qualified.join("bin").join(driver_name())).unwrap();
            bytes.push(0);
            fs::write(&driver, bytes).unwrap();
            make_executable(&driver);
        }
        DriverStaging::Missing => {}
        _ => panic!("CLI driver staging covers only rejected drivers"),
    }
    link_tree(&template().kit.join(kit_relative()), &staged.path().join(kit_relative()));
    (staged, cli)
}

/// R4-TOOL-08: the installed CLI admits its Rust owner driver only as the exact qualified sibling
/// in its own prefix. The same CLI bytes beside a tampered or missing driver fail closed.
#[test]
fn v06_glue_cli_private_prefix_driver_admission_fails_closed() {
    let tools = explicit_tool_arguments(&require_tools());
    for (staging, label) in [(DriverStaging::Tampered, "tampered driver"), (DriverStaging::Missing, "missing driver")] {
        let (prefix, cli) = stage_cli_prefix(staging);
        assert_eq!(sha256_file(&cli), sha256_file(&installed_cli(&require_qualified_prefix())), "linked CLI bytes");
        let project = CliProject::new();
        let output = project.build_with(&cli, &tools, &[]);
        project.assert_rejected_without_publication(&output, label);
        assert!(
            !mentions(&output, "independently qualified native tool driver"),
            "{label}: the CLI under test was built without a qualified driver\n{}",
            output_text(&output)
        );
        if staging == DriverStaging::Tampered {
            assert!(mentions(&output, "installed compiler driver changed"), "{label}\n{}", output_text(&output));
        }
        drop(prefix);
    }
}

/// Names the release fixture binary (`beskid_release_fixture`); the producer supplies it for the
/// cases bound to it. Absence is a test failure, never a skip.
const RELEASE_FIXTURE_ENV: &str = "BESKID_V06_RELEASE_FIXTURE";

fn release_fixture() -> PathBuf {
    let raw = std::env::var_os(RELEASE_FIXTURE_ENV)
        .unwrap_or_else(|| panic!("{RELEASE_FIXTURE_ENV} must name the release fixture binary"));
    fs::canonicalize(&raw).unwrap_or_else(|error| panic!("{RELEASE_FIXTURE_ENV}={}: {error}", PathBuf::from(&raw).display()))
}

/// A Rust compiler that runs and reports its version and host but holds no standard library for
/// any target: links of the files of `<sysroot>/bin` and `<sysroot>/lib` of the explicit `rustc`
/// (its own executables and shared libraries) under `root`, without `lib/rustlib`.
fn stage_rustc_without_target_library(root: &Path, rustc: &Path) -> PathBuf {
    let sysroot = rustc.parent().and_then(Path::parent).expect("the explicit rustc lives in <sysroot>/bin");
    for directory in ["bin", "lib"] {
        let destination = root.join(directory);
        fs::create_dir_all(&destination).unwrap();
        for entry in fs::read_dir(sysroot.join(directory)).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                link_or_copy(&entry.path(), &destination.join(entry.file_name()));
            }
        }
    }
    root.join("bin").join(rustc.file_name().unwrap())
}

/// R4-TOOL-02: a Rust compiler that answers its version probe but cannot build the requested
/// target is rejected by the installed CLI before the owner build, without publication.
#[test]
fn v06_glue_cli_rustc_without_the_target_library_is_rejected_before_build() {
    let tools = require_tools();
    let cli = installed_cli(&require_qualified_prefix());
    let scratch = harness_tempdir("beskid-v06-glue-sysroot-");
    let rustc = stage_rustc_without_target_library(scratch.path(), &tools.rustc);
    let arguments: Vec<OsString> = vec![
        "--cargo".into(),
        tools.cargo.clone().into_os_string(),
        "--rustc".into(),
        rustc.into_os_string(),
        "--linker".into(),
        tools.linker.clone().into_os_string(),
    ];
    let project = CliProject::new();
    let output = project.build_with(&cli, &arguments, &[]);
    project.assert_rejected_without_publication(&output, "rustc without the target library");
    assert!(
        mentions(&output, &format!("Rust compiler cannot build target `{}`", host_target().triple.as_str())),
        "rejection does not name the missing target support\n{}",
        output_text(&output)
    );
}

/// R4-TOOL-04: a tool probe that never answers is terminated with its process tree within the
/// probe bound, and the build fails without publication.
#[test]
fn v06_glue_cli_hanging_tool_probe_is_terminated() {
    let tools = require_tools();
    let cli = installed_cli(&require_qualified_prefix());
    let scratch = harness_tempdir("beskid-v06-glue-hanging-");
    let hanging = scratch.path().join(format!("beskid-hanging-tool{}", std::env::consts::EXE_SUFFIX));
    fs::copy(release_fixture(), &hanging).unwrap();
    make_executable(&hanging);
    let arguments: Vec<OsString> = vec![
        "--cargo".into(),
        tools.cargo.clone().into_os_string(),
        "--rustc".into(),
        hanging.clone().into_os_string(),
        "--linker".into(),
        tools.linker.clone().into_os_string(),
    ];
    let project = CliProject::new();
    let started = Instant::now();
    let output = project.build_with(&cli, &arguments, &[]);
    let elapsed = started.elapsed();
    project.assert_rejected_without_publication(&output, "hanging rustc probe");
    assert!(mentions(&output, "Rust compiler probe"), "rejection does not name the probe\n{}", output_text(&output));
    let pid = fs::read_to_string(scratch.path().join("beskid-hanging-tool.pid"))
        .unwrap_or_else(|error| panic!("the hanging probe never started: {error}\n{}", output_text(&output)));
    assert!(
        elapsed < Duration::from_secs(300),
        "the probe bound did not end the build early ({elapsed:?})\n{}",
        output_text(&output)
    );
    #[cfg(unix)]
    {
        let alive = Command::new("kill").args(["-0", pid.trim()]).status().unwrap().success();
        assert!(!alive, "hanging probe process {} outlived the build", pid.trim());
    }
    #[cfg(not(unix))]
    let _ = pid;
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
        ChildProject::None,
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
                beskid_queries::compile_mod_free_front_end(&resolved, FrontEndOptions::default(), None)
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
