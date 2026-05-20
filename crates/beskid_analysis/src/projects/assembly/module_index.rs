//! Cross-unit module graph built by collection-only resolver passes.

use std::collections::{HashMap, HashSet};

use crate::hir::{AstProgram, HirProgram, lower_program as lower_hir_program};
use crate::resolve::{ItemId, ItemInfo, ModuleGraph, Resolution, ResolveResult, Resolver};
use crate::syntax::{Program, Spanned};

use super::SourceUnit;
use super::roots::EffectiveCompilationRoots;
use crate::projects::CompilePlan;

/// Items and module paths collected from non-entry compilation units.
#[derive(Debug, Clone)]
pub struct ModuleIndex {
    items: Vec<ItemInfo>,
    module_graph: ModuleGraph,
    builtin_items: HashMap<ItemId, usize>,
}

impl ModuleIndex {
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            module_graph: ModuleGraph::new_root(),
            builtin_items: HashMap::new(),
        }
    }

    /// Collect symbols from all units except `entry_index`.
    pub fn build(
        units: &[SourceUnit],
        entry_index: usize,
        roots: &EffectiveCompilationRoots,
        plan: &CompilePlan,
    ) -> Self {
        let mut resolver = Resolver::new();
        resolver.collect_builtins();

        for (index, unit) in units.iter().enumerate() {
            if index == entry_index {
                continue;
            }
            if let Ok(hir) = unit_to_hir(&unit.program) {
                if let Some(module_path) = infer_logical_module_path(unit, roots, plan) {
                    resolver.collect_program_in_module(&hir, &module_path);
                } else {
                    resolver.collect_program(&hir);
                }
            }
        }

        let (items, module_graph, builtin_items) = resolver.into_prefetch_parts();
        Self {
            items,
            module_graph,
            builtin_items,
        }
    }

    /// Logical module paths present in the prefetch graph (`Std::System::IO`, etc.).
    pub fn known_module_path_strings(&self) -> HashSet<String> {
        self.module_graph
            .modules()
            .iter()
            .filter(|module| !module.path.is_empty())
            .map(|module| module.path.join("::"))
            .collect()
    }

    /// Resolve the entry unit against prefetched external modules (lowers AST only; prefer [`Self::resolve_entry_hir`] when HIR is already normalized).
    pub fn resolve_entry(&self, entry_program: &Spanned<Program>) -> ResolveResult<Resolution> {
        let entry_hir = unit_to_hir(entry_program).map_err(|_| Vec::new())?;
        self.resolve_entry_hir(&entry_hir)
    }

    /// Resolve entry HIR against prefetched modules (spans must match the HIR passed to type checking).
    pub fn resolve_entry_hir(&self, entry_hir: &Spanned<HirProgram>) -> ResolveResult<Resolution> {
        let mut resolver = Resolver::with_module_prefetch(
            self.items.clone(),
            self.module_graph.clone(),
            self.builtin_items.clone(),
        );
        resolver.collect_program(entry_hir);
        resolver.resolve_collected_program(entry_hir)
    }
}

fn unit_to_hir(program: &Spanned<Program>) -> Result<Spanned<HirProgram>, ()> {
    let ast: Spanned<AstProgram> = program.clone().into();
    Ok(lower_hir_program(&ast))
}

fn infer_logical_module_path(
    unit: &SourceUnit,
    roots: &EffectiveCompilationRoots,
    plan: &CompilePlan,
) -> Option<Vec<String>> {
    let path = &unit.path;
    for root in std::iter::once(&roots.host).chain(roots.dependencies.iter()) {
        let Ok(rel) = path.strip_prefix(&root.source_root) else {
            continue;
        };
        let rel = rel.with_extension("");
        let segments: Vec<String> = rel
            .components()
            .filter_map(|component| match component {
                std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        if segments.is_empty() {
            continue;
        }
        if plan.has_std_dependency {
            let mut with_std = vec!["Std".to_string()];
            with_std.extend(segments);
            return Some(with_std);
        }
        return Some(segments);
    }
    None
}
