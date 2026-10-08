use crate::syntax::Program;
use crate::syntax::Spanned;

use super::super::errors::ResolveResult;
use super::super::resolver::{self, Resolution, Resolver};
use super::super::span_index::span_index_from_tables;
use super::super::tables::ResolutionTables;

impl Resolver {
    pub fn resolve_program(&mut self, program: &Spanned<Program>) -> ResolveResult<Resolution> {
        self.resolve_entry_program_in_module(program, None)
    }

    /// Resolve an entry unit whose source also has a logical module path (`Console.bd` is module
    /// `Console`). The entry is collected at the root, as `resolve_program` does, and its
    /// top-level items are additionally bound in that module's scope. A sibling unit naming
    /// `Console.ConsoleSize` then reaches the entry's own declaration, exactly as it does when
    /// the entry is collected as a dependency (`collect_program_in_module`).
    pub fn resolve_entry_program_in_module(
        &mut self,
        program: &Spanned<Program>,
        logical_module_path: Option<&[String]>,
    ) -> ResolveResult<Resolution> {
        self.tables = ResolutionTables::new();
        self.local_scopes.clear();
        self.generic_scopes.clear();
        if self.builtin_items.is_empty() {
            self.collect_builtins();
        }
        let first_entry_item = self.items.len();
        self.collect_program(program);
        if let Some(module_path) = logical_module_path {
            self.bind_root_items_in_module(first_entry_item, module_path);
        }
        self.resolve_collected_program(program)
    }

    /// Bind root-scope items collected at or after `first_item` (the entry's own top-level
    /// declarations) in `module_path`'s scope. An existing binding there is never replaced.
    fn bind_root_items_in_module(&mut self, first_item: usize, module_path: &[String]) {
        if module_path.is_empty() {
            return;
        }
        let root = self.module_graph.root();
        let Some(root_scope) = self.module_graph.module(root).map(|module| module.scope.clone()) else {
            return;
        };
        let mut entry_items = root_scope
            .into_iter()
            .filter(|(_, item)| {
                item.0 >= first_item && self.items.get(item.0).is_some_and(|info| info.parent_id.is_none())
            })
            .collect::<Vec<_>>();
        entry_items.sort_by_key(|(_, item)| item.0);
        let module = self.module_graph.ensure_module_path(module_path);
        for (name, item) in entry_items {
            self.module_graph.insert_item(module, name, item);
        }
    }

    pub fn resolve_collected_program(&mut self, program: &Spanned<Program>) -> ResolveResult<Resolution> {
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

    /// Resolve a pre-collected program under its own import scope while retaining partial facts.
    pub(crate) fn resolve_collected_program_tolerating_errors(
        &mut self,
        program: &Spanned<Program>,
        logical_module_path: Option<&[String]>,
    ) -> Resolution {
        self.prepare_collected_program(program, logical_module_path);
        let file_scoped_module_index = resolver::file_scoped_module_index(program);
        for (index, item) in program.node.items.iter().enumerate() {
            if Some(index) == file_scoped_module_index {
                continue;
            }
            self.resolve_item(item);
        }
        self.take_resolution()
    }

    pub fn resolve_collected_program_for_api_documentation(
        &mut self,
        program: &Spanned<Program>,
        logical_module_path: Option<&[String]>,
    ) -> Resolution {
        self.resolve_collected_program_tolerating_errors(program, logical_module_path)
    }

    /// Resolve only declaration annotations for one pre-collected unit.
    pub(crate) fn resolve_collected_program_declarations(
        &mut self,
        program: &Spanned<Program>,
        logical_module_path: Option<&[String]>,
    ) -> ResolutionTables {
        self.tables = ResolutionTables::new();
        self.local_scopes.clear();
        self.generic_scopes.clear();
        self.errors.clear();
        self.warnings.clear();
        self.current_receiver_item_id = None;
        self.prepare_collected_program(program, logical_module_path);

        let file_scoped_module_index = resolver::file_scoped_module_index(program);
        for (index, item) in program.node.items.iter().enumerate() {
            if Some(index) == file_scoped_module_index {
                continue;
            }
            self.resolve_item_declaration(item);
        }
        self.errors.clear();
        std::mem::take(&mut self.tables)
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
}
