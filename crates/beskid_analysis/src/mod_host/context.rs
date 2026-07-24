//! Populate `beskid_abi::mod_contract` context structs from host compilation state.
//!
//! Owned string storage keeps C-layout views valid for the lifetime of
//! [`ModInvocationContext`]. Stub fields use empty strings or zero ids until native
//! marshaling reads live session fingerprints.

use beskid_abi::{
    BeskidStr, ModCatalog, ModCollectRequest, ModCollectTargetSet, ModCompilation, ModContractRegistration,
    ModContractRegistrationSlice, ModGenerationRequest, ModPackage, ModPackageSlice, ModStrSlice, ModWorkspace,
    ModWorkspaceMember, ModWorkspaceMemberSlice,
};

use super::types::{LoadedModArtifact, ModHostInput};

/// Owned mod-invocation context with stable C-layout views for native entrypoints.
#[derive(Debug)]
pub struct ModInvocationContext {
    arena: ContextArena,
    pub collect_request: ModCollectRequest,
}

#[derive(Debug, Default)]
struct ContextArena {
    strings: Vec<String>,
    workspace_members: Vec<ModWorkspaceMember>,
    package_rows: Vec<ModPackage>,
    package_capabilities: Vec<Vec<BeskidStr>>,
    package_registrations: Vec<Vec<ModContractRegistration>>,
    generation_targets: Vec<BeskidStr>,
}

impl ModInvocationContext {
    /// Build context from the active compile plan, entry source, and loaded mod artifacts.
    pub(crate) fn build(input: &ModHostInput<'_>, loaded: &[LoadedModArtifact]) -> Self {
        let mut arena = ContextArena::default();
        let target_triple = loaded
            .iter()
            .find_map(|artifact| artifact.descriptor.as_ref())
            .map(|descriptor| descriptor.target_triple.as_str());
        let compilation = arena.compilation(input, target_triple);
        let workspace = arena.workspace(input, loaded);
        let mods = arena.catalog(loaded);
        Self { collect_request: ModCollectRequest { compilation, workspace, mods }, arena }
    }

    /// Empty context for tests and pre-plan invocations.
    pub fn empty() -> Self {
        Self::build(
            &ModHostInput {
                compile_plan: None,
                source_name: "",
                source: "",
                pipeline: None,
                invoker: None,
                cached_target_fingerprint: None,
            },
            &[],
        )
    }

    /// `GenerationRequest` ABI view with the supplied target ids.
    pub fn generation_request(&mut self, target_ids: &[String]) -> ModGenerationRequest {
        self.arena.generation_targets = target_ids.iter().map(|target| self.arena.intern(target)).collect();
        ModGenerationRequest {
            context: self.collect_request,
            targets: ModCollectTargetSet {
                target_ids: ModStrSlice {
                    items: self.arena.generation_targets.as_ptr(),
                    len: self.arena.generation_targets.len(),
                },
            },
        }
    }
}

impl ContextArena {
    fn compilation(&mut self, input: &ModHostInput<'_>, target_triple: Option<&str>) -> ModCompilation {
        let (active_project_name, active_project_root) = match input.compile_plan {
            Some(plan) => (self.intern(&plan.project_name), self.intern_path(&plan.project_root)),
            None => (self.intern(""), self.intern("")),
        };
        ModCompilation {
            active_project_name,
            active_project_root,
            target_triple: self.intern(target_triple.unwrap_or("")),
            syntax_generation_id: 0,
            entry_source_path: self.intern(input.source_name),
            entry_source_name: self.intern(input.source_name),
        }
    }

    fn workspace(&mut self, input: &ModHostInput<'_>, loaded: &[LoadedModArtifact]) -> ModWorkspace {
        let root_path = match input.compile_plan {
            Some(plan) => self.intern_path(&plan.project_root),
            None => self.intern(""),
        };
        let lock_hash = loaded
            .iter()
            .find_map(|artifact| artifact.descriptor.as_ref())
            .map(|descriptor| self.intern(&descriptor.lock_hash))
            .unwrap_or_else(|| self.intern(""));

        if let Some(plan) = input.compile_plan {
            self.workspace_members = plan
                .dependency_projects
                .iter()
                .map(|dependency| ModWorkspaceMember {
                    member_id: self.intern(&dependency.dependency_name),
                    project_name: self.intern(&dependency.project_name),
                    project_root: self.intern_path(&dependency.project_root),
                    source_root: self.intern_path(&dependency.source_root),
                })
                .collect();
        }

        ModWorkspace {
            root_path,
            members: ModWorkspaceMemberSlice {
                items: self.workspace_members.as_ptr(),
                len: self.workspace_members.len(),
            },
            lock_hash,
        }
    }

    fn catalog(&mut self, loaded: &[LoadedModArtifact]) -> ModCatalog {
        self.package_rows.clear();
        self.package_capabilities.clear();
        self.package_registrations.clear();

        for artifact in loaded {
            let descriptor = artifact.descriptor.as_ref();
            let package_id =
                descriptor.map(|value| value.package_id.as_str()).unwrap_or(artifact.discovered.project_name.as_str());
            let package_version = descriptor.and_then(|value| value.package_version.as_deref()).unwrap_or("");
            let descriptor_path =
                descriptor.map(|value| self.intern_path(&value.sidecar_path())).unwrap_or_else(|| self.intern(""));

            let capability_strings = artifact
                .discovered
                .mod_section
                .as_ref()
                .and_then(|section| section.capabilities.as_ref())
                .cloned()
                .unwrap_or_default();
            let capabilities = capability_strings.iter().map(|cap| self.intern(cap)).collect::<Vec<_>>();
            let registrations = artifact
                .registrations
                .iter()
                .map(|registration| ModContractRegistration {
                    contract_id: self.intern(&registration.contract_id),
                    type_id: self.intern(&registration.type_id),
                    entry_symbol: self.intern(&registration.entry_symbol),
                })
                .collect::<Vec<_>>();

            let package_id = self.intern(package_id);
            let package_version = self.intern(package_version);
            let project_name = self.intern(&artifact.discovered.project_name);
            let project_root = self.intern_path(&artifact.discovered.project_root);
            let source_root = self.intern_path(&artifact.discovered.source_root);
            let manifest_path = self.intern_path(&artifact.discovered.manifest_path);

            self.package_capabilities.push(capabilities);
            self.package_registrations.push(registrations);
            let capabilities = self.package_capabilities.last().expect("package capabilities");
            let registrations = self.package_registrations.last().expect("package registrations");
            self.package_rows.push(ModPackage {
                package_id,
                package_version,
                project_name,
                project_root,
                source_root,
                manifest_path,
                descriptor_path,
                capabilities: ModStrSlice { items: capabilities.as_ptr(), len: capabilities.len() },
                registrations: ModContractRegistrationSlice { items: registrations.as_ptr(), len: registrations.len() },
            });
        }

        ModCatalog { packages: ModPackageSlice { items: self.package_rows.as_ptr(), len: self.package_rows.len() } }
    }

    fn intern(&mut self, value: &str) -> BeskidStr {
        self.strings.push(value.to_owned());
        let stored = self.strings.last().expect("interned string");
        BeskidStr { ptr: stored.as_ptr(), len: stored.len() }
    }

    fn intern_path(&mut self, path: &std::path::Path) -> BeskidStr {
        self.intern(&path.to_string_lossy())
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
            has_std_dependency: false,
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
                schema_version: 1,
                package_id: "ModA".to_owned(),
                package_version: Some("0.1.0".to_owned()),
                mod_source_hash: "hash".to_owned(),
                lock_hash: "lock123".to_owned(),
                target_triple: "test-triple".to_owned(),
                compiler_version: "test".to_owned(),
                object_file: "mod.o".to_owned(),
                registrations: Vec::new(),
                artifact_dir: PathBuf::from("/ws/host/.beskid/obj/mods/ModA"),
            }),
            registrations: Vec::new(),
        }];
        let input = ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source: "unit Main() { return; }\n",
            pipeline: None,
            invoker: None,
            cached_target_fingerprint: None,
        };

        let context = ModInvocationContext::build(&input, &loaded);
        assert_eq!(context.arena.workspace_members.len(), 1);
        assert_eq!(context.arena.package_rows.len(), 1);
        assert_eq!(context.collect_request.workspace.members.len, 1);
        assert_eq!(context.collect_request.mods.packages.len, 1);
    }
}
