//! Owned, typed invocation values. Native transport never borrows caller pointer views.
use super::types::{LoadedModArtifact, ModHostInput};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModCollectRequest {
    pub compilation: ModCompilation,
    pub workspace: ModWorkspace,
    pub mods: ModCatalog,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModCompilation {
    pub active_project_name: String,
    pub active_project_root: String,
    pub target_triple: String,
    pub syntax_generation_id: u64,
    pub entry_source_path: String,
    pub entry_source_name: String,
    #[serde(skip)]
    pub entry_source_text: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModWorkspace {
    pub root_path: String,
    pub members: Vec<ModWorkspaceMember>,
    pub lock_hash: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModWorkspaceMember {
    pub member_id: String,
    pub project_name: String,
    pub project_root: String,
    pub source_root: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct ModCatalog {
    pub packages: Vec<ModPackage>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModPackage {
    pub package_id: String,
    pub package_version: String,
    pub project_name: String,
    pub project_root: String,
    pub source_root: String,
    pub manifest_path: String,
    pub descriptor_path: String,
    pub capabilities: Vec<String>,
    pub registrations: Vec<ModContractRegistration>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModContractRegistration {
    pub contract_id: String,
    pub type_id: String,
    pub entry_symbol: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct ModGenerationRequest {
    pub context: ModCollectRequest,
    pub targets: ModCollectTargetSet,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModCollectTargetSet {
    pub target_ids: Vec<String>,
}
#[derive(Debug)]
pub struct ModInvocationContext {
    pub collect_request: ModCollectRequest,
}
fn path(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}
impl ModInvocationContext {
    pub(crate) fn build(input: &ModHostInput<'_>, loaded: &[LoadedModArtifact]) -> Self {
        let descriptor = loaded.iter().find_map(|a| a.descriptor.as_ref());
        let compilation = ModCompilation {
            active_project_name: input.compile_plan.map(|p| p.project_name.clone()).unwrap_or_default(),
            active_project_root: input.compile_plan.map(|p| path(&p.project_root)).unwrap_or_default(),
            target_triple: descriptor.map(|d| d.target_triple.clone()).unwrap_or_default(),
            syntax_generation_id: input.syntax_generation_id.map_or(0, |g| g.0),
            entry_source_path: input.source_name.into(),
            entry_source_name: input.source_name.into(),
            entry_source_text: input.source.into(),
        };
        let workspace = ModWorkspace {
            root_path: input.compile_plan.map(|p| path(&p.project_root)).unwrap_or_default(),
            lock_hash: descriptor.map(|d| d.lock_hash.clone()).unwrap_or_default(),
            members: input
                .compile_plan
                .map(|p| {
                    p.dependency_projects
                        .iter()
                        .map(|d| ModWorkspaceMember {
                            member_id: d.dependency_name.clone(),
                            project_name: d.project_name.clone(),
                            project_root: path(&d.project_root),
                            source_root: path(&d.source_root),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        };
        let mods = ModCatalog {
            packages: loaded
                .iter()
                .map(|a| {
                    let d = a.descriptor.as_ref();
                    ModPackage {
                        package_id: d
                            .map(|d| d.package_id.clone())
                            .unwrap_or_else(|| a.discovered.project_name.clone()),
                        package_version: d.and_then(|d| d.package_version.clone()).unwrap_or_default(),
                        project_name: a.discovered.project_name.clone(),
                        project_root: path(&a.discovered.project_root),
                        source_root: path(&a.discovered.source_root),
                        manifest_path: path(&a.discovered.manifest_path),
                        descriptor_path: d.map(|d| path(&d.sidecar_path())).unwrap_or_default(),
                        capabilities: a
                            .discovered
                            .mod_section
                            .as_ref()
                            .and_then(|s| s.capabilities.clone())
                            .unwrap_or_default(),
                        registrations: a
                            .registrations
                            .iter()
                            .map(|r| ModContractRegistration {
                                contract_id: r.contract_id.clone(),
                                type_id: r.type_id.clone(),
                                entry_symbol: r.entry_symbol.clone(),
                            })
                            .collect(),
                    }
                })
                .collect(),
        };
        Self { collect_request: ModCollectRequest { compilation, workspace, mods } }
    }
    pub fn empty() -> Self {
        Self::build(&ModHostInput::default(), &[])
    }
    pub fn generation_request(&mut self, target_ids: &[String]) -> ModGenerationRequest {
        ModGenerationRequest {
            context: self.collect_request.clone(),
            targets: ModCollectTargetSet { target_ids: target_ids.to_vec() },
        }
    }
}
#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::mod_host::types::{DiscoveredMod, LoadedModArtifact, ModArtifactDescriptor};
    use crate::projects::{CompilePlan, ResolvedDependencyProject, Target, TargetKind};

    use super::*;

    #[test]
    fn builds_collect_request_from_compile_plan_and_loaded_mods() {
        let plan = CompilePlan {
            project_root: PathBuf::from("/ws/host"),
            manifest_path: PathBuf::from("/ws/host/Host.bproj"),
            project_name: "Host".to_owned(),
            source_root: PathBuf::from("/ws/host/Src"),
            target: Target { name: "Host".to_owned(), kind: TargetKind::App, entry: Some("Main.bd".to_owned()) },
            dependency_projects: vec![ResolvedDependencyProject {
                dependency_name: "moda".to_owned(),
                manifest_path: PathBuf::from("/ws/ModA/ModA.bproj"),
                project_root: PathBuf::from("/ws/ModA"),
                project_name: "ModA".to_owned(),
                source_root: PathBuf::from("/ws/ModA/Src"),
            }],
            unresolved_dependencies: Vec::new(),
            has_core_dependency: false,
        };
        let loaded = vec![LoadedModArtifact {
            discovered: DiscoveredMod {
                dependency_name: "moda".to_owned(),
                project_name: "ModA".to_owned(),
                project_root: PathBuf::from("/ws/ModA"),
                manifest_path: PathBuf::from("/ws/ModA/ModA.bproj"),
                source_root: PathBuf::from("/ws/ModA/Src"),
                mod_section: None,
            },
            descriptor: Some(ModArtifactDescriptor {
                schema_version: 2,
                package_id: "ModA".to_owned(),
                package_version: Some("0.1.0".to_owned()),
                mod_source_hash: "hash".to_owned(),
                lock_hash: "lock123".to_owned(),
                target_triple: "test-triple".to_owned(),
                compiler_version: "test".to_owned(),
                executable_file: "mod.dylib".to_owned(),
                registrations: Vec::new(),
                artifact_dir: PathBuf::from("/ws/host/.beskid/obj/mods/ModA"),
                ..ModArtifactDescriptor::context_fixture()
            }),
            registrations: Vec::new(),
        }];
        let input = ModHostInput {
            semantic_scope: None,
            semantic_authority: None,
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source: "unit Main() { return; }\n",
            pipeline: None,
            invoker: None,
            cached_target_fingerprint: None,
            syntax_generation_id: None,
        };

        let context = ModInvocationContext::build(&input, &loaded);

        assert_eq!(context.collect_request.workspace.members.len(), 1);
        assert_eq!(context.collect_request.mods.packages.len(), 1);
    }

    #[test]
    fn compilation_interns_entry_source_text() {
        let plan = CompilePlan {
            project_root: PathBuf::from("/ws/host"),
            manifest_path: PathBuf::from("/ws/host/Host.bproj"),
            project_name: "Host".to_owned(),
            source_root: PathBuf::from("/ws/host/Src"),
            target: Target { name: "Host".to_owned(), kind: TargetKind::App, entry: Some("Main.bd".to_owned()) },
            dependency_projects: Vec::new(),
            unresolved_dependencies: Vec::new(),
            has_core_dependency: false,
        };
        let source = "unit Main() { return; }\n";
        let input = ModHostInput {
            semantic_scope: None,
            semantic_authority: None,
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: None,
            invoker: None,
            cached_target_fingerprint: None,
            syntax_generation_id: None,
        };

        let context = ModInvocationContext::build(&input, &[]);
        assert_eq!(
            context.collect_request.compilation.entry_source_text, source,
            "entry_source_text must intern input.source verbatim"
        );
    }

    #[test]
    fn empty_context_interns_empty_entry_source_text() {
        let context = ModInvocationContext::empty();
        assert_eq!(
            context.collect_request.compilation.entry_source_text, "",
            "empty context must intern an empty entry_source_text"
        );
    }
}

#[cfg(test)]
mod generation_authority_tests {
    use super::*;
    use crate::projects::{AssemblyOptions, CompilePlan, Target, TargetKind, assemble_program_with_materializer};

    #[test]
    fn invocation_context_carries_actual_assembly_generation() {
        let root = tempfile::tempdir().expect("disposable assembly project");
        std::fs::create_dir(root.path().join("Src")).unwrap();
        let path = root.path().join("Src/Main.bd");
        std::fs::write(&path, "pub unit Main() { return; }").unwrap();
        std::fs::write(root.path().join("Host.bproj"), "Host { name = \"Host\" version = \"0.1.0\" root = \"Src\" } target \"Main\" { kind = Lib entry = \"Main.bd\" }").unwrap();
        let plan = CompilePlan {
            project_root: root.path().to_owned(),
            manifest_path: root.path().join("Host.bproj"),
            project_name: "Host".to_owned(),
            source_root: root.path().join("Src"),
            target: Target { name: "Main".to_owned(), kind: TargetKind::Lib, entry: Some("Main.bd".to_owned()) },
            dependency_projects: vec![],
            unresolved_dependencies: vec![],
            has_core_dependency: false,
        };
        let assembly =
            assemble_program_with_materializer(&plan, None, &path, None, &AssemblyOptions::default(), None, None)
                .expect("real source assembly");
        let input = ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source: "pub unit Main() { return; }",
            syntax_generation_id: Some(assembly.generation),
            ..Default::default()
        };
        let mut context = ModInvocationContext::build(&input, &[]);
        let request = context.generation_request(&[]);
        assert_ne!(assembly.generation.0, 0);
        assert_eq!(request.context.compilation.syntax_generation_id, assembly.generation.0);
    }
}

#[cfg(test)]
mod owned_transport_tests {
    use super::*;
    #[test]
    fn v06_owned_context_clone_retains_strings_and_targets_without_pointer_views() {
        let mut context = ModInvocationContext::empty();
        context.collect_request.compilation.entry_source_name = "Owned.bd".into();
        let request = context.generation_request(&["first".into(), "second".into()]);
        context.collect_request.compilation.entry_source_name.clear();
        drop(context);
        assert_eq!(request.context.compilation.entry_source_name, "Owned.bd");
        assert_eq!(request.targets.target_ids, ["first", "second"]);
        let wire = serde_json::to_value(&request).unwrap();
        assert_eq!(wire["context"]["compilation"]["entrySourceName"], "Owned.bd");
        assert!(wire["context"]["compilation"].get("entrySourceText").is_none());
    }
}
