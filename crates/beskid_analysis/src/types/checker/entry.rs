//! Entry-point orchestration: dependency seeding, body typing, and [`TypeResult`] assembly.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use beskid_pipeline::report_progress;

use crate::projects::assembly::{ModuleIndex, ProgramAssembly};
use crate::resolve::Resolution;
use crate::syntax::Spanned;
use crate::syntax::{Node, Program};
use crate::types::lowering_prep::{LoweringPrep, LoweringPrepSurfaces};
use crate::types::result::{TypeError, TypeResult};
use crate::types::surface::{build_unit_type_surface, merge_unit_surfaces_with_types};

use super::TypeChecker;

impl TypeChecker<'_> {
    /// Type-check entry program with optional dependency units and assemble [`TypeResult`].
    pub fn check_entry(
        program: &mut Spanned<Program>,
        resolution: &Resolution,
        dependency_programs: &[&Spanned<Program>],
        dependency_source_paths: Option<&[PathBuf]>,
        entry_source_path: Option<PathBuf>,
        type_dependency_bodies: bool,
        module_index: Option<&ModuleIndex>,
        assembly: Option<&ProgramAssembly>,
        prefetched_surfaces: Option<&HashMap<PathBuf, Arc<crate::types::surface::UnitTypeSurface>>>,
        progress: Option<(&dyn beskid_pipeline::PipelineObserver, &'static str)>,
    ) -> (TypeResult, Vec<TypeError>) {
        let _types_guard = tracing::info_span!(
            target: "beskid.analysis",
            "beskid.analysis.types",
            entry = tracing::field::display(
                entry_source_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string())
            ),
            session_fingerprint = tracing::field::display("<none>"),
            syntax_generation_id = 0,
        )
        .entered();

        let mut unit_surfaces: HashMap<PathBuf, Arc<crate::types::surface::UnitTypeSurface>> = HashMap::new();
        for (index, dependency) in dependency_programs.iter().enumerate() {
            if let Some(paths) = dependency_source_paths
                && let Some(path) = paths.get(index)
            {
                let key = crate::paths::unit_path_key(path);
                let surface = build_unit_type_surface(dependency, resolution, path);
                unit_surfaces.insert(key, Arc::new(surface));
            }
        }

        let entry_surface = entry_source_path
            .as_ref()
            .map(|path| build_unit_type_surface(program, resolution, path))
            .unwrap_or_default();
        if let Some(entry_path) = entry_source_path.as_ref() {
            let key = crate::paths::unit_path_key(entry_path);
            unit_surfaces.insert(key, Arc::new(entry_surface.clone()));
        }

        if let Some(index) = module_index {
            for path in index.prefetched_paths() {
                let key = crate::paths::unit_path_key(path);
                if unit_surfaces.contains_key(&key) {
                    continue;
                }
                let surface = assembly
                    .and_then(|assembly| {
                        assembly
                            .units
                            .iter()
                            .find(|unit| crate::paths::same_file(&unit.path, path))
                            .map(|unit| Arc::new(build_unit_type_surface(&unit.program, resolution, path)))
                    })
                    .or_else(|| {
                        prefetched_surfaces
                            .and_then(|surfaces| surfaces.get(&key).or_else(|| surfaces.get(path)).cloned())
                    });
                if let Some(surface) = surface {
                    unit_surfaces.insert(key, surface);
                }
            }
        }

        let (merged_types, merged) = merge_unit_surfaces_with_types(
            unit_surfaces
                .iter()
                .filter(|(path, _)| {
                    entry_source_path.as_ref().map(|entry| **path != crate::paths::unit_path_key(entry)).unwrap_or(true)
                })
                .map(|(path, surface)| (path.clone(), surface.clone())),
            Arc::new(entry_surface.clone()),
        );

        let mut checker = TypeChecker::from_merged(resolution, &merged, merged_types);

        let dependency_errors_before = checker.errors.len();
        for (index, dependency) in dependency_programs.iter().enumerate() {
            checker.current_source_path = dependency_source_paths
                .and_then(|paths| paths.get(index))
                .map(|path| crate::paths::unit_path_key(path));
            checker.seed_enum_definitions(dependency);
            checker.seed_struct_definitions(dependency);
            checker.seed_generics_from_program(dependency);
            let errors_before = checker.errors.len();
            checker.seed_contract_signatures(dependency);
            checker.errors.truncate(errors_before);
            checker.register_foreign_function_signatures(dependency);
            checker.errors.truncate(errors_before);
            checker.seed_method_receivers_from_items(&dependency.node.items);
        }

        if type_dependency_bodies {
            for (index, dependency) in dependency_programs.iter().enumerate() {
                checker.current_source_path = dependency_source_paths
                    .and_then(|paths| paths.get(index))
                    .map(|path| crate::paths::unit_path_key(path));
                checker.type_dependency_function_items(&dependency.node.items);
            }
        }

        checker.errors.truncate(dependency_errors_before);
        checker.current_source_path = entry_source_path.as_ref().map(|path| crate::paths::unit_path_key(path));
        checker.seed_struct_definitions(program);
        checker.seed_generics_from_program(program);
        checker.seed_contract_signatures(program);
        checker.seed_method_receivers_from_items(&program.node.items);

        let items = &program.node.items;
        let item_total = items.len() as u64;
        for (index, item) in items.iter().enumerate() {
            if let Some((observer, phase)) = progress {
                let label = checker.progress_label_with_path(syntax_item_progress_label(item));
                report_progress(Some(observer), phase, index as u64 + 1, item_total.max(1), label);
            }
            checker.type_item(item);
        }

        let checker_call_kinds = std::mem::take(&mut checker.call_kinds);
        let checker_result = checker.finish();
        let lowering_surfaces = LoweringPrepSurfaces {
            types: &checker_result.types,
            local_types: &checker_result.local_types,
            function_signatures: &checker_result.function_signatures,
            method_function_signatures: &checker_result.method_function_signatures,
            struct_fields_ordered: &checker_result.struct_fields_ordered,
            struct_event_fields: &checker_result.struct_event_fields,
            enum_variants_ordered: &checker_result.enum_variants_ordered,
            generic_items: &checker_result.generic_items,
            methods_by_receiver: &checker_result.methods_by_receiver,
            contract_signatures: &checker_result.contract_signatures,
            named_types: &checker_result.named_types,
        };
        let mut lowering = LoweringPrep::default();
        if type_dependency_bodies {
            for dependency in dependency_programs {
                merge_lowering_prep(
                    &mut lowering,
                    LoweringPrep::run(dependency, resolution, &checker_result.node_types, &lowering_surfaces),
                );
            }
        }
        merge_lowering_prep(
            &mut lowering,
            LoweringPrep::run(program, resolution, &checker_result.node_types, &lowering_surfaces),
        );
        // Type-checked call kinds win over lowering-prep rediscovery (same spans, authoritative arity).
        lowering.call_kinds.extend(checker_call_kinds);

        let result = TypeResult {
            types: checker_result.types,
            named_type_names: resolution.items.iter().map(|item| (item.id, item.name.clone())).collect(),
            node_types: checker_result.node_types,
            local_types: checker_result.local_types,
            unit_surfaces,
            function_signatures: checker_result.function_signatures,
            method_function_signatures: checker_result.method_function_signatures,
            struct_fields_ordered: checker_result.struct_fields_ordered,
            struct_event_fields: checker_result.struct_event_fields,
            enum_variants_ordered: checker_result.enum_variants_ordered,
            generic_items: checker_result.generic_items,
            lowering,
        };
        (result, checker_result.errors)
    }
}

fn merge_lowering_prep(target: &mut LoweringPrep, from: LoweringPrep) {
    target.call_kinds.extend(from.call_kinds);
    target.cast_intents.extend(from.cast_intents);
}

impl<'a> TypeChecker<'a> {
    pub(super) fn seed_generics_from_program(&mut self, program: &Spanned<Program>) {
        self.seed_generics_from_items(&program.node.items);
    }

    fn seed_generics_from_items(&mut self, items: &[Spanned<Node>]) {
        for item in items {
            let (span, generics) = match &item.node {
                Node::Function(def) => (item.span, &def.node.generics),
                Node::TypeDefinition(def) => (item.span, &def.node.generics),
                Node::EnumDefinition(def) => (item.span, &def.node.generics),
                Node::InlineModule(m) => {
                    self.seed_generics_from_items(&m.node.items);
                    continue;
                }
                _ => continue,
            };
            if let Some(item_id) = self.item_id_for_span(span) {
                let names = generics.iter().map(|generic| generic.node.name.clone()).collect::<Vec<_>>();
                self.generic_items.insert(item_id, names);
            }
        }
    }

    pub(super) fn seed_method_receivers_from_items(&mut self, items: &[Spanned<Node>]) {
        for item in items {
            match &item.node {
                Node::Method(def) => {
                    self.seed_method_receiver(item.span, def);
                }
                Node::ExtendTypeDefinition(def) => {
                    for method in &def.node.methods {
                        self.seed_method_receiver(method.span, method);
                    }
                }
                Node::TypeDefinition(def) => {
                    for method in &def.node.methods {
                        self.seed_method_receiver(method.span, method);
                    }
                }
                Node::InlineModule(m) => {
                    self.seed_method_receivers_from_items(&m.node.items);
                }
                _ => {}
            }
        }
    }

    fn progress_label_with_path(&self, item_label: String) -> String {
        if let Some(path) = &self.current_source_path
            && let Some(file) = path.file_name()
        {
            return format!("{item_label} ({})", file.to_string_lossy());
        }
        item_label
    }
}

fn syntax_item_progress_label(item: &Spanned<Node>) -> String {
    match &item.node {
        Node::Function(def) => format!("fn {}", def.node.name.node.name),
        Node::TypeDefinition(def) => format!("type {}", def.node.name.node.name),
        Node::EnumDefinition(def) => format!("enum {}", def.node.name.node.name),
        Node::Method(def) => format!("method {}", def.node.name.node.name),
        Node::TestDefinition(def) => format!("test {}", def.node.name.node.name),
        Node::ContractDefinition(def) => format!("contract {}", def.node.name.node.name),
        Node::ExtendTypeDefinition(def) => {
            if let Some(method) = def.node.methods.first() {
                format!("extend {}", method.node.name.node.name)
            } else {
                "extend type".into()
            }
        }
        _ => "item".into(),
    }
}
