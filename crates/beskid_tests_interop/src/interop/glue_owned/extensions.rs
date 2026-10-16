//! v0.6.1 Glue extensions: records by value, fieldless enums, call-scoped callbacks and the
//! generated `beskid` consumer module of a Rust owner.
//!
//! Manual first: `v061_glue_extensions_manual_c_abi` builds the hand-written C ABI fixture
//! (`fixtures/glue/manual/extensions`: a C header with the profile layout, a Rust library and a
//! Rust consumer that plays the generated adapters' role) with the explicit `rustc`. The
//! generated case then runs the same behaviour through the owner build, the consumer image and
//! `GlueProcess` (`fixtures/glue/extensions/owned`).
use super::*;

const MANUAL_HEADER: &str = include_str!("../../../fixtures/glue/manual/extensions/extensions.h");
const MANUAL_LIBRARY: &str = include_str!("../../../fixtures/glue/manual/extensions/extensions.rs");
const MANUAL_CALLER: &str = include_str!("../../../fixtures/glue/manual/extensions/extensions_caller.rs");

const EXTENSION_FIXTURE_FILES: [(&str, &str); 3] = [
    ("ExtensionTypes.bd", include_str!("../../../fixtures/glue/extensions/owned/ExtensionTypes.bd")),
    ("ExtensionImport.bd", include_str!("../../../fixtures/glue/extensions/owned/ExtensionImport.bd")),
    ("rust/implementation.rs", include_str!("../../../fixtures/glue/extensions/owned/rust/implementation.rs")),
];
const EXTENSION_FIXTURE_MANIFEST: &str = include_str!("../../../fixtures/glue/extensions/owned/glue_extensions.bproj");
const EXTENSION_PROJECT: &str = "glue_extensions";
const EXTENSION_CONSUMER: &str = "GlueExtensions";

fn run_tool(command: &mut Command, label: &str) -> std::process::Output {
    let output = command.output().unwrap_or_else(|error| panic!("{label}: {error}"));
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn v061_glue_extensions_manual_c_abi() {
    let rustc = require_tool(RUSTC_ENV);
    let work = harness_tempdir("glue-extensions-manual-");
    let root = work.path();
    for (name, text) in
        [("extensions.h", MANUAL_HEADER), ("extensions.rs", MANUAL_LIBRARY), ("extensions_caller.rs", MANUAL_CALLER)]
    {
        fs::write(root.join(name), text).unwrap();
    }
    if !cfg!(windows) {
        // The header's static assertions fix the C ABI profile layout.
        run_tool(
            Command::new(require_tool(LINKER_ENV))
                .args(["-x", "c", "-std=c11", "-fsyntax-only"])
                .arg(root.join("extensions.h")),
            "extensions.h layout check",
        );
    }
    run_tool(
        Command::new(&rustc)
            .args(["--edition", "2024", "--crate-type", "cdylib", "--crate-name", "glue_extensions_manual", "--out-dir"])
            .arg(root)
            .arg(root.join("extensions.rs")),
        "manual extension library",
    );
    let mut caller = Command::new(&rustc);
    caller
        .args(["--edition", "2024", "--crate-name", "extensions_caller", "--out-dir"])
        .arg(root)
        .arg("-L")
        .arg(format!("native={}", root.display()))
        .arg(root.join("extensions_caller.rs"));
    if !cfg!(windows) {
        caller.arg("-C").arg(format!("link-arg=-Wl,-rpath,{}", root.display()));
    }
    run_tool(&mut caller, "manual extension consumer");
    let executable = root.join(if cfg!(windows) { "extensions_caller.exe" } else { "extensions_caller" });
    let mut run = Command::new(&executable);
    if cfg!(windows) {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![root.to_owned()];
        paths.extend(std::env::split_paths(&path));
        run.env("PATH", std::env::join_paths(paths).unwrap());
    }
    let output = run_tool(&mut run, "manual extension gate");
    assert!(String::from_utf8_lossy(&output.stdout).contains("glue extension manual C ABI: ok"));
}

/// Writes the embedded extension fixture project into `root`, bound to the installed Corelib.
fn materialize_extension_project(root: &Path) -> PathBuf {
    let project = root.join(EXTENSION_PROJECT);
    fs::create_dir_all(project.join("rust")).unwrap();
    for (name, text) in EXTENSION_FIXTURE_FILES {
        fs::write(project.join(name), text).unwrap();
    }
    assert!(EXTENSION_FIXTURE_MANIFEST.contains(OWNED_FIXTURE_CORELIB), "extension manifest corelib dependency changed");
    let corelib = installed_corelib(&require_qualified_prefix()).join("beskid_corelib");
    let corelib = corelib.to_str().expect("UTF-8 corelib path").replace('\\', "/");
    fs::write(
        project.join(format!("{EXTENSION_PROJECT}.bproj")),
        EXTENSION_FIXTURE_MANIFEST.replace(OWNED_FIXTURE_CORELIB, &format!("{corelib:?}")),
    )
    .unwrap();
    seed_from_template(&project);
    project.join(format!("{EXTENSION_PROJECT}.bproj"))
}

fn build_extensions(prefix: &Path, work: &Path) -> anyhow::Result<OwnedBuild> {
    let tools = require_tools();
    fs::create_dir_all(work)?;
    let manifest = materialize_extension_project(work);
    let sources = manifest_owner_sources(&manifest)?;
    with_glue_input(&manifest, EXTENSION_PROJECT, EXTENSION_CONSUMER, |input, items| {
        let declarations = glue_declarations(input, items, &[OWNER_LIBRARY.to_owned()])?;
        let imports =
            declarations.iter().filter(|declaration| declaration.is_import_of(OWNER_LIBRARY)).cloned().collect::<Vec<_>>();
        let selected = declarations.iter().map(|declaration| declaration.item.clone()).collect::<Vec<_>>();
        let consumer_packet = beskid_codegen::glue::emit(input, &selected, EXTENSION_CONSUMER)?;
        consumer_packet.validate()?;
        let owner_selected = imports.iter().map(|declaration| declaration.item.clone()).collect::<Vec<_>>();
        let owner_packet = beskid_codegen::glue::emit(input, &owner_selected, OWNER_LIBRARY)?;
        let tables = rust_owner_source_tables(input, OWNER_LIBRARY, &imports, &owner_packet)?;
        anyhow::ensure!(tables.types.is_empty(), "the extension fixture declares no opaque handle types");
        let types = Vec::<RustOwnerType>::new();
        let callables = tables.callables.into_iter().map(RustOwnerCallable::from).collect::<Vec<_>>();
        let runtime = runtime_request(prefix);
        let control = control();
        let image_cache = std::env::var_os(OWNER_IMAGE_CACHE_ENV).map(PathBuf::from);
        let owner = build_rust_owner(&RustOwnerBuildRequest {
            input,
            selected: &owner_selected,
            native_library: OWNER_LIBRARY,
            expected: &owner_packet,
            consumer: &consumer_packet,
            sources: &sources,
            types: &types,
            callables: &callables,
            runtime: &runtime,
            cargo: &tools.cargo,
            rustc: &tools.rustc,
            linker: &tools.linker,
            release: false,
            output_path: &work.join(native_name(OWNER_LIBRARY)),
            control: &control,
            image_cache: image_cache.as_deref(),
        })?;
        let consumer = build_glue_artifact(&GlueArtifactBuildRequest {
            input,
            all_items: items,
            selected: &selected,
            native_library: EXTENSION_CONSUMER,
            expected: &consumer_packet,
            runtime: &runtime,
            output_path: &work.join(native_name(EXTENSION_CONSUMER)),
            control: &control,
            owners: &[&owner],
        })?;
        Ok(OwnedBuild { owner, consumer, packet: consumer_packet })
    })
}

/// C layout of the fixture's `Point` record (`u8 tag, i64 x, f64 weight, bool visible, char
/// mark`), with raw Bool and char fields so invalid values can be sent.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct RawPoint {
    tag: u8,
    x: i64,
    weight: f64,
    visible: u8,
    mark: u32,
}

fn assert_extensions(process: &GlueProcess, packet: &GlueArtifact) {
    let point = export_binding(packet, "ext_point");
    let record = point.result.record.as_ref().expect("ext_point returns a record");
    assert_eq!(
        (record.size, record.alignment, record.fields.iter().map(|field| field.offset).collect::<Vec<_>>()),
        (32, 8, vec![0, 8, 16, 24, 28]),
        "generated record layout differs from the manual C ABI fixture"
    );
    assert_eq!(
        (size_of::<RawPoint>(), core::mem::offset_of!(RawPoint, visible), core::mem::offset_of!(RawPoint, mark)),
        (32, 24, 28)
    );

    // Record by value through the import and back.
    let call: unsafe extern "C" fn(RawPoint, *mut RawPoint) -> i32 = unsafe { checked(process, packet, "ext_point") };
    let input = RawPoint { tag: 255, x: -21, weight: 1.25, visible: 1, mark: 0x1f600 };
    let mut output = RawPoint { tag: 9, x: 9, weight: 9.0, visible: 9, mark: 9 };
    assert_eq!(unsafe { call(input, &mut output) }, 0);
    assert_eq!(output, RawPoint { tag: 0, x: -42, weight: 1.75, visible: 0, mark: 0x1f600 });
    for invalid in [RawPoint { visible: 2, ..input }, RawPoint { mark: 0xd800, ..input }] {
        let mut output = RawPoint { tag: 9, x: 9, weight: 9.0, visible: 9, mark: 9 };
        assert_eq!(unsafe { call(invalid, &mut output) }, 1, "{invalid:?} reached the export body");
        assert_eq!(output, RawPoint { tag: 0, x: 0, weight: 0.0, visible: 0, mark: 0 }, "failed output not zeroed");
    }

    // Fieldless enum discriminants, checked on receipt.
    let next: unsafe extern "C" fn(u32, *mut u32) -> i32 = unsafe { checked(process, packet, "ext_next_color") };
    for (value, expected) in [(0, 1), (1, 2), (2, 0)] {
        let mut out = u32::MAX;
        assert_eq!(unsafe { next(value, &mut out) }, 0);
        assert_eq!(out, expected);
    }
    let mut out = u32::MAX;
    assert_eq!(unsafe { next(3, &mut out) }, 1, "a discriminant that names no variant reached the body");
    assert_eq!(out, 0);

    // Call-scoped callbacks: a capturing lambda, and enum callback arguments.
    let sum: unsafe extern "C" fn(i64, i64, *mut i64) -> i32 = unsafe { checked(process, packet, "ext_sum_each") };
    let mut total = 0;
    assert_eq!(unsafe { sum(5, 3, &mut total) }, 0);
    assert_eq!(total, 3 * (1 + 2 + 3 + 4));
    let red: unsafe extern "C" fn(i64, *mut i64) -> i32 = unsafe { checked(process, packet, "ext_count_red") };
    let mut count = 0;
    assert_eq!(unsafe { red(7, &mut count) }, 0);
    assert_eq!(count, 3, "indices 0, 3 and 6 are Red");

    // Rust owner code calls a Beskid export through its generated `beskid` module.
    let reenter: unsafe extern "C" fn(i64, *mut i64) -> i32 = unsafe { checked(process, packet, "ext_reenter") };
    let mut value = 0;
    assert_eq!(unsafe { reenter(20, &mut value) }, 0);
    assert_eq!(value, 41, "owner code did not reach the Beskid export");
}

#[test]
fn v061_glue_extensions_owned_roundtrip() {
    run_in_private_prefix("extensions::v061_glue_extensions_owned_roundtrip", DriverStaging::Exact, ChildProject::Warmed, |prefix| {
        let build = build_extensions(prefix, prefix).unwrap_or_else(|error| panic!("{error:#}"));
        let (process, _owner, packet) = open_owned(build);
        assert_extensions(&process, &packet);
        drop(process);
    });
}

/// Release case GLUE066-01: the installed CLI builds the extension fixture's Rust owner and
/// consumer (`build --backend glue-rust`), and the published images alone, admitted by the
/// consumer digest the CLI printed, carry records by value, fieldless enums, call-scoped
/// callbacks and owner re-entry exactly as the in-process route does.
#[test]
fn v066_glue_cli_extensions_records_enums_callbacks_roundtrip() {
    let work = harness_tempdir("beskid-v066-glue-extensions-cli-");
    let manifest = with_template_project(template_project(), || materialize_extension_project(work.path()));
    let out = work.path().join("out");
    let consumer = out.join(native_name(EXTENSION_CONSUMER));
    let owner = out.join(native_name(OWNER_LIBRARY));
    let output = run_installed_target_build(&manifest, EXTENSION_CONSUMER, &consumer, &[]);
    assert_cli_success(&output);
    assert!(native_magic(&fs::read(&consumer).unwrap()), "consumer is not a native image");
    assert!(native_magic(&fs::read(&owner).unwrap()), "owner is not a native image");
    let consumer_sha256 = reported_sha256(&output, &consumer);
    assert_eq!(consumer_sha256, sha256_file(&consumer), "reported consumer digest");
    assert_eq!(reported_sha256(&output, &owner), sha256_file(&owner), "reported owner digest");
    let published = admit_published(&consumer, &consumer_sha256, &[owner])
        .unwrap_or_else(|error| panic!("admit published extension images: {error}"));
    assert_extensions(published.process(), published.packet());
    drop(published);
}
