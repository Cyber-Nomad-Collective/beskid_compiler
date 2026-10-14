//! Assemble a declared fixture beside, never inside, the exact production runtime corpus.

use super::roots::{EffectiveCompilationRoots, effective_roots_for_plan, effective_roots_from_plan_and_workspace};
use super::{AssemblyError, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit};
use crate::projects::{CompilePlan, PreparedProjectWorkspace};
use crate::syntax_query::SyntaxIndex;
use beskid_abi::abi_v5::SourceUnit as CanonicalSourceUnit;
use beskid_abi::runtime_source::{
    CANONICAL_PUBLIC_DYNAMIC_SOURCE_PATH, canonical_runtime_sources, canonical_runtime_support_sources,
    prove_runtime_fixture, runtime_fixture_project_root,
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Logical prefix of every unit owned by the native runtime package root (`runtime/beskid`).
const RUNTIME_ROOT_LOGICAL_PREFIX: &str = "src/";
/// Dependency name of the Corelib Foundation package in every fixture plan's closure.
const FOUNDATION_DEPENDENCY_NAME: &str = "corelib_foundation";

/// Source roots an assembly of `plan` reads.
///
/// The declared runtime fixture project is proven against the compiler checkout itself (runtime
/// root, Foundation root and the fixture's own sources), so without a prepared workspace its
/// roots are exactly the plan-declared checkout roots. A `Project.lock` and `obj/beskid` snapshot
/// left beside the fixture by an earlier CLI run are a local replay hint whose bytes may be stale
/// and whose paths are not the checkout paths [`attach_runtime_fixture`] replaces; replaying them
/// would admit a second copy of every canonical Foundation unit (`Core.Results` declared twice)
/// and move the host root away from the fixture entry. Every other plan keeps the ordinary
/// workspace / lockfile / plan precedence.
pub(super) fn assembly_roots(
    plan: &CompilePlan,
    workspace: Option<&PreparedProjectWorkspace>,
) -> EffectiveCompilationRoots {
    if workspace.is_none() && is_runtime_fixture_manifest(&plan.manifest_path) {
        return effective_roots_from_plan_and_workspace(plan, None);
    }
    effective_roots_for_plan(plan, workspace)
}

/// Whether `manifest` is the compiler-declared runtime fixture manifest of this checkout.
fn is_runtime_fixture_manifest(manifest: &Path) -> bool {
    let expected = runtime_fixture_project_root().join("runtime_semantics.bproj");
    manifest.canonicalize().ok().zip(expected.canonicalize().ok()).is_some_and(|(actual, expected)| actual == expected)
}

pub(super) fn attach_runtime_fixture(
    mut assembly: ProgramAssembly,
    plan: &CompilePlan,
) -> Result<ProgramAssembly, AssemblyError> {
    let failure = |message: &str| AssemblyError::Parse { path: plan.manifest_path.clone(), message: message.into() };
    let Some(proof) = prove_runtime_fixture(
        &plan.manifest_path,
        &plan.target.name,
        plan.target.entry.as_deref().unwrap_or(""),
        &assembly.entry_unit().source,
    )
    .map_err(|_| failure("runtime fixture manifest or source differs from the compiler-declared text"))?
    else {
        return Ok(assembly);
    };
    let runtime_root = runtime_fixture_project_root()
        .join("../..")
        .canonicalize()
        .map_err(|_| failure("canonical runtime source root is unavailable"))?;
    // The fixture manifest's `corelib` dependency resolves beside the runtime package; the
    // Foundation source root is anchored to the same compiler-owned checkout, never to the plan.
    let foundation_root = runtime_fixture_project_root()
        .join("../../../../corelib/packages/foundation/src")
        .canonicalize()
        .map_err(|_| failure("canonical Foundation source root is unavailable"))?;
    let expected = canonical_runtime_sources();
    verify_runtime_closure(&runtime_root, &foundation_root, &expected, &proof)
        .map_err(|()| failure("canonical runtime source closure differs from the compiler-embedded corpus"))?;

    // The canonical Foundation units replace the same files of the Foundation package the
    // assembly actually reads: the checkout root without a prepared workspace, or the workspace's
    // materialized copy of it. Placing them at the checkout path while a materialized Foundation
    // root is assembled would admit a second declaration of every canonical module
    // (`Core.Results` declared twice), outside any verified package root.
    let assembled_foundation_root = assembly
        .roots
        .dependencies
        .iter()
        .find(|root| root.dependency_name.as_deref() == Some(FOUNDATION_DEPENDENCY_NAME))
        .ok_or_else(|| failure("runtime fixture requires the Corelib Foundation dependency"))?
        .source_root
        .clone();
    let entry_path = assembly.entry_unit().path.clone();
    let native_root = assembly
        .roots
        .dependencies
        .iter()
        .find(|root| root.dependency_name.as_deref() == Some("beskid-runtime-native"))
        .ok_or_else(|| failure("runtime fixture requires the real native runtime dependency"))?
        .source_root
        .clone();
    // Every canonical unit enters the fixture assembly exactly once. Runtime-root units replace
    // the plan's native dependency units; Foundation support units (`Core/...`) replace the same
    // files the assembled Foundation root already contributed, at that root's paths, so
    // `use Core.Results;` and the private runtime scope name one declaration, not two copies.
    let foundation_roots = [assembled_foundation_root.clone(), assembled_foundation_root.canonicalize().unwrap_or_default()];
    let canonical_paths = expected
        .iter()
        .filter(|source| !source.logical_path.starts_with(RUNTIME_ROOT_LOGICAL_PREFIX))
        .flat_map(|source| foundation_roots.iter().map(move |root| root.join(&source.logical_path)))
        .collect::<std::collections::HashSet<_>>();
    let mut units = assembly
        .units
        .iter()
        .filter(|unit| !unit.path.starts_with(&native_root) && !canonical_paths.contains(&unit.path))
        .cloned()
        .collect::<Vec<_>>();
    for source in expected {
        let path = canonical_source_path(&runtime_root, &assembled_foundation_root, &source.logical_path);
        let program =
            crate::services::parse_program_with_source_name(path.to_str().unwrap_or_default(), &source.source)
                .map_err(|error| AssemblyError::Parse { path: path.clone(), message: error.to_string() })?;
        units.push(SourceUnit {
            logical_name: source.logical_path,
            origin_path: path.clone(),
            path,
            source: source.source,
            program,
        });
    }
    assembly.entry_index = units
        .iter()
        .position(|unit| unit.path == entry_path)
        .ok_or_else(|| failure("runtime fixture entry disappeared"))?;
    units[assembly.entry_index].logical_name = proof.fixture().logical_path.clone();
    assembly.roots.dependencies.push(RootEntry {
        dependency_name: Some("beskid-runtime-native".into()),
        source_root: runtime_root.join("src"),
    });
    let indexes =
        units.iter().map(|unit| SyntaxIndex::from_program(&unit.program, assembly.generation)).collect::<Vec<_>>();
    assembly.module_index = Arc::new(ModuleIndex::build(&units, &indexes, &assembly.roots, plan));
    assembly.units = Arc::new(units);
    assembly.syntax_indexes = Arc::new(indexes);
    assembly.runtime_fixture = Some(Arc::new(proof));
    Ok(assembly)
}

/// The on-disk identity of one canonical unit: runtime-root units (`src/...`) live under the native
/// runtime package, every other canonical unit is a Foundation support source.
fn canonical_source_path(runtime_root: &Path, foundation_root: &Path, logical_path: &str) -> PathBuf {
    if logical_path.starts_with(RUNTIME_ROOT_LOGICAL_PREFIX) {
        runtime_root.join(logical_path)
    } else {
        foundation_root.join(logical_path)
    }
}

/// Verify the compiler-embedded runtime closure against its two exact on-disk owners.
///
/// Every expected unit belongs to exactly one partition. Runtime-root units (`src/...`) must equal
/// the complete `.bd` set under `runtime/beskid/src`, byte for byte, with no missing or extra file.
/// Foundation units (the declared runtime support closure plus the public `Core/Dynamic` facade)
/// must each be a regular file under the Foundation package source root with identical bytes.
/// Any unit outside both partitions, a duplicate logical path, or a symlink fails closed.
fn verify_runtime_closure(
    runtime_root: &Path,
    foundation_root: &Path,
    expected: &[CanonicalSourceUnit],
    proof: &beskid_abi::runtime_source::RuntimeFixtureProof,
) -> Result<(), ()> {
    let foundation_paths = canonical_runtime_support_sources()
        .into_iter()
        .map(|source| source.logical_path)
        .chain(std::iter::once(CANONICAL_PUBLIC_DYNAMIC_SOURCE_PATH.to_string()))
        .collect::<BTreeSet<_>>();
    let mut runtime_units = BTreeSet::new();
    let mut foundation_units = BTreeSet::new();
    for source in expected {
        let fresh = if source.logical_path.starts_with(RUNTIME_ROOT_LOGICAL_PREFIX) {
            !foundation_paths.contains(&source.logical_path)
                && runtime_units.insert(runtime_root.join(&source.logical_path))
        } else if foundation_paths.contains(&source.logical_path) {
            foundation_units.insert(source.logical_path.clone())
        } else {
            false
        };
        if !fresh {
            return Err(());
        }
        let root = if source.logical_path.starts_with(RUNTIME_ROOT_LOGICAL_PREFIX) { runtime_root } else { foundation_root };
        let path = root.join(&source.logical_path);
        let regular = std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_file());
        if !regular || std::fs::read_to_string(&path).ok().as_deref() != Some(proof.source_file_text(source)) {
            return Err(());
        }
    }
    if foundation_units != foundation_paths {
        return Err(());
    }
    let mut on_disk = BTreeSet::new();
    collect_sources(&runtime_root.join("src"), &mut on_disk).map_err(|_| ())?;
    if on_disk != runtime_units {
        return Err(());
    }
    Ok(())
}

/// Enumerate every `.bd` file under `root` without following links; a symlink anywhere in the
/// runtime source tree is rejected rather than resolved.
fn collect_sources(root: &Path, paths: &mut BTreeSet<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_symlink() {
            return Err(std::io::Error::other(format!("runtime source tree contains a symlink: {}", path.display())));
        }
        if file_type.is_dir() {
            collect_sources(&path, paths)?;
        } else if path.extension().is_some_and(|extension| extension == "bd") {
            paths.insert(path);
        }
    }
    Ok(())
}
