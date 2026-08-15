use std::collections::{HashMap, HashSet};

use crate::projects::CompilePlan;
use crate::syntax_query::SyntaxIndex;

use super::super::{roots::EffectiveCompilationRoots, SourceUnit};
use super::model::{AssemblyModule, ModuleGraph, ModuleIndex};
use super::path_inference::{infer_logical_module_path, package_for_unit};

impl ModuleIndex {
    /// Build the project module graph exclusively from expanded syntax facts.
    pub fn build(
        units: &[SourceUnit],
        syntax_indexes: &[SyntaxIndex],
        roots: &EffectiveCompilationRoots,
        plan: &CompilePlan,
    ) -> Self {
        let dependency_packages: HashMap<String, String> = plan
            .dependency_projects
            .iter()
            .map(|dependency| (dependency.dependency_name.clone(), dependency.project_name.clone()))
            .collect();
        let mut module_graph = ModuleGraph::default();
        let mut known_paths = HashSet::new();

        for (unit, syntax_index) in units.iter().zip(syntax_indexes) {
            let Some(path) = infer_logical_module_path(unit, roots, plan.has_std_dependency) else {
                continue;
            };
            known_paths.insert(path.clone());

            let imports = syntax_index.import_paths(&unit.program);
            let declarations = syntax_index.module_declaration_paths(&unit.program);
            known_paths.extend(declarations.iter().cloned());
            for inline_name in syntax_index.inline_module_names(&unit.program) {
                let mut inline_path = path.clone();
                inline_path.push(inline_name);
                known_paths.insert(inline_path);
            }

            module_graph.insert(AssemblyModule {
                path,
                source_path: unit.path.clone(),
                package: package_for_unit(unit, roots, &plan.project_name, &dependency_packages),
                imports,
                declarations,
            });
        }

        Self { module_graph, known_paths, prefetched_paths: Vec::new() }
    }
}
