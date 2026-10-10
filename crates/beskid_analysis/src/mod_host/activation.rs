//! Mod activation: which `type = Mod` dependencies take part in one host compilation.
//!
//! A Mod contributes to exactly the packages that declare it, in the manner of a Rust
//! procedural-macro crate, which expands only inside the crates that depend on it. The host
//! compiles one assembly: the import closure of its entry (or the workspace scan of an entry-less
//! library). A package takes part in that compilation only when the assembly holds at least one of
//! its source units, so a Mod is **active** for the host when it is a direct dependency of
//! the host project itself or of a package with a unit in the host assembly. The host project is
//! always part of its own compilation.
//!
//! Every other Mod in the resolved graph is inactive: it is never discovered, loaded, built or
//! run for this compilation. A Mod that only another Mod depends on belongs to that Mod's own
//! build, never to a host that runs the outer Mod. A Corelib aggregate pulls in
//! `corelib_serialization` and therefore `serialization_mod`, but a program whose imports never
//! reach `Core.Serialization` does not run the Serialization Mod; a program that imports that
//! surface (directly or through a package that does) runs it.
//!
//! The rule is decided from the compile plan (its resolved dependency projects), the declared
//! dependencies of the host and of each reached package (their own manifests), and the assembly
//! (reached packages) only, so the CLI, which builds the active Mods before it prepares the host,
//! and the prepare spine, which loads them, always agree on the same set. A synthetic plan (a
//! single file, REPL or engine input) has no root manifest: its root declares nothing, and only the
//! packages it reaches can activate a Mod.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::projects::{
    CompilePlan, DependencySource, ProgramAssembly, ProjectKind, ResolvedDependencyProject, load_manifest_from_path,
};

/// Canonical manifest paths of the Mods active for the compilation of `plan` whose assembly is
/// `assembly`, in stable order.
pub fn active_mod_manifests(plan: &CompilePlan, assembly: &ProgramAssembly) -> Result<Vec<PathBuf>> {
    let mods = plan_mods(plan)?;
    if mods.is_empty() {
        return Ok(Vec::new());
    }
    active_mods(plan, &mods, &reached_packages(plan, assembly))
}

/// `plan` with every inactive `type = Mod` dependency removed; every other dependency, the
/// target and the roots are unchanged. The Mod host discovers, loads and schedules Mods from this
/// plan, so an inactive Mod never needs an artifact.
pub fn active_mod_plan(plan: &CompilePlan, assembly: &ProgramAssembly) -> Result<CompilePlan> {
    let mods = plan_mods(plan)?;
    if mods.is_empty() {
        return Ok(plan.clone());
    }
    let active = active_mods(plan, &mods, &reached_packages(plan, assembly))?.into_iter().collect::<BTreeSet<_>>();
    let mut narrowed = plan.clone();
    narrowed.dependency_projects.retain(|dependency| {
        canonical_manifest(&dependency.manifest_path)
            .map(|manifest| !mods.contains_key(&manifest) || active.contains(&manifest))
            .unwrap_or(true)
    });
    Ok(narrowed)
}

/// Active Mod manifests for a resolved input, assembled exactly as the prepare spine assembles it
/// for `front_end` and `native_mod_adapter_sources`. A caller that must provide the active Mods
/// before preparing (the CLI builds them) uses this; it reuses an assembly the input already holds.
pub fn active_mod_manifests_for_input(
    resolved: &crate::services::ResolvedInput,
    front_end: &crate::services::FrontEndOptions,
    native_mod_adapter_sources: bool,
) -> Result<Vec<PathBuf>> {
    let Some(plan) = resolved.compile_plan.as_ref() else {
        return Ok(Vec::new());
    };
    let mods = plan_mods(plan)?;
    if mods.is_empty() {
        return Ok(Vec::new());
    }
    let reached = match resolved.assembly.as_ref() {
        Some(assembly) => reached_packages(plan, assembly),
        None => {
            let mut options = crate::projects::assembly_options_for_prepare(plan, front_end.assembly_discovery);
            options.recovery_policy = front_end.assembly_recovery;
            options.native_mod_adapter_sources = native_mod_adapter_sources;
            let assembly = crate::projects::assemble_program(
                plan,
                resolved.prepared_workspace.as_ref(),
                &resolved.source_path,
                Some(&resolved.source),
                &options,
                None,
            )
            .map_err(|error| anyhow::anyhow!("{error}"))
            .with_context(|| format!("assemble {} to decide its active Mods", resolved.source_path.display()))?;
            reached_packages(plan, &assembly)
        }
    };
    active_mods(plan, &mods, &reached)
}

/// Every `type = Mod` project of the plan, by canonical manifest path.
fn plan_mods(plan: &CompilePlan) -> Result<BTreeMap<PathBuf, &ResolvedDependencyProject>> {
    let mut mods = BTreeMap::new();
    for dependency in &plan.dependency_projects {
        let manifest_path = canonical_manifest(&dependency.manifest_path)?;
        if mods.contains_key(&manifest_path) {
            continue;
        }
        let manifest = load_manifest_from_path(&manifest_path)
            .map_err(anyhow::Error::from)
            .with_context(|| format!("load dependency manifest {} for Mod activation", manifest_path.display()))?;
        if manifest.project.kind == ProjectKind::Mod {
            mods.insert(manifest_path, dependency);
        }
    }
    Ok(mods)
}

/// Package names (project names) of every unit in `assembly`, plus the host project.
fn reached_packages(plan: &CompilePlan, assembly: &ProgramAssembly) -> BTreeSet<String> {
    let mut reached = assembly
        .module_index
        .module_graph()
        .modules()
        .iter()
        .map(|module| module.package.clone())
        .collect::<BTreeSet<_>>();
    reached.insert(plan.project_name.clone());
    reached
}

/// The Mods declared directly by the host or by a reached non-Mod dependency package.
fn active_mods(
    plan: &CompilePlan,
    mods: &BTreeMap<PathBuf, &ResolvedDependencyProject>,
    reached: &BTreeSet<String>,
) -> Result<Vec<PathBuf>> {
    let mut active = BTreeSet::new();
    // A synthetic plan has no root manifest, so its root declares no dependency of its own.
    if plan.manifest_path.is_file() {
        declared_mods(&plan.manifest_path, &plan.project_root, mods, &mut active)?;
    }
    let mut seen = BTreeSet::new();
    for dependency in &plan.dependency_projects {
        if !reached.contains(&dependency.project_name) {
            continue;
        }
        let manifest_path = canonical_manifest(&dependency.manifest_path)?;
        // A Mod's own dependencies belong to that Mod's build; its sources are never part of a
        // host assembly.
        if mods.contains_key(&manifest_path) || !seen.insert(manifest_path.clone()) {
            continue;
        }
        declared_mods(&manifest_path, &dependency.project_root, mods, &mut active)?;
    }
    Ok(active.into_iter().collect())
}

/// Add the Mods that the manifest at `manifest_path` declares directly. A path dependency is
/// identified by its resolved manifest; a registry or git dependency, which the resolver
/// materializes under its declared name, by that name.
fn declared_mods(
    manifest_path: &Path,
    project_root: &Path,
    mods: &BTreeMap<PathBuf, &ResolvedDependencyProject>,
    active: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let manifest = load_manifest_from_path(manifest_path)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("load manifest {} for Mod activation", manifest_path.display()))?;
    for declared in &manifest.dependencies {
        let path_target = match (&declared.source, &declared.path) {
            (DependencySource::Path, Some(relative)) => {
                crate::projects::graph::pathing::dependency_manifest_path(project_root, relative)
                    .ok()
                    .and_then(|path| path.canonicalize().ok())
            }
            _ => None,
        };
        for (mod_manifest, mod_dependency) in mods {
            let declared_here = match &path_target {
                Some(target) => target == mod_manifest,
                None => declared.source != DependencySource::Path && mod_dependency.dependency_name == declared.name,
            };
            if declared_here {
                active.insert(mod_manifest.clone());
            }
        }
    }
    Ok(())
}

fn canonical_manifest(path: &Path) -> Result<PathBuf> {
    path.canonicalize().with_context(|| format!("resolve dependency manifest {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::projects::{AssemblyDiscovery, AssemblyOptions, build_compile_plan};

    /// Host -> Lib -> ModA (Lib declares the Mod), Host -> Facade -> ModB, ModA -> ModC.
    fn workspace(host_source: &str) -> (tempfile::TempDir, CompilePlan) {
        let root = tempfile::tempdir().expect("temp root");
        let write = |path: &Path, text: &str| {
            fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            fs::write(path, text).expect("write");
        };
        let mod_manifest = |name: &str, extra: &str| {
            format!(
                "{name} {{\n  name = \"{name}\"\n  version = \"0.1.0\"\n  type = Mod\n  root = \"Src\"\n  mod {{ capabilities = [emit_syntax] }}\n}}\n{extra}"
            )
        };
        write(&root.path().join("ModC/ModC.bproj"), &mod_manifest("ModC", ""));
        write(&root.path().join("ModC/Src/Mod.bd"), "pub unit C() { return; }\n");
        write(
            &root.path().join("ModA/ModA.bproj"),
            &mod_manifest("ModA", "dependency \"ModC\" { source = path path = \"../ModC\" }\n"),
        );
        write(&root.path().join("ModA/Src/Mod.bd"), "pub unit A() { return; }\n");
        write(&root.path().join("ModB/ModB.bproj"), &mod_manifest("ModB", ""));
        write(&root.path().join("ModB/Src/Mod.bd"), "pub unit B() { return; }\n");
        for (name, mod_name) in [("Lib", "ModA"), ("Facade", "ModB")] {
            write(
                &root.path().join(format!("{name}/{name}.bproj")),
                &format!(
                    "{name} {{\n  name = \"{name}\"\n  version = \"0.1.0\"\n  root = \"Src\"\n}}\ndependency \"{mod_name}\" {{ source = path path = \"../{mod_name}\" }}\ntarget \"lib\" {{ kind = Lib }}\n"
                ),
            );
            write(
                &root.path().join(format!("{name}/Src/{name}/Api.bd")),
                "pub unit Touch() { return; }\n",
            );
        }
        write(
            &root.path().join("Host/Host.bproj"),
            "Host {\n  name = \"Host\"\n  version = \"0.1.0\"\n  root = \"Src\"\n}\ndependency \"Lib\" { source = path path = \"../Lib\" }\ndependency \"Facade\" { source = path path = \"../Facade\" }\ntarget \"main\" { kind = App entry = \"Main.bd\" }\n",
        );
        write(&root.path().join("Host/Src/Main.bd"), host_source);
        let plan = build_compile_plan(&root.path().join("Host/Host.bproj"), None).expect("host plan");
        (root, plan)
    }

    fn assembly(plan: &CompilePlan) -> ProgramAssembly {
        let entry = plan.source_root.join("Main.bd");
        let source = fs::read_to_string(&entry).expect("entry");
        let options = AssemblyOptions { discovery: AssemblyDiscovery::ImportClosure, ..AssemblyOptions::default() };
        crate::projects::assemble_program(plan, None, &entry, Some(&source), &options, None).expect("assembly")
    }

    fn names(manifests: &[PathBuf]) -> Vec<String> {
        manifests
            .iter()
            .map(|path| path.file_stem().and_then(|stem| stem.to_str()).expect("stem").to_owned())
            .collect()
    }

    #[test]
    fn unreached_library_mods_are_inactive() {
        let (_root, plan) = workspace("unit Main() { return; }\n");
        let assembly = assembly(&plan);
        assert!(active_mod_manifests(&plan, &assembly).expect("activation").is_empty());
        let narrowed = active_mod_plan(&plan, &assembly).expect("narrowed plan");
        let kept = narrowed.dependency_projects.iter().map(|dependency| dependency.project_name.as_str()).collect::<Vec<_>>();
        assert!(kept.contains(&"Lib") && kept.contains(&"Facade"), "libraries stay in the plan: {kept:?}");
        assert!(!kept.iter().any(|name| name.starts_with("Mod")), "inactive Mods leave the plan: {kept:?}");
    }

    #[test]
    fn a_reached_library_activates_only_its_own_mods() {
        let (_root, plan) = workspace("use Lib.Api;\nunit Main() { Api.Touch(); return; }\n");
        let assembly = assembly(&plan);
        assert_eq!(names(&active_mod_manifests(&plan, &assembly).expect("activation")), ["ModA"]);
        let narrowed = active_mod_plan(&plan, &assembly).expect("narrowed plan");
        let mods = narrowed
            .dependency_projects
            .iter()
            .filter(|dependency| dependency.project_name.starts_with("Mod"))
            .map(|dependency| dependency.project_name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(mods, ["ModA"], "a Mod's own Mod dependency (ModC) belongs to that Mod's build");
    }

    #[test]
    fn a_direct_host_mod_dependency_is_always_active() {
        let (root, _) = workspace("unit Main() { return; }\n");
        fs::write(
            root.path().join("Host/Host.bproj"),
            "Host {\n  name = \"Host\"\n  version = \"0.1.0\"\n  root = \"Src\"\n}\ndependency \"ModB\" { source = path path = \"../ModB\" }\ntarget \"main\" { kind = App entry = \"Main.bd\" }\n",
        )
        .expect("host manifest");
        let plan = build_compile_plan(&root.path().join("Host/Host.bproj"), None).expect("host plan");
        assert_eq!(names(&active_mod_manifests(&plan, &assembly(&plan)).expect("activation")), ["ModB"]);
    }
}
