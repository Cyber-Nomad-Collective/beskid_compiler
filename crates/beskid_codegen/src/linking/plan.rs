//! Reachability-based link plan: which symbols to lower before JIT/AOT linking.

use std::collections::HashSet;
use std::path::PathBuf;

use beskid_analysis::hir::{HirItem, HirMethodDefinition, HirProgram};
use beskid_analysis::resolve::{ItemId, Resolution};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::{TypeInfo, TypeResult};

use crate::lowering::function::mangle_method_name;
use crate::lowering::types::type_id_for_type;

use super::call_graph::collect_calls_in_body;
use super::def_index::FunctionDefIndex;

/// One symbol to emit in the link plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkSymbol {
    Function {
        item: ItemId,
        /// `None` uses the item's base name; `Some` is a monomorphized instance name.
        mangled: Option<String>,
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
    pub mangled: Option<String>,
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

        walk_hir_items(&entry.node.items, &mut module_path, &mut |item, _qualified, _short| {
            let HirItem::TestDefinition(test) = &item.node else {
                return;
            };
            let Some(info) = resolution.items.iter().find(|i| i.span == item.span) else {
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
        });

        // Stack walk records callers before callees; emit dependencies first.
        callees.reverse();

        Self { callees, entries }
    }

    /// Reachability plan for a single named entry function or test (run / selective test JIT).
    pub fn build_for_entrypoint(
        entry: &Spanned<HirProgram>,
        entrypoint: &str,
        resolution: &Resolution,
        type_result: &TypeResult,
        def_index: &FunctionDefIndex<'_>,
    ) -> Self {
        let mut entries = Vec::new();
        let mut callees = Vec::new();
        let mut visited: HashSet<CalleeKey> = HashSet::new();
        let mut module_path = Vec::new();

        walk_hir_items(&entry.node.items, &mut module_path, &mut |item, qualified, short| {
            if !entrypoint_matches(entrypoint, qualified, short) {
                return;
            }
            let Some(info) = resolution.items.iter().find(|i| i.span == item.span) else {
                return;
            };
            match &item.node {
                HirItem::TestDefinition(test) => {
                    entries.push(LinkSymbol::Test {
                        item: info.id,
                        name: short.to_string(),
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
                }
                HirItem::FunctionDefinition(def) => {
                    entries.push(LinkSymbol::Function {
                        item: info.id,
                        mangled: None,
                    });
                    for call in collect_calls_in_body(&def.node.body, resolution, type_result, None) {
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
        });

        callees.reverse();
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
            let Some(path) = resolution
                .items
                .get(item.0)
                .and_then(|info| info.source_path.as_ref())
            else {
                continue;
            };
            unit_paths.insert(path.canonicalize().unwrap_or_else(|_| path.clone()));
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
                let key = path.canonicalize().unwrap_or_else(|_| path.clone());
                unit_paths.contains(&key)
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
                } => {
                    names.insert(name.clone());
                }
                LinkSymbol::Function {
                    item,
                    mangled: None,
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
)
where
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
    def_index: &FunctionDefIndex<'_>,
    visited: &mut HashSet<CalleeKey>,
    callees: &mut Vec<LinkSymbol>,
) {
    let mut stack = vec![call];
    while let Some(call) = stack.pop() {
        let key = CalleeKey {
            item: call.item_id,
            mangled: call.mangled.clone(),
        };
        if !visited.insert(key) {
            continue;
        }
        if resolution.builtin_items.contains_key(&call.item_id) {
            continue;
        }

        if let Some(def) = def_index.function(call.item_id) {
            let callee_path = def_index.source_path(call.item_id);
            let inners = collect_calls_in_body(&def.node.body, resolution, type_result, callee_path);
            stack.extend(inners);
            callees.push(LinkSymbol::Function {
                item: call.item_id,
                mangled: call.mangled,
            });
            continue;
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
            let inners = collect_calls_in_body(&def.node.body, resolution, type_result, callee_path);
            stack.extend(inners);
            callees.push(LinkSymbol::Method {
                item: call.item_id,
                mangled,
            });
        }
    }
}

fn method_mangled_name(
    resolution: &Resolution,
    type_result: &TypeResult,
    def: &Spanned<HirMethodDefinition>,
) -> Option<String> {
    let receiver_type_id = type_id_for_type(resolution, type_result, &def.node.receiver_type)?;
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
    Some(mangle_method_name(
        receiver_name,
        &def.node.name.node.name,
    ))
}
