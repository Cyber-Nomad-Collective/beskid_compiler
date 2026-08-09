use std::collections::HashMap;

use crate::hir::HirProgram;
use crate::syntax::Spanned;

use super::super::errors::ResolveResult;
use super::super::ids::ItemId;
use super::super::items::ItemInfo;
use super::super::module_graph::ModuleGraph;
use super::super::resolver::{self, Resolution, Resolver};
use super::super::span_index::span_index_from_tables;
use super::super::symbol::{SymbolId, SymbolRegistry};
use super::super::tables::ResolutionTables;

impl Resolver {
    pub fn resolve_program(&mut self, program: &Spanned<HirProgram>) -> ResolveResult<Resolution> {
        self.tables = ResolutionTables::new();
        self.local_scopes.clear();
        self.generic_scopes.clear();
        if self.builtin_items.is_empty() {
            self.collect_builtins();
        }
        self.collect_program(program);
        self.resolve_collected_program(program)
    }

    pub fn resolve_collected_program(&mut self, program: &Spanned<HirProgram>) -> ResolveResult<Resolution> {
        let file_scoped_module_index = resolver::file_scoped_module_index(program);
        self.current_module = resolver::file_scoped_module_path(program)
            .map(|path| self.module_graph.ensure_module_path(&path))
            .unwrap_or(self.module_graph.root());
        for (index, item) in program.node.items.iter().enumerate() {
            if Some(index) == file_scoped_module_index {
                continue;
            }
            self.resolve_item(item);
        }

        if self.errors.is_empty() { Ok(self.take_resolution()) } else { Err(std::mem::take(&mut self.errors)) }
    }

    pub fn resolve_collected_program_for_api_documentation(
        &mut self,
        program: &Spanned<HirProgram>,
        logical_module_path: Option<&[String]>,
    ) -> Resolution {
        let file_scoped_module_index = resolver::file_scoped_module_index(program);
        self.current_module = logical_module_path
            .map(|path| self.module_graph.ensure_module_path(path))
            .or_else(|| {
                resolver::file_scoped_module_path(program).map(|path| self.module_graph.ensure_module_path(&path))
            })
            .unwrap_or(self.module_graph.root());
        for (index, item) in program.node.items.iter().enumerate() {
            if Some(index) == file_scoped_module_index {
                continue;
            }
            self.resolve_item(item);
        }
        self.take_resolution()
    }

    fn take_resolution(&mut self) -> Resolution {
        let tables = std::mem::take(&mut self.tables);
        let span_index = span_index_from_tables(&tables);
        Resolution {
            items: std::mem::take(&mut self.items),
            module_graph: std::mem::take(&mut self.module_graph),
            tables,
            span_index,
            warnings: std::mem::take(&mut self.warnings),
            builtin_items: std::mem::take(&mut self.builtin_items),
            module_imports: std::mem::take(&mut self.module_imports),
            symbols: std::mem::take(&mut self.symbols),
            by_symbol: std::mem::take(&mut self.by_symbol),
        }
    }

    pub(crate) fn into_prefetch_parts(
        self,
    ) -> (Vec<ItemInfo>, ModuleGraph, HashMap<ItemId, usize>, SymbolRegistry, HashMap<SymbolId, ItemId>) {
        (self.items, self.module_graph, self.builtin_items, self.symbols, self.by_symbol)
    }
}
