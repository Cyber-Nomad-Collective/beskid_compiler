//! Map resolved [`ItemId`] to HIR definitions using assembly unit HIR and item spans.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use beskid_analysis::paths::same_file;
use beskid_analysis::hir::{
    HirFunctionDefinition, HirItem, HirMethodDefinition, HirProgram,
};
use beskid_analysis::projects::assembly::UnitHir;
use beskid_analysis::resolve::{ItemId, ItemInfo, ItemKind, Resolution};
use beskid_analysis::syntax::{SpanInfo, Spanned};

/// Index of lowerable function/method bodies keyed by [`ItemId`].
pub struct FunctionDefIndex<'a> {
    functions: HashMap<ItemId, &'a Spanned<HirFunctionDefinition>>,
    methods: HashMap<ItemId, &'a Spanned<HirMethodDefinition>>,
    source_paths: HashMap<ItemId, PathBuf>,
}

impl<'a> FunctionDefIndex<'a> {
    pub fn build(resolution: &Resolution, hir_units: &'a [UnitHir]) -> Self {
        let mut by_path: HashMap<PathBuf, &'a UnitHir> = HashMap::new();
        for unit in hir_units {
            let key = unit.path.canonicalize().unwrap_or_else(|_| unit.path.clone());
            by_path.insert(key, unit);
        }

        let mut functions = HashMap::new();
        let mut methods = HashMap::new();
        let mut source_paths = HashMap::new();

        for info in &resolution.items {
            let Some(unit) = unit_for_item(info, &by_path) else {
                continue;
            };
            source_paths.insert(info.id, unit.path.canonicalize().unwrap_or_else(|_| unit.path.clone()));
            match info.kind {
                ItemKind::Function => {
                    if let Some(def) = find_function_by_span(&unit.hir, info.span) {
                        functions.insert(info.id, def);
                    }
                }
                ItemKind::Method => {
                    if let Some(def) = find_method_by_span(&unit.hir, info.span) {
                        methods.insert(info.id, def);
                    }
                }
                _ => {}
            }
        }

        Self {
            functions,
            methods,
            source_paths,
        }
    }

    pub fn functions(&self) -> &HashMap<ItemId, &'a Spanned<HirFunctionDefinition>> {
        &self.functions
    }

    pub fn methods(&self) -> &HashMap<ItemId, &'a Spanned<HirMethodDefinition>> {
        &self.methods
    }

    pub fn function(&self, item: ItemId) -> Option<&'a Spanned<HirFunctionDefinition>> {
        self.functions.get(&item).copied()
    }

    pub fn method(&self, item: ItemId) -> Option<&'a Spanned<HirMethodDefinition>> {
        self.methods.get(&item).copied()
    }

    pub fn source_path(&self, item: ItemId) -> Option<&PathBuf> {
        self.source_paths.get(&item)
    }
}

fn unit_for_item<'a>(
    info: &ItemInfo,
    by_path: &HashMap<PathBuf, &'a UnitHir>,
) -> Option<&'a UnitHir> {
    let source = info.source_path.as_ref()?;
    by_path
        .iter()
        .find(|(unit_path, _)| same_file(unit_path, source))
        .map(|(_, unit)| *unit)
}

fn find_function_by_span<'a>(
    program: &'a Spanned<HirProgram>,
    span: SpanInfo,
) -> Option<&'a Spanned<HirFunctionDefinition>> {
    find_function_in_items(&program.node.items, span)
}

fn find_method_by_span<'a>(
    program: &'a Spanned<HirProgram>,
    span: SpanInfo,
) -> Option<&'a Spanned<HirMethodDefinition>> {
    find_method_in_items(&program.node.items, span)
}

fn find_function_in_items<'a>(
    items: &'a [Spanned<HirItem>],
    span: SpanInfo,
) -> Option<&'a Spanned<HirFunctionDefinition>> {
    find_function_in_items_inner(items, span, &mut HashSet::new())
}

fn find_function_in_items_inner<'a>(
    items: &'a [Spanned<HirItem>],
    span: SpanInfo,
    modules: &mut HashSet<usize>,
) -> Option<&'a Spanned<HirFunctionDefinition>> {
    for item in items {
        if item.span == span {
            if let HirItem::FunctionDefinition(def) = &item.node {
                return Some(def);
            }
            return None;
        }
        if let HirItem::InlineModule(module) = &item.node {
            let ptr = module.node.items.as_ptr() as usize;
            if modules.insert(ptr) {
                if let Some(def) =
                    find_function_in_items_inner(&module.node.items, span, modules)
                {
                    return Some(def);
                }
            }
        }
    }
    None
}

fn find_method_in_items<'a>(
    items: &'a [Spanned<HirItem>],
    span: SpanInfo,
) -> Option<&'a Spanned<HirMethodDefinition>> {
    find_method_in_items_inner(items, span, &mut HashSet::new())
}

fn find_method_in_items_inner<'a>(
    items: &'a [Spanned<HirItem>],
    span: SpanInfo,
    modules: &mut HashSet<usize>,
) -> Option<&'a Spanned<HirMethodDefinition>> {
    for item in items {
        if item.span == span {
            if let HirItem::MethodDefinition(def) = &item.node {
                return Some(def);
            }
            return None;
        }
        if let HirItem::InlineModule(module) = &item.node {
            let ptr = module.node.items.as_ptr() as usize;
            if modules.insert(ptr) {
                if let Some(def) = find_method_in_items_inner(&module.node.items, span, modules) {
                    return Some(def);
                }
            }
        }
    }
    None
}
