//! `beskid build --backend glue-rust`: one shared Glue consumer image for the selected Lib target
//! plus one Rust owner image for each manifest `glue` block that a selected binding references.
//!
//! Every rejection here happens before Cargo, rustc or the linker runs. Tools come only from the
//! command line (`--rust-toolchain` XOR `--cargo` + `--rustc`, plus `--linker`); `CARGO`, `RUSTC`
//! and `PATH` are never read. Owner type and callable tables come only from `RustOwner` source facts
//! through `beskid_aot::api::glue::rust_owner_source_tables`, which the native harness shares. The
//! images link the installed kit provider by run path (Gate WEB-GLUE02). All images are produced in one fresh staging directory
//! beside the output and published together after every image has been admitted, so a failed
//! rebuild leaves the previous images untouched.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use beskid_aot::{
    BuildOutputKind, BuildProfile,
    api::{
        NativeExecutionControl,
        glue::{
            GlueArtifactBuildRequest, RustOwnerBuildRequest, RustOwnerCallable, RustOwnerSource, RustOwnerType,
            build_glue_artifact, build_rust_owner, glue_declarations, rust_owner_declaration_libraries,
            rust_owner_source_tables,
        },
    },
    runtime::{RuntimeBuildRequest, RuntimeLinkage},
};
use beskid_analysis::projects::{
    ProjectGlueBackend, ProjectGlueOwner, ProjectManifest, TargetKind, collect_glue_owner_sources,
    load_manifest_from_path,
};
use beskid_codegen::{CodegenInput, glue::GlueArtifact};
use beskid_queries::GlueDirection;
use beskid_tools::PipelineProgressKind;
use beskid_tools::pipeline::tui::CommandSummary;
use beskid_tools::session::{CommandSession, SemanticGateOptions};

use super::{BuildArgs, BuildKind, default_output_path, resolve_input_args};

/// Wall-clock budget for the complete owner and consumer production of one build.
const GLUE_BUILD_BUDGET: Duration = Duration::from_secs(900);
const STAGE_PREFIX: &str = ".beskid-glue-build-";

/// Explicit Rust tools of one `glue-rust` build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RustGlueTools {
    pub cargo: PathBuf,
    pub rustc: PathBuf,
    pub linker: PathBuf,
}

const TOOL_FORM: &str = "pass `--rust-toolchain <prefix>`, or both `--cargo <path>` and `--rustc <path>`, plus `--linker <path>`";

/// Select the Rust tools from the command-line flags only. Pure: no file system access.
pub(super) fn select_tools(
    rust_toolchain: Option<&Path>,
    cargo: Option<&Path>,
    rustc: Option<&Path>,
    linker: Option<&Path>,
) -> Result<RustGlueTools> {
    let (cargo, rustc) = match (rust_toolchain, cargo, rustc) {
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
            bail!("`--rust-toolchain` conflicts with `--cargo`/`--rustc`; {TOOL_FORM}")
        }
        (Some(prefix), None, None) => {
            let bin = prefix.join("bin");
            (
                bin.join(format!("cargo{}", std::env::consts::EXE_SUFFIX)),
                bin.join(format!("rustc{}", std::env::consts::EXE_SUFFIX)),
            )
        }
        (None, Some(cargo), Some(rustc)) => (cargo.to_path_buf(), rustc.to_path_buf()),
        (None, Some(_), None) => bail!("`--cargo` requires `--rustc`; {TOOL_FORM}"),
        (None, None, Some(_)) => bail!("`--rustc` requires `--cargo`; {TOOL_FORM}"),
        (None, None, None) => bail!("`--backend glue-rust` requires explicit Rust tools; {TOOL_FORM}"),
    };
    let linker = linker.ok_or_else(|| anyhow!("`--backend glue-rust` requires `--linker <path>`; {TOOL_FORM}"))?;
    Ok(RustGlueTools { cargo, rustc, linker: linker.to_path_buf() })
}

/// Reject flags that have no meaning for a shared Glue consumer instead of ignoring them.
pub(super) fn reject_inapplicable_flags(args: &BuildArgs) -> Result<()> {
    if let Some(kind) = args.kind
        && kind != BuildKind::Shared
    {
        bail!(
            "`--backend glue-rust` builds a shared Glue consumer; `--kind {}` is not supported (omit `--kind` or pass `--kind shared`)",
            format!("{kind:?}").to_lowercase()
        );
    }
    let inapplicable = [
        ("--entrypoint", args.entrypoint.is_some()),
        ("--object-output", args.object_output.is_some()),
        ("--export", !args.export_symbols.is_empty()),
        ("--prefer-static", args.prefer_static),
        ("--prefer-dynamic", args.prefer_dynamic),
        ("--verbose-link", args.verbose_link),
    ];
    if let Some((flag, _)) = inapplicable.iter().find(|(_, present)| *present) {
        bail!("`{flag}` does not apply to `--backend glue-rust`");
    }
    Ok(())
}

fn resolve_tool(flag: &str, path: &Path) -> Result<PathBuf> {
    let canonical =
        fs::canonicalize(path).with_context(|| format!("{flag} tool `{}` cannot be resolved", path.display()))?;
    ensure!(canonical.is_file(), "{flag} tool `{}` is not a regular file", canonical.display());
    Ok(canonical)
}

impl RustGlueTools {
    /// Resolve each selected tool to an existing file. The bounded probe and the executable
    /// identity recheck run in the owner producer.
    fn resolve(&self, from_toolchain: bool) -> Result<Self> {
        let (cargo_flag, rustc_flag) =
            if from_toolchain { ("--rust-toolchain", "--rust-toolchain") } else { ("--cargo", "--rustc") };
        Ok(Self {
            cargo: resolve_tool(cargo_flag, &self.cargo)?,
            rustc: resolve_tool(rustc_flag, &self.rustc)?,
            linker: resolve_tool("--linker", &self.linker)?,
        })
    }
}

/// Every `RustOwner` placement of every assembly unit must name a manifest Rust `glue` block.
fn validate_rust_owner_declarations(input: &CodegenInput<'_>, manifest: &ProjectManifest) -> Result<()> {
    for (unit, library) in rust_owner_declaration_libraries(input)? {
        let declared =
            manifest.glue.iter().any(|owner| owner.library == library && owner.backend == ProjectGlueBackend::Rust);
        ensure!(
            declared,
            "`RustOwner` in `{unit}` names library `{library}`, but the manifest has no `glue \"{library}\" {{ backend = rust ... }}` block"
        );
    }
    Ok(())
}

/// Libraries that the consumer packet references through an Extern import or a `GlueHandle` type.
fn referenced_libraries(packet: &GlueArtifact) -> BTreeSet<String> {
    let mut libraries = BTreeSet::new();
    for binding in &packet.manifest.bindings {
        if binding.direction == "import" {
            libraries.insert(binding.library.clone());
        }
        for physical in binding.parameters.iter().chain(std::iter::once(&binding.result)) {
            if let Some(opaque) = &physical.opaque {
                libraries.insert(opaque.library.clone());
            }
        }
    }
    libraries
}

/// One staged image, its published destination, and the producer-admitted image digest.
struct StagedImage {
    staged: PathBuf,
    destination: PathBuf,
    sha256: String,
}

/// Replace every destination with its staged image. A failure restores the previous images.
fn publish_images(stage: &Path, images: &[StagedImage]) -> Result<()> {
    let previous_root = stage.join("previous");
    fs::create_dir(&previous_root).context("create the previous-image directory")?;
    let mut previous = Vec::new();
    let mut published = Vec::new();
    let mut failure = None;
    for (index, image) in images.iter().enumerate() {
        if fs::symlink_metadata(&image.destination).is_ok() {
            let saved = previous_root.join(index.to_string());
            if let Err(error) = fs::rename(&image.destination, &saved) {
                failure = Some(anyhow!("move previous `{}` aside: {error}", image.destination.display()));
                break;
            }
            previous.push((saved, image.destination.clone()));
        }
        if let Err(error) = fs::rename(&image.staged, &image.destination) {
            failure = Some(anyhow!("publish `{}`: {error}", image.destination.display()));
            break;
        }
        published.push(image.destination.clone());
    }
    let Some(failure) = failure else { return Ok(()) };
    for destination in published {
        let _ = fs::remove_file(destination);
    }
    for (saved, destination) in previous {
        if let Err(error) = fs::rename(&saved, &destination) {
            return Err(failure.context(format!(
                "restoring `{}` from `{}` also failed: {error}",
                destination.display(),
                saved.display()
            )));
        }
    }
    Err(failure)
}

fn select_owner<'a>(manifest: &'a ProjectManifest, library: &str) -> Option<&'a ProjectGlueOwner> {
    manifest.glue.iter().find(|owner| owner.library == library)
}

pub(super) fn run(args: BuildArgs, tools: RustGlueTools) -> Result<()> {
    let tools = tools.resolve(args.rust_toolchain.is_some())?;
    let resolve_args = resolve_input_args(&args);
    let (mut session, resolved) =
        CommandSession::open_and_resolve(args.plain, PipelineProgressKind::FullBuild, &resolve_args)?;
    let plan = resolved.compile_plan.as_ref().ok_or_else(|| {
        anyhow!("`--backend glue-rust` builds a project Lib target; pass `--project <manifest>` instead of a source file")
    })?;
    match plan.target.kind {
        TargetKind::Lib => {}
        TargetKind::App | TargetKind::Test => bail!(
            "`--backend glue-rust` requires a Lib target; target `{}` is {:?}",
            plan.target.name,
            plan.target.kind
        ),
    }
    let manifest = load_manifest_from_path(&plan.manifest_path)?;
    ensure!(
        !manifest.glue.is_empty(),
        "`--backend glue-rust` requires a manifest `glue \"<library>\" {{ backend = rust path = \"<dir>\" }}` block; project `{}` declares none",
        plan.project_name
    );
    let consumer_library = plan.target.name.clone();
    for owner in &manifest.glue {
        ensure!(
            owner.backend == ProjectGlueBackend::Rust,
            "glue `{}` selects backend `{}`, which is unavailable in 0.6",
            owner.library,
            owner.backend.as_str()
        );
        ensure!(
            owner.library != consumer_library,
            "glue owner `{}` has the same native library identity as the consumer target `{consumer_library}`",
            owner.library
        );
    }

    session.set_mod_invoker(super::super::compiler_mod::prepare_native_mod_executor(
        &resolved,
        args.lockfile.WorkspaceOptions(),
        Some(session.observer()),
    )?);
    let prepared = session.executable_gate_prepared(
        &resolved,
        SemanticGateOptions { finish_prepare_ui: false, prepare_message: "Analysis complete" },
    )?;
    let front = prepared.into_executable()?;

    let target = beskid_aot::target::detect_target(args.target_triple.as_deref())?;
    let abi_target = beskid_abi::abi_v5::TargetMetadata::for_triple(&target.triple)
        .map_err(|_| anyhow!("unsupported ABI-v5 Glue target `{}`", target.triple))?;
    let output = match args.output.clone() {
        Some(path) => path,
        None => default_output_path(&resolved, BuildOutputKind::SharedLib, &target),
    };
    let output_name = output.file_name().ok_or_else(|| anyhow!("output `{}` has no file name", output.display()))?;
    let parent = match output.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    fs::create_dir_all(&parent).with_context(|| format!("create output directory `{}`", parent.display()))?;
    let profile = if args.release { BuildProfile::Release } else { BuildProfile::Debug };
    let release = args.release;
    let target_triple = args.target_triple.clone();
    let project_root = plan.project_root.clone();

    let stage = tempfile::Builder::new()
        .prefix(STAGE_PREFIX)
        .tempdir_in(&parent)
        .with_context(|| format!("create a staging directory in `{}`", parent.display()))?;
    let images = beskid_aot::with_prepared_glue_input(&front, abi_target, |input, items| -> Result<Vec<StagedImage>> {
        let glue_libraries = manifest.glue.iter().map(|owner| owner.library.clone()).collect::<Vec<_>>();
        let declarations = glue_declarations(input, items, &glue_libraries)?;
        validate_rust_owner_declarations(input, &manifest)?;
        ensure!(
            declarations.iter().any(|declaration| declaration.direction == GlueDirection::Import),
            "`--backend glue-rust` requires at least one glue block referenced by an Extern import; target `{consumer_library}` has none"
        );
        let selected = declarations.iter().map(|declaration| declaration.item.clone()).collect::<Vec<_>>();
        let consumer_packet = beskid_codegen::glue::emit(input, &selected, &consumer_library)?;
        consumer_packet.validate()?;
        let referenced = referenced_libraries(&consumer_packet);
        for library in &referenced {
            ensure!(
                select_owner(&manifest, library).is_some(),
                "Glue library `{library}` is referenced by target `{consumer_library}` but has no manifest `glue \"{library}\"` block"
            );
        }
        for owner in &manifest.glue {
            ensure!(
                referenced.contains(&owner.library),
                "glue `{}` is not referenced by any selected Extern import or GlueHandle type of target `{consumer_library}`",
                owner.library
            );
        }

        let runtime = RuntimeBuildRequest {
            kit: beskid_aot::default_runtime_strategy(profile, target_triple.as_deref())?,
            linkage: RuntimeLinkage::GlueSharedProviderV1,
        };
        let control = NativeExecutionControl::new(Instant::now() + GLUE_BUILD_BUDGET, Arc::new(|| false));
        let mut images = Vec::new();
        let mut owners = Vec::new();
        for owner in &manifest.glue {
            let imports = declarations
                .iter()
                .filter(|declaration| declaration.is_import_of(&owner.library))
                .cloned()
                .collect::<Vec<_>>();
            ensure!(
                !imports.is_empty(),
                "glue `{}` is referenced only by GlueHandle types; a Rust owner needs at least one Extern import",
                owner.library
            );
            let owner_selected = imports.iter().map(|declaration| declaration.item.clone()).collect::<Vec<_>>();
            let packet = beskid_codegen::glue::emit(input, &owner_selected, &owner.library)?;
            packet.validate()?;
            let tables = rust_owner_source_tables(input, &owner.library, &imports, &packet)?;
            let types = tables.types.into_iter().map(RustOwnerType::from).collect::<Vec<_>>();
            let callables = tables.callables.into_iter().map(RustOwnerCallable::from).collect::<Vec<_>>();
            let sources = collect_glue_owner_sources(&project_root, owner)?
                .files
                .into_iter()
                .map(|file| RustOwnerSource { relative_path: file.relative_path, bytes: file.bytes })
                .collect::<Vec<_>>();
            let file_name = beskid_aot::target::output_filename(&owner.library, BuildOutputKind::SharedLib, &target);
            let destination = parent.join(&file_name);
            ensure!(
                destination.file_name() != Some(output_name),
                "glue owner `{}` image `{file_name}` collides with the consumer output `{}`",
                owner.library,
                output.display()
            );
            let staged = stage.path().join(&file_name);
            let prepared = build_rust_owner(&RustOwnerBuildRequest {
                input,
                selected: &owner_selected,
                native_library: &owner.library,
                expected: &packet,
                sources: &sources,
                types: &types,
                callables: &callables,
                runtime: &runtime,
                cargo: &tools.cargo,
                rustc: &tools.rustc,
                linker: &tools.linker,
                release,
                output_path: &staged,
                control: &control,
            })
            .with_context(|| format!("build Rust owner `{}`", owner.library))?;
            images.push(StagedImage { staged, destination, sha256: prepared.native_sha256().to_owned() });
            owners.push(prepared);
        }
        let staged = stage.path().join(output_name);
        // The consumer's embedded admission record pins every owner image digest (Gate WEB-GLUE03).
        let owner_pins = owners.iter().collect::<Vec<_>>();
        let consumer = build_glue_artifact(&GlueArtifactBuildRequest {
            input,
            all_items: items,
            selected: &selected,
            native_library: &consumer_library,
            expected: &consumer_packet,
            runtime: &runtime,
            output_path: &staged,
            control: &control,
            owners: &owner_pins,
        })
        .with_context(|| format!("build Glue consumer `{consumer_library}`"))?;
        images.push(StagedImage { staged, destination: output.clone(), sha256: consumer.native_sha256().to_owned() });
        Ok(images)
    })?;
    publish_images(stage.path(), &images)?;
    drop(stage);

    session.pipeline().finish_build_with_summary(
        "Build complete",
        CommandSummary::plain("Build", "Build complete").with_stat("output", output.display().to_string()),
    );
    println!();
    // The consumer digest is the trust root a host passes to
    // `beskid_aot::api::glue::admit_published_glue_images` (Gate WEB-GLUE03).
    for image in &images {
        println!("  output   {}", image.destination.display());
        println!("  sha256   {}  {}", image.sha256, image.destination.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> PathBuf {
        PathBuf::from(text)
    }

    #[test]
    fn toolchain_prefix_selects_bin_tools() {
        let tools = select_tools(Some(Path::new("/rust")), None, None, Some(Path::new("/cc"))).unwrap();
        let suffix = std::env::consts::EXE_SUFFIX;
        assert_eq!(tools.cargo, path("/rust").join("bin").join(format!("cargo{suffix}")));
        assert_eq!(tools.rustc, path("/rust").join("bin").join(format!("rustc{suffix}")));
        assert_eq!(tools.linker, path("/cc"));
    }

    #[test]
    fn explicit_cargo_and_rustc_select_exact_paths() {
        let tools =
            select_tools(None, Some(Path::new("/c/cargo")), Some(Path::new("/c/rustc")), Some(Path::new("/cc"))).unwrap();
        assert_eq!(tools, RustGlueTools { cargo: path("/c/cargo"), rustc: path("/c/rustc"), linker: path("/cc") });
    }

    #[test]
    fn conflicting_or_partial_tool_flags_name_the_required_form() {
        let cases: [(Option<&str>, Option<&str>, Option<&str>, Option<&str>, &str); 6] = [
            (Some("/rust"), Some("/cargo"), None, Some("/cc"), "conflicts"),
            (Some("/rust"), None, Some("/rustc"), Some("/cc"), "conflicts"),
            (None, Some("/cargo"), None, Some("/cc"), "`--cargo` requires `--rustc`"),
            (None, None, Some("/rustc"), Some("/cc"), "`--rustc` requires `--cargo`"),
            (None, None, None, Some("/cc"), "requires explicit Rust tools"),
            (Some("/rust"), None, None, None, "requires `--linker <path>`"),
        ];
        for (toolchain, cargo, rustc, linker, expected) in cases {
            let error = select_tools(
                toolchain.map(Path::new),
                cargo.map(Path::new),
                rustc.map(Path::new),
                linker.map(Path::new),
            )
            .unwrap_err()
            .to_string();
            assert!(error.contains(expected), "{error}");
            assert!(error.contains("--rust-toolchain <prefix>"), "{error}");
        }
    }

    #[test]
    fn missing_tool_file_is_rejected_with_its_flag() {
        let root = tempfile::tempdir().unwrap();
        let linker = root.path().join("cc");
        fs::write(&linker, b"").unwrap();
        let tools = select_tools(Some(root.path()), None, None, Some(linker.as_path())).unwrap();
        let error = tools.resolve(true).unwrap_err().to_string();
        assert!(error.contains("--rust-toolchain"), "{error}");
    }

    #[test]
    fn publish_replaces_every_destination_and_keeps_no_stage_entry() {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().join("stage");
        fs::create_dir(&stage).unwrap();
        let images = ["a", "b"]
            .into_iter()
            .map(|name| {
                let staged = stage.join(name);
                fs::write(&staged, format!("new {name}")).unwrap();
                let destination = root.path().join(name);
                fs::write(&destination, format!("old {name}")).unwrap();
                StagedImage { staged, destination, sha256: String::new() }
            })
            .collect::<Vec<_>>();
        publish_images(&stage, &images).unwrap();
        for name in ["a", "b"] {
            assert_eq!(fs::read_to_string(root.path().join(name)).unwrap(), format!("new {name}"));
        }
    }

    #[test]
    fn failed_publish_restores_previous_images() {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().join("stage");
        fs::create_dir(&stage).unwrap();
        let first = StagedImage { staged: stage.join("a"), destination: root.path().join("a"), sha256: String::new() };
        fs::write(&first.staged, "new a").unwrap();
        fs::write(&first.destination, "old a").unwrap();
        // The second staged image is missing, so its publication fails after the first succeeded.
        let second = StagedImage { staged: stage.join("b"), destination: root.path().join("b"), sha256: String::new() };
        fs::write(&second.destination, "old b").unwrap();
        assert!(publish_images(&stage, &[first, second]).is_err());
        assert_eq!(fs::read_to_string(root.path().join("a")).unwrap(), "old a");
        assert_eq!(fs::read_to_string(root.path().join("b")).unwrap(), "old b");
    }
}
