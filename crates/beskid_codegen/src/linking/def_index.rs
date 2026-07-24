//! Map resolved [`ItemId`] to HIR definitions using assembly unit HIR and item spans.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use beskid_analysis::hir::{AstProgram, HirFunctionDefinition, HirItem, HirMethodDefinition, HirProgram};
use beskid_analysis::paths::{same_file, unit_path_key};
use beskid_analysis::projects::assembly::UnitHir;
use beskid_analysis::resolve::{ItemId, ItemInfo, ItemKind, Resolution, SymbolId};
use beskid_analysis::syntax::{SpanInfo, Spanned};

#[derive(Clone)]
struct DefLoc {
    program: usize,
    span: SpanInfo,
    short_name: String,
}

/// Index of lowerable function/method bodies keyed by [`ItemId`].
pub struct FunctionDefIndex<'a> {
    hir_units: &'a [UnitHir],
    assembly_unit_count: usize,
    programs: Vec<Spanned<HirProgram>>,
    functions: HashMap<ItemId, DefLoc>,
    methods: HashMap<ItemId, DefLoc>,
    source_paths: HashMap<ItemId, PathBuf>,
    by_symbol: HashMap<SymbolId, ItemId>,
}

impl<'a> FunctionDefIndex<'a> {
    pub fn build(resolution: &Resolution, hir_units: &'a [UnitHir]) -> Self {
        let mut by_path: HashMap<PathBuf, &UnitHir> = HashMap::new();
        for unit in hir_units {
            let key = unit_path_key(&unit.path);
            by_path.insert(key, unit);
        }

        let mut programs = Vec::new();
        let mut program_keys = HashMap::new();
        let mut functions = HashMap::new();
        let mut methods = HashMap::new();
        let mut source_paths = HashMap::new();
        let by_symbol = resolution.by_symbol.clone();

        for info in &resolution.items {
            let Some(source_path) = info.source_path.as_ref() else {
                continue;
            };
            let short_name = item_short_name(&info.name).to_string();
            let Some(program_index) =
                program_index_for_source(&mut programs, &mut program_keys, hir_units, &by_path, source_path)
            else {
                continue;
            };
            let program = if program_index < hir_units.len() {
                &hir_units[program_index].hir
            } else {
                &programs[program_index - hir_units.len()]
            };
            let source_key = unit_path_key(source_path);
            source_paths.insert(info.id, source_key);
            let loc = DefLoc { program: program_index, span: info.span, short_name: short_name.clone() };
            match info.kind {
                ItemKind::Function => {
                    if find_function_in_unit(program, info.span, &short_name).is_some() {
                        functions.insert(info.id, loc);
                    } else if find_method_in_unit(program, info.span, &short_name).is_some() {
                        methods.insert(info.id, loc);
                    }
                }
                ItemKind::Method => {
                    if find_method_in_unit(program, info.span, &short_name).is_some() {
                        methods.insert(info.id, loc);
                    } else if find_function_in_unit(program, info.span, &short_name).is_some() {
                        functions.insert(info.id, loc);
                    }
                }
                _ => {}
            }
        }

        Self { hir_units, assembly_unit_count: hir_units.len(), programs, functions, methods, source_paths, by_symbol }
    }

    fn program_at(&self, index: usize) -> Option<&Spanned<HirProgram>> {
        if index < self.assembly_unit_count {
            self.hir_units.get(index).map(|unit| &unit.hir)
        } else {
            self.programs.get(index - self.assembly_unit_count)
        }
    }

    pub fn item_for_symbol(&self, symbol: SymbolId) -> Option<ItemId> {
        self.by_symbol.get(&symbol).copied()
    }

    pub fn functions(&self) -> HashMap<ItemId, &Spanned<HirFunctionDefinition>> {
        self.functions.keys().filter_map(|item| Some((*item, self.function(*item)?))).collect()
    }

    pub fn function(&self, item: ItemId) -> Option<&Spanned<HirFunctionDefinition>> {
        let loc = self.functions.get(&item)?;
        let program = self.program_at(loc.program)?;
        find_function_in_unit(program, loc.span, &loc.short_name)
    }

    pub fn method(&self, item: ItemId) -> Option<&Spanned<HirMethodDefinition>> {
        let loc = self.methods.get(&item)?;
        let program = self.program_at(loc.program)?;
        find_method_in_unit(program, loc.span, &loc.short_name)
    }

    pub fn source_path(&self, item: ItemId) -> Option<&PathBuf> {
        self.source_paths.get(&item)
    }

    pub fn by_symbol(&self) -> &HashMap<SymbolId, ItemId> {
        &self.by_symbol
    }
}

/// Load a unit HIR program from disk for a resolved item.
pub fn load_hir_program_for_item(resolution: &Resolution, item: ItemId) -> Option<Spanned<HirProgram>> {
    let info = resolution.items.get(item.0)?;
    load_hir_program(info)
}

fn load_hir_program(info: &ItemInfo) -> Option<Spanned<HirProgram>> {
    let path = info.source_path.as_ref()?;
    load_hir_program_from_path(path)
}

fn program_index_for_source(
    programs: &mut Vec<Spanned<HirProgram>>,
    program_keys: &mut HashMap<PathBuf, usize>,
    hir_units: &[UnitHir],
    _by_path: &HashMap<PathBuf, &UnitHir>,
    source_path: &PathBuf,
) -> Option<usize> {
    let key = unit_path_key(source_path);
    if let Some(&index) = program_keys.get(&key) {
        return Some(index);
    }
    if let Some(index) = hir_units.iter().position(|unit| same_file(&unit.path, source_path)) {
        program_keys.insert(key, index);
        return Some(index);
    }
    let hir = load_hir_program_from_path(source_path)?;
    let index = hir_units.len() + programs.len();
    programs.push(hir);
    program_keys.insert(key, index);
    Some(index)
}

fn load_hir_program_from_path(path: &PathBuf) -> Option<Spanned<HirProgram>> {
    let source = std::fs::read_to_string(path).ok()?;
    let logical_name = path.display().to_string();
    let program = beskid_analysis::services::parse_program_with_source_name(&logical_name, &source).ok()?;
    let ast: Spanned<AstProgram> = program.into();
    Some(beskid_analysis::hir::lower_program(&ast))
}

fn item_short_name(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}

fn find_function_in_unit<'a>(
    program: &'a Spanned<HirProgram>,
    span: SpanInfo,
    short_name: &str,
) -> Option<&'a Spanned<HirFunctionDefinition>> {
    find_function_by_span(program, span).or_else(|| find_function_by_name(program, short_name))
}

fn find_method_in_unit<'a>(
    program: &'a Spanned<HirProgram>,
    span: SpanInfo,
    short_name: &str,
) -> Option<&'a Spanned<HirMethodDefinition>> {
    find_method_by_span(program, span).or_else(|| find_method_by_name(program, short_name))
}

pub(crate) fn find_function_by_span(
    program: &Spanned<HirProgram>,
    span: SpanInfo,
) -> Option<&Spanned<HirFunctionDefinition>> {
    find_function_in_items(&program.node.items, span)
}

pub(crate) fn find_function_by_name<'a>(
    program: &'a Spanned<HirProgram>,
    name: &str,
) -> Option<&'a Spanned<HirFunctionDefinition>> {
    find_function_by_name_in_items(&program.node.items, name, &mut HashSet::new())
}

pub(crate) fn find_method_by_span(
    program: &Spanned<HirProgram>,
    span: SpanInfo,
) -> Option<&Spanned<HirMethodDefinition>> {
    find_method_in_items(&program.node.items, span)
}

pub(crate) fn find_method_by_name<'a>(
    program: &'a Spanned<HirProgram>,
    name: &str,
) -> Option<&'a Spanned<HirMethodDefinition>> {
    find_method_by_name_in_items(&program.node.items, name, &mut HashSet::new())
}

fn find_function_in_items(items: &[Spanned<HirItem>], span: SpanInfo) -> Option<&Spanned<HirFunctionDefinition>> {
    find_function_in_items_inner(items, span, &mut HashSet::new())
}

fn find_function_by_name_in_items<'a>(
    items: &'a [Spanned<HirItem>],
    name: &str,
    modules: &mut HashSet<usize>,
) -> Option<&'a Spanned<HirFunctionDefinition>> {
    let mut match_def: Option<&'a Spanned<HirFunctionDefinition>> = None;
    for item in items {
        if let HirItem::FunctionDefinition(def) = &item.node
            && def.node.name.node.name == name
        {
            if match_def.is_some() {
                return None;
            }
            match_def = Some(def);
        }
        if let HirItem::InlineModule(module) = &item.node {
            let ptr = module.node.items.as_ptr() as usize;
            if modules.insert(ptr) {
                let nested = find_function_by_name_in_items(&module.node.items, name, modules);
                if let Some(def) = nested {
                    if match_def.is_some() {
                        return None;
                    }
                    match_def = Some(def);
                }
            }
        }
    }
    match_def
}

fn find_function_in_items_inner<'a>(
    items: &'a [Spanned<HirItem>],
    span: SpanInfo,
    modules: &mut HashSet<usize>,
) -> Option<&'a Spanned<HirFunctionDefinition>> {
    for item in items {
        if spans_match(item.span, span)
            && let HirItem::FunctionDefinition(def) = &item.node
        {
            return Some(def);
        }
        if let HirItem::InlineModule(module) = &item.node {
            let ptr = module.node.items.as_ptr() as usize;
            if modules.insert(ptr)
                && let Some(def) = find_function_in_items_inner(&module.node.items, span, modules)
            {
                return Some(def);
            }
        }
    }
    None
}

fn find_method_in_items(items: &[Spanned<HirItem>], span: SpanInfo) -> Option<&Spanned<HirMethodDefinition>> {
    find_method_in_items_inner(items, span, &mut HashSet::new())
}

fn spans_match(stored: SpanInfo, target: SpanInfo) -> bool {
    stored == target || stored.start == target.start
}

fn find_method_by_name_in_items<'a>(
    items: &'a [Spanned<HirItem>],
    name: &str,
    modules: &mut HashSet<usize>,
) -> Option<&'a Spanned<HirMethodDefinition>> {
    let mut match_def: Option<&'a Spanned<HirMethodDefinition>> = None;
    for item in items {
        if let HirItem::ExtendTypeDefinition(def) = &item.node {
            for method in &def.node.methods {
                if method.node.name.node.name == name {
                    if match_def.is_some() {
                        return None;
                    }
                    match_def = Some(method);
                }
            }
        }
        if let HirItem::TypeDefinition(def) = &item.node {
            for method in &def.node.methods {
                if method.node.name.node.name == name {
                    if match_def.is_some() {
                        return None;
                    }
                    match_def = Some(method);
                }
            }
        }
        if let HirItem::MethodDefinition(def) = &item.node
            && def.node.name.node.name == name
        {
            if match_def.is_some() {
                return None;
            }
            match_def = Some(def);
        }
        if let HirItem::InlineModule(module) = &item.node {
            let ptr = module.node.items.as_ptr() as usize;
            if modules.insert(ptr) {
                let nested = find_method_by_name_in_items(&module.node.items, name, modules);
                if let Some(def) = nested {
                    if match_def.is_some() {
                        return None;
                    }
                    match_def = Some(def);
                }
            }
        }
    }
    match_def
}

fn find_method_in_items_inner<'a>(
    items: &'a [Spanned<HirItem>],
    span: SpanInfo,
    modules: &mut HashSet<usize>,
) -> Option<&'a Spanned<HirMethodDefinition>> {
    for item in items {
        if let HirItem::ExtendTypeDefinition(def) = &item.node {
            for method in &def.node.methods {
                if spans_match(method.span, span) {
                    return Some(method);
                }
            }
        }
        if let HirItem::TypeDefinition(def) = &item.node {
            for method in &def.node.methods {
                if spans_match(method.span, span) {
                    return Some(method);
                }
            }
        }
        if spans_match(item.span, span) {
            if let HirItem::MethodDefinition(def) = &item.node {
                return Some(def);
            }
            return None;
        }
        if let HirItem::InlineModule(module) = &item.node {
            let ptr = module.node.items.as_ptr() as usize;
            if modules.insert(ptr)
                && let Some(def) = find_method_in_items_inner(&module.node.items, span, modules)
            {
                return Some(def);
            }
        }
    }
    None
}
