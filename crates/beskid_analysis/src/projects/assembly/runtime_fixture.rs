//! Assemble a declared fixture beside, never inside, the exact production runtime corpus.

use super::{AssemblyError, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit};
use crate::projects::CompilePlan;
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

    let entry_path = assembly.entry_unit().path.clone();
    let native_root = assembly
        .roots
        .dependencies
        .iter()
        .find(|root| root.dependency_name.as_deref() == Some("beskid-runtime-native"))
        .ok_or_else(|| failure("runtime fixture requires the real native runtime dependency"))?
        .source_root
        .clone();
    let mut units =
        assembly.units.iter().filter(|unit| !unit.path.starts_with(&native_root)).cloned().collect::<Vec<_>>();
    for source in expected {
        let path = runtime_root.join(&source.logical_path);
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
