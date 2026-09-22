//! Assemble a declared fixture beside, never inside, the exact production runtime corpus.

use super::{AssemblyError, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit};
use crate::projects::CompilePlan;
use crate::syntax_query::SyntaxIndex;
use beskid_abi::runtime_source::{canonical_runtime_sources, prove_runtime_fixture, runtime_fixture_project_root};
use std::{path::PathBuf, sync::Arc};

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
    let expected = canonical_runtime_sources();
    let mut on_disk = Vec::new();
    collect_sources(&runtime_root.join("src"), &mut on_disk)
        .map_err(|_| failure("cannot enumerate canonical runtime sources"))?;
    if on_disk.len() != expected.len()
        || expected.iter().any(|source| {
            std::fs::read_to_string(runtime_root.join(&source.logical_path)).ok().as_deref()
                != Some(proof.source_file_text(source))
        })
    {
        return Err(failure("canonical runtime source closure differs from the compiler-embedded corpus"));
    }

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

fn collect_sources(root: &std::path::Path, paths: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_sources(&path, paths)?;
        } else if path.extension().is_some_and(|extension| extension == "bd") {
            paths.push(path);
        }
    }
    Ok(())
}
