//! Reachability-based link plan: which symbols to lower before JIT/AOT linking.

use std::collections::HashSet;
use std::path::PathBuf;

use beskid_analysis::hir::{HirItem, HirMethodDefinition, HirProgram};
use beskid_analysis::paths::unit_path_key;
use beskid_analysis::resolve::{ItemId, ItemInfo, ItemKind, Resolution};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::{TypeId, TypeInfo, TypeResult};

use crate::lowering::function::mangle_method_name;
use crate::lowering::types::type_id_for_type;

use super::call_graph::collect_calls_in_body;
use super::def_index::{
    find_function_by_name, find_function_by_span, find_method_by_name, find_method_by_span,
    FunctionDefIndex,
};

/// One symbol to emit in the link plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkSymbol {
    Function {
        item: ItemId,
        /// `None` uses the item's base name; `Some` is a monomorphized instance name.
        mangled: Option<String>,
        /// Monomorph receiver type for generic owning-type methods (`Send<T>` on `Channel<i64>`).
        receiver_type: Option<TypeId>,
    },
    Method {
        item: ItemId,
        mangled: String,
    },
    Test {
        item: ItemId,
        name: String,
    },
}

/// Callees to lower before entry tests/functions, then entry symbols.
#[derive(Debug, Clone, Default)]
pub struct LinkPlan {
    pub callees: Vec<LinkSymbol>,
    pub entries: Vec<LinkSymbol>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CalleeKey {
    item: ItemId,
    mangled: Option<String>,
}

/// One resolved call edge discovered during HIR walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedCall {
    pub item_id: ItemId,
    pub symbol: Option<beskid_analysis::resolve::SymbolId>,
    pub mangled: Option<String>,
    pub receiver_type: Option<TypeId>,
}

impl LinkPlan {
    pub fn build(
        entry: &Spanned<HirProgram>,
        resolution: &Resolution,
        type_result: &TypeResult,
        def_index: &FunctionDefIndex<'_>,
    ) -> Self {
        let mut entries = Vec::new();
        let mut callees = Vec::new();
        let mut visited: HashSet<CalleeKey> = HashSet::new();
        let mut module_path = Vec::new();

        walk_hir_items(
            &entry.node.items,
            &mut module_path,
            &mut |item, _qualified, _short| {
                let HirItem::TestDefinition(test) = &item.node else {
                    return;
                };
                let Some(info) = item_info_for_span(resolution, item.span, None) else {
                    return;
                };
                entries.push(LinkSymbol::Test {
                    item: info.id,
                    name: test.node.name.node.name.clone(),
                });
                for call in collect_calls_in_body(&test.node.body, resolution, type_result, None) {
                    visit_callee(
                        call,
                        resolution,
                        type_result,
                        def_index,
                        &mut visited,
                        &mut callees,
                    );
                }
            },
        );

        Self { callees, entries }
    }

    /// Reachability plan for a single named entry function or test (run / selective test JIT).
    pub fn build_for_entrypoint(
        entry: &Spanned<HirProgram>,
        entrypoint: &str,
        entry_source_path: Option<&PathBuf>,
        resolution: &Resolution,
        type_result: &TypeResult,
        def_index: &FunctionDefIndex<'_>,
    ) -> Self {
        let mut entries = Vec::new();
        let mut callees = Vec::new();
        let mut visited: HashSet<CalleeKey> = HashSet::new();
        let mut module_path = Vec::new();

        walk_hir_items(
            &entry.node.items,
            &mut module_path,
            &mut |item, qualified, short| {
                if !entrypoint_matches(entrypoint, qualified, short) {
                    return;
                }
                let Some(info) = item_info_for_span(resolution, item.span, entry_source_path)
                else {
                    return;
                };
                match &item.node {
                    HirItem::TestDefinition(test) => {
                        entries.push(LinkSymbol::Test {
                            item: info.id,
                            name: short.to_string(),
                        });
                        for call in collect_calls_in_body(
                            &test.node.body,
                            resolution,
                            type_result,
                            entry_source_path,
                        ) {
                            visit_callee(
                                call,
                                resolution,
                                type_result,
                                def_index,
                                &mut visited,
                                &mut callees,
                            );
                        }
                    }
                    HirItem::FunctionDefinition(def) => {
                        entries.push(LinkSymbol::Function {
                            item: info.id,
                            mangled: None,
                            receiver_type: None,
                        });
                        for call in collect_calls_in_body(
                            &def.node.body,
                            resolution,
                            type_result,
                            entry_source_path,
                        ) {
                            visit_callee(
                                call,
                                resolution,
                                type_result,
                                def_index,
                                &mut visited,
                                &mut callees,
                            );
                        }
                    }
                    _ => {}
                }
            },
        );

        Self { callees, entries }
    }

    /// Function [`ItemId`]s referenced by the link plan (entries + callees), stable order.
    pub fn function_item_ids(&self) -> Vec<ItemId> {
        let mut items: Vec<ItemId> = self
            .callees
            .iter()
            .chain(self.entries.iter())
            .filter_map(|symbol| match symbol {
                LinkSymbol::Function { item, .. } => Some(*item),
                _ => None,
            })
            .collect();
        items.sort_by_key(|item| item.0);
        items.dedup();
        items
    }

    /// All non-generic functions in any source unit touched by the plan (breaks intra-unit mutual recursion).
    pub fn unit_coalesced_function_items(
        &self,
        resolution: &Resolution,
        def_index: &FunctionDefIndex<'_>,
    ) -> Vec<ItemId> {
        let mut unit_paths: HashSet<PathBuf> = HashSet::new();
        for symbol in self.callees.iter().chain(self.entries.iter()) {
            let LinkSymbol::Function { item, .. } = symbol else {
                continue;
            };
            let path = def_index.source_path(*item).or_else(|| {
                resolution
                    .items
                    .get(item.0)
                    .and_then(|info| info.source_path.as_ref())
            });
            let Some(path) = path else {
                continue;
            };
            unit_paths.insert(unit_path_key(path));
        }

        let mut items: Vec<ItemId> = def_index
            .functions()
            .keys()
            .copied()
            .filter(|item| {
                let Some(path) = resolution
                    .items
                    .get(item.0)
                    .and_then(|info| info.source_path.as_ref())
                else {
                    return false;
                };
                unit_paths.contains(&unit_path_key(path))
            })
            .collect();
        items.sort_by_key(|item| item.0);
        items
    }

    pub fn emitted_symbol_names(&self, resolution: &Resolution) -> HashSet<String> {
        let mut names = HashSet::new();
        for symbol in self.callees.iter().chain(self.entries.iter()) {
            match symbol {
                LinkSymbol::Function {
                    item: _,
                    mangled: Some(name),
                    receiver_type: _,
                } => {
                    names.insert(name.clone());
                }
                LinkSymbol::Function {
                    item,
                    mangled: None,
                    receiver_type: _,
                } => {
                    if let Some(info) = resolution.items.get(item.0) {
                        names.insert(info.name.clone());
                    }
                }
                LinkSymbol::Method { mangled, .. } => {
                    names.insert(mangled.clone());
                }
                LinkSymbol::Test { name, .. } => {
                    names.insert(name.clone());
                }
            }
        }
        names
    }
}

fn entrypoint_matches(entrypoint: &str, qualified: &str, short: &str) -> bool {
    entrypoint == short || entrypoint == qualified
}

fn walk_hir_items<'a, F>(
    items: &'a [Spanned<HirItem>],
    module_path: &mut Vec<String>,
    visit: &mut F,
) where
    F: FnMut(&'a Spanned<HirItem>, &str, &str),
{
    for item in items {
        match &item.node {
            HirItem::InlineModule(module) => {
                module_path.push(module.node.name.node.name.clone());
                walk_hir_items(&module.node.items, module_path, visit);
                module_path.pop();
            }
            HirItem::TestDefinition(def) => {
                let short = def.node.name.node.name.as_str();
                let qualified = qualified_item_name(module_path, short);
                visit(item, &qualified, short);
            }
            HirItem::FunctionDefinition(def) => {
                let short = def.node.name.node.name.as_str();
                let qualified = qualified_item_name(module_path, short);
                visit(item, &qualified, short);
            }
            _ => {}
        }
    }
}

fn qualified_item_name(module_path: &[String], short: &str) -> String {
    if module_path.is_empty() {
        short.to_string()
    } else {
        format!("{}::{}", module_path.join("::"), short)
    }
}

fn visit_callee(
    call: ResolvedCall,
    resolution: &Resolution,
    type_result: &TypeResult,
    def_index: &FunctionDefIndex,
    visited: &mut HashSet<CalleeKey>,
    callees: &mut Vec<LinkSymbol>,
) {
    let key = CalleeKey {
        item: call.item_id,
        mangled: call.mangled.clone(),
    };
    if !visited.insert(key) {
        return;
    }
    if resolution.builtin_items.contains_key(&call.item_id) {
        return;
    }

    if let Some(def) = def_index.function(call.item_id) {
        let callee_path = def_index.source_path(call.item_id);
        for inner in collect_calls_in_body(&def.node.body, resolution, type_result, callee_path) {
            visit_callee(inner, resolution, type_result, def_index, visited, callees);
        }
        callees.push(LinkSymbol::Function {
            item: call.item_id,
            mangled: call.mangled,
            receiver_type: call.receiver_type,
        });
        return;
    }

    if let Some(def) = def_index.method(call.item_id) {
        let mangled = call
            .mangled
            .clone()
            .or_else(|| method_mangled_name(resolution, type_result, def))
            .unwrap_or_else(|| {
                resolution
                    .items
                    .get(call.item_id.0)
                    .map(|i| i.name.clone())
                    .unwrap_or_default()
            });
        let callee_path = def_index.source_path(call.item_id);
        for inner in collect_calls_in_body(&def.node.body, resolution, type_result, callee_path) {
            visit_callee(inner, resolution, type_result, def_index, visited, callees);
        }
        callees.push(LinkSymbol::Method {
            item: call.item_id,
            mangled,
        });
        return;
    }

    if let Some(info) = resolution.items.get(call.item_id.0)
        && matches!(info.kind, ItemKind::Function | ItemKind::Method)
    {
        visit_callee_body_from_source(info, resolution, type_result, def_index, visited, callees);
        callees.push(LinkSymbol::Function {
            item: call.item_id,
            mangled: call.mangled,
            receiver_type: call.receiver_type,
        });
    }
}

fn visit_callee_body_from_source(
    info: &ItemInfo,
    resolution: &Resolution,
    type_result: &TypeResult,
    def_index: &FunctionDefIndex,
    visited: &mut HashSet<CalleeKey>,
    callees: &mut Vec<LinkSymbol>,
) {
    let Some(path) = info.source_path.as_ref() else {
        return;
    };
    let Ok(source) = std::fs::read_to_string(path) else {
        return;
    };
    let logical_name = path.display().to_string();
    let Ok(program) = beskid_analysis::services::parse_program_with_source_name(&logical_name, &source)
    else {
        return;
    };
    let ast: beskid_analysis::syntax::Spanned<beskid_analysis::hir::AstProgram> = program.into();
    let hir = beskid_analysis::hir::lower_program(&ast);
    let source_path = unit_path_key(path);
    let short_name = info.name.rsplit("::").next().unwrap_or(&info.name);
    let body = match info.kind {
        ItemKind::Function => find_function_by_span(&hir, info.span)
            .or_else(|| find_function_by_name(&hir, short_name))
            .map(|def| &def.node.body),
        ItemKind::Method => find_method_by_span(&hir, info.span)
            .or_else(|| find_method_by_name(&hir, short_name))
            .map(|def| &def.node.body),
        _ => None,
    };
    let Some(body) = body else {
        return;
    };
    for call in collect_calls_in_body(body, resolution, type_result, Some(&source_path)) {
        visit_callee(
            call,
            resolution,
            type_result,
            def_index,
            visited,
            callees,
        );
    }
}

fn method_mangled_name(
    resolution: &Resolution,
    type_result: &TypeResult,
    def: &Spanned<HirMethodDefinition>,
) -> Option<String> {
    let receiver_type_id = type_id_for_type(resolution, type_result, None, &def.node.receiver_type)?;
    let receiver_item = match type_result.types.get(receiver_type_id) {
        Some(TypeInfo::Named(item_id)) => *item_id,
        Some(TypeInfo::Applied { base, .. }) => *base,
        _ => return None,
    };
    let receiver_name = resolution
        .items
        .iter()
        .find(|info| info.id == receiver_item)
        .map(|info| info.name.as_str())?;
    Some(mangle_method_name(receiver_name, &def.node.name.node.name))
}

fn item_info_for_span<'a>(
    resolution: &'a Resolution,
    span: beskid_analysis::syntax::SpanInfo,
    source_path: Option<&PathBuf>,
) -> Option<&'a ItemInfo> {
    if let Some(path) = source_path {
        if let Some(info) = resolution.items.iter().find(|info| {
            info.span == span
                && info
                    .source_path
                    .as_ref()
                    .is_some_and(|source| beskid_analysis::paths::same_file(source, path))
        }) {
            return Some(info);
        }
    }

    let matches: Vec<_> = resolution
        .items
        .iter()
        .filter(|info| info.span == span)
        .collect();
    match matches.as_slice() {
        [] => None,
        [single] => Some(single),
        _ => None,
    }
}
