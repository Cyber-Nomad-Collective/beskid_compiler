use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use crate::resolve::{Resolution, ResolutionTables, ResolveResult, Resolver};
use crate::syntax::{Program, Spanned};

use super::super::ProgramAssembly;
use super::model::ModuleIndex;
use super::path_inference::infer_logical_module_path;

/// Declaration annotations of every unit of one assembly, resolved once with every unit
/// collected: units of one assembly see each other, whichever of them is being analyzed.
#[derive(Debug)]
pub struct AssemblyDeclarationTables {
    units: Arc<Vec<crate::projects::assembly::SourceUnit>>,
    tables: Vec<ResolutionTables>,
}

impl AssemblyDeclarationTables {
    /// Whether these tables were resolved for `assembly`'s exact unit list.
    fn describes(&self, assembly: &ProgramAssembly) -> bool {
        Arc::ptr_eq(&self.units, &assembly.units)
    }
}

impl ModuleIndex {
    /// Logical module paths present or explicitly declared in expanded syntax.
    pub fn known_module_path_strings(&self) -> HashSet<String> {
        self.known_paths.iter().filter(|path| !path.is_empty()).map(|path| path.join("::")).collect()
    }

    /// Resolve a complete module path to the assembled source that owns it.
    pub fn resolve_module(&self, path: &[String]) -> Option<&Path> {
        self.module_graph.module(path).map(|module| module.source_path.as_path())
    }

    /// Resolve one unit's expanded import paths against the assembled module graph.
    pub fn resolve_imports_for_source(&self, source_path: &Path) -> Vec<(Vec<String>, Option<&Path>)> {
        let Some(module) = self.module_graph.module_for_source(source_path) else {
            return Vec::new();
        };
        module.imports.iter().map(|import| (import.clone(), self.resolve_longest_module_prefix(import))).collect()
    }

    /// Resolve the entry program against the assembled module graph.
    ///
    /// Seeds the resolver with all known module paths and collects dependency
    /// units so that `use` declarations and cross-module references in the entry
    /// program resolve against the full assembled closure.
    ///
    /// `declarations` are the assembly's shared dependency declaration tables when the caller
    /// resolves several entries of one assembly; without them (or for another assembly) they are
    /// resolved here.
    pub fn resolve_entry_program(
        &self,
        program: &Spanned<Program>,
        entry_source_path: Option<&Path>,
        assembly: &ProgramAssembly,
        declarations: Option<&AssemblyDeclarationTables>,
    ) -> ResolveResult<Resolution> {
        let mut resolver = Resolver::new();
        let entry_source_path = entry_source_path.map(|path| path.to_path_buf());
        resolver.set_current_source_path(entry_source_path.clone());

        self.seed_resolver_from_assembly(&mut resolver, assembly, Some(assembly.entry_index));
        // The entry is still a module of the assembly: sibling declarations may name it by its
        // logical path (`Console.ConsoleSize` from `Platform/Terminal.bd`).
        let entry_unit = entry_source_path
            .as_ref()
            .and_then(|path| assembly.units.iter().find(|unit| crate::paths::same_file(&unit.path, path)))
            .unwrap_or_else(|| assembly.entry_unit());
        let entry_module_path = infer_logical_module_path(entry_unit, &assembly.roots);
        resolver.set_current_source_path(entry_source_path);

        let mut resolution = resolver.resolve_entry_program_in_module(program, entry_module_path.as_deref())?;
        self.merge_dependency_declaration_tables(&mut resolution, assembly, declarations);
        Ok(resolution)
    }

    /// Resolve a non-entry unit program against the assembled module graph.
    ///
    /// Like [`Self::resolve_entry_program`], seeds the resolver with all known
    /// module paths and collects sibling units so cross-module references resolve.
    pub fn resolve_unit_program(
        &self,
        program: &Spanned<Program>,
        unit_source_path: &Path,
        assembly: &ProgramAssembly,
    ) -> ResolveResult<Resolution> {
        let mut resolver = Resolver::new();
        resolver.set_current_source_path(Some(unit_source_path.to_path_buf()));

        self.seed_resolver_from_assembly(&mut resolver, assembly, Some(assembly.entry_index));

        resolver.resolve_program(program)
    }

    /// Seed `resolver` with all known module paths and collect every unit's items except
    /// `excluded` (the entry, which its own resolution collects) so `use` declarations and
    /// references resolve against the full assembled closure.
    fn seed_resolver_from_assembly(
        &self,
        resolver: &mut Resolver,
        assembly: &ProgramAssembly,
        excluded: Option<usize>,
    ) {
        let source_path = resolver.current_source_path.clone();
        if let Some(proof) = assembly.runtime_fixture.as_ref()
            && source_path.as_ref().is_some_and(|path| {
                assembly.units.iter().any(|unit| {
                    &unit.path == path
                        && (unit.logical_name.starts_with("src/Runtime/")
                            || unit.logical_name == proof.fixture().logical_path)
                })
            })
        {
            for unit in assembly.units.iter().filter(|unit| unit.logical_name.starts_with("src/Runtime/")) {
                for item in &unit.program.node.items {
                    if let crate::syntax::Node::ConstantDefinition(definition) = &item.node {
                        let value = definition.node.value.clone();
                        resolver
                            .shared_constants
                            .entry(definition.node.name.node.name.clone())
                            .and_modify(|previous| {
                                if previous.as_ref().is_some_and(|previous| previous.node != value.node) {
                                    *previous = None;
                                }
                            })
                            .or_insert(Some(value));
                    }
                }
            }
        }
        for path in &self.known_paths {
            resolver.module_graph.ensure_module_path(path);
        }
        for (index, unit) in assembly.units.iter().enumerate() {
            if Some(index) == excluded {
                continue;
            }
            let module_path = infer_logical_module_path(unit, &assembly.roots);
            if let Some(path) = module_path.as_ref() {
                resolver.collect_program_in_module(&unit.program, path, Some(&unit.path));
            }
        }
        resolver.errors.clear();
        resolver.module_imports.clear();
        // Native SDK callbacks are admitted only in the exact canonical compiler-SDK unit.
        resolver.sdk_source_authority = source_path.as_ref().and_then(|path| {
            assembly
                .units
                .iter()
                .find(|unit| &unit.path == path)
                .and_then(crate::resolve::sdk_authority::canonical_sdk_source_authority)
        });
        resolver.current_source_path = source_path;
    }

    /// Full-project resolution for `api.json`: resolve entry plus every assembled unit, merging tables.
    pub fn resolve_for_api_documentation(
        &self,
        entry_program: &Spanned<Program>,
        assembly: &ProgramAssembly,
    ) -> Option<Resolution> {
        let entry_source_path = assembly.entry_unit().path.clone();
        let entry_module_path =
            infer_logical_module_path(assembly.entry_unit(), &assembly.roots);

        let mut resolver = Resolver::new();
        resolver.set_current_source_path(Some(entry_source_path.clone()));
        self.seed_resolver_from_assembly(&mut resolver, assembly, Some(assembly.entry_index));
        let mut resolution =
            resolver.resolve_collected_program_for_api_documentation(entry_program, entry_module_path.as_deref());

        self.merge_dependency_declaration_tables(&mut resolution, assembly, None);
        Some(resolution)
    }

    /// Resolve every unit's declaration annotations once, under its own logical module and import
    /// scope, against a resolver that collected every unit of the assembly.
    ///
    /// Dependency declarations are collected while seeding an entry resolver, but their type
    /// annotations can depend on imports declared by the dependency itself; resolving each unit
    /// in its own scope preserves those annotations for declaration surfaces without duplicating
    /// import expansion in the type checker. The tables do not depend on which unit is the entry,
    /// so every own-unit analysis of one assembly shares them.
    pub fn assembly_declaration_tables(&self, assembly: &ProgramAssembly) -> AssemblyDeclarationTables {
        let mut resolver = Resolver::new();
        self.seed_resolver_from_assembly(&mut resolver, assembly, None);
        let tables = assembly
            .units
            .iter()
            .map(|unit| {
                let module_path = infer_logical_module_path(unit, &assembly.roots);
                resolver.set_current_source_path(Some(unit.path.clone()));
                resolver.resolve_collected_program_declarations(&unit.program, module_path.as_deref())
            })
            .collect();
        AssemblyDeclarationTables { units: Arc::clone(&assembly.units), tables }
    }

    /// Merge the declaration tables of every dependency (every unit but the entry) into an entry
    /// resolution.
    fn merge_dependency_declaration_tables(
        &self,
        resolution: &mut Resolution,
        assembly: &ProgramAssembly,
        declarations: Option<&AssemblyDeclarationTables>,
    ) {
        let resolved;
        let declarations = match declarations.filter(|declarations| declarations.describes(assembly)) {
            Some(declarations) => declarations,
            None => {
                resolved = self.assembly_declaration_tables(assembly);
                &resolved
            }
        };
        for (index, (unit, unit_tables)) in assembly.units.iter().zip(&declarations.tables).enumerate() {
            if index == assembly.entry_index {
                continue;
            }
            resolution.tables.merge_declaration_types_from(unit_tables, unit.path.clone());
        }
        resolution.rebuild_span_index();
    }

    fn resolve_longest_module_prefix(&self, path: &[String]) -> Option<&Path> {
        (1..=path.len()).rev().find_map(|length| self.resolve_module(&path[..length]))
    }
}
