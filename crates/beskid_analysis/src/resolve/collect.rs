use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use crate::syntax::{self, Spanned};
use crate::syntax::{Node, Type, UseDeclaration, Visibility};

use super::errors::ResolveError;
use super::ids::{ItemId, ModuleId};
use super::items::{ItemInfo, ItemKind};
use super::member_items;
use super::module_graph::ModuleGraph;
use super::resolver::{self, Resolver};
use super::symbol::{BUILTIN_PACKAGE, SymbolId, SymbolQualifier, SymbolShape, symbol_shape_for_item, symbol_to_string};
use crate::builtins::builtin_specs;

pub(super) fn type_name_for_method_receiver(receiver_type: &Spanned<Type>) -> String {
    match &receiver_type.node {
        Type::Primitive(primitive) => format!("{:?}", primitive.node),
        Type::Complex(path) => {
            path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>().join(".")
        }
        Type::Associated { contract, name } => format!(
            "{}::{}",
            contract
                .node
                .segments
                .iter()
                .map(|segment| segment.node.name.node.name.as_str())
                .collect::<Vec<_>>()
                .join("."),
            name.node.name
        ),
        Type::This => "This".to_string(),
        Type::Array(_) => "Array".to_string(),
        Type::Function { .. } => "Function".to_string(),
    }
}

pub(super) fn path_tail(path: &Spanned<crate::syntax::Path>) -> String {
    path.node
        .segments
        .last()
        .map(|segment| segment.node.name.node.name.clone())
        .unwrap_or_else(|| "<unnamed>".to_string())
}

pub(super) fn builtin_span() -> syntax::SpanInfo {
    syntax::SpanInfo { start: 0, end: 0, line_col_start: (1, 1), line_col_end: (1, 1) }
}

pub(crate) fn use_imported_name(use_decl: &UseDeclaration) -> String {
    use_decl.alias.as_ref().map(|alias| alias.node.name.clone()).unwrap_or_else(|| path_tail(&use_decl.path))
}

/// Outcome of [`Resolver::reexported_item_route`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReexportRoute {
    Found(ItemId),
    /// Distinct items of the name at the nearest providing distance.
    Ambiguous(usize),
    Missing,
}

impl Resolver {
    pub(crate) fn set_current_source_path(&mut self, source_path: Option<PathBuf>) {
        self.current_source_path = source_path.map(|path| crate::paths::unit_path_key(&path));
    }

    pub fn with_module_prefetch(
        items: Vec<ItemInfo>,
        module_graph: ModuleGraph,
        builtin_items: std::collections::HashMap<ItemId, usize>,
        symbols: super::symbol::SymbolRegistry,
        by_symbol: std::collections::HashMap<SymbolId, ItemId>,
    ) -> Self {
        Self { items, module_graph, builtin_items, symbols, by_symbol, ..Self::default() }
    }

    fn current_module_path(&self) -> Vec<String> {
        self.module_graph.module(self.current_module).map(|module| module.path.clone()).unwrap_or_default()
    }

    fn try_register_symbol(
        &mut self,
        item_id: ItemId,
        kind: ItemKind,
        name: &str,
        module_path: &[String],
        method_receiver: Option<&str>,
        parent_symbol: Option<SymbolId>,
        member_name: Option<&str>,
        span: syntax::SpanInfo,
    ) -> Option<SymbolId> {
        let shape = symbol_shape_for_item(kind, module_path, name, method_receiver, parent_symbol, member_name)?;
        let qualifier = SymbolQualifier { package: self.declaring_package.clone(), shape };
        if let Some(existing) = self.symbols.lookup(&qualifier) {
            if self.by_symbol.get(&existing) != Some(&item_id) {
                let previous = self
                    .by_symbol
                    .get(&existing)
                    .and_then(|prev| self.items.get(prev.0))
                    .map(|info| info.span)
                    .unwrap_or(span);
                self.errors.push(ResolveError::DuplicateSymbol {
                    symbol: symbol_to_string(&self.symbols, &qualifier),
                    span,
                    previous,
                });
            }
            return Some(existing);
        }
        let symbol_id = self.symbols.intern(qualifier);
        self.by_symbol.insert(symbol_id, item_id);
        Some(symbol_id)
    }

    pub fn collect_program_in_module(
        &mut self,
        program: &Spanned<crate::syntax::Program>,
        module_path: &[String],
        source_path: Option<&PathBuf>,
    ) {
        self.module_imports.clear();
        self.imported_scope_origins.clear();
        self.declaring_scope_origins.clear();
        self.module_import_origins.clear();
        self.current_source_path = source_path.map(|path| crate::paths::unit_path_key(path));
        if resolver::file_scoped_module_index(program).is_some() {
            self.collect_program(program);
            return;
        }
        self.current_module = self.module_graph.ensure_module_path(module_path);
        self.collect_items(&program.node.items, None);
    }

    pub fn collect_program(&mut self, program: &Spanned<crate::syntax::Program>) {
        self.module_imports.clear();
        self.imported_scope_origins.clear();
        self.declaring_scope_origins.clear();
        self.module_import_origins.clear();
        let file_scoped_module_index = resolver::file_scoped_module_index(program);
        self.current_module = resolver::file_scoped_module_path(program)
            .map(|path| self.module_graph.ensure_module_path(&path))
            .unwrap_or(self.module_graph.root());
        self.collect_items(&program.node.items, file_scoped_module_index);
    }

    /// Restore one already-collected unit's module and import scope before resolving its refs.
    pub(super) fn prepare_collected_program(
        &mut self,
        program: &Spanned<crate::syntax::Program>,
        logical_module_path: Option<&[String]>,
    ) {
        self.module_imports.clear();
        self.imported_scope_origins.clear();
        self.declaring_scope_origins.clear();
        self.module_import_origins.clear();
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
            if let Node::UseDeclaration(def) = &item.node {
                self.collect_use_declaration(item, def);
            }
        }
        self.apply_pending_reexport_imports();
        self.add_implicit_core_import();
    }

    /// Collect local declarations before applying imports so declaration identity is independent
    /// of source ordering. Imports intentionally skip occupied names; therefore a local function
    /// such as `Query.ArrayIterator.Current` deterministically shadows an imported `Current`.
    fn collect_items(&mut self, items: &[Spanned<Node>], excluded_index: Option<usize>) {
        for (index, item) in items.iter().enumerate() {
            if Some(index) != excluded_index && !matches!(item.node, Node::UseDeclaration(_)) {
                self.collect_item(item);
            }
        }
        for (index, item) in items.iter().enumerate() {
            if Some(index) != excluded_index && matches!(item.node, Node::UseDeclaration(_)) {
                self.collect_item(item);
            }
        }
        self.apply_pending_reexport_imports();
        self.add_implicit_core_import();
    }

    fn add_implicit_core_import(&mut self) {
        if self.current_module_path().first().is_some_and(|segment| segment == "Core") {
            return;
        }

        let core = vec!["Core".to_owned()];
        if self.module_graph.module_id(&core).is_some() {
            self.module_imports.entry("Core".to_owned()).or_insert(core);
        }
    }

    pub(crate) fn collect_builtins(&mut self) {
        let saved_package = self.declaring_package.clone();
        self.declaring_package = BUILTIN_PACKAGE.to_string();
        for (index, spec) in builtin_specs().iter().enumerate() {
            let module_path: Vec<String> = spec
                .beskid_path
                .iter()
                .take(spec.beskid_path.len().saturating_sub(1))
                .map(|segment| (*segment).to_string())
                .collect();
            let module_id = self.module_graph.ensure_module_path(&module_path);
            let name = spec
                .beskid_path
                .last()
                .map(|segment| (*segment).to_string())
                .unwrap_or_else(|| "<builtin>".to_string());
            let id = ItemId(self.items.len());
            if let Some(prev) = self.module_graph.insert_item(module_id, name.clone(), id) {
                let prev_span = self.items[prev.0].span;
                self.errors.push(ResolveError::DuplicateItem { name, span: builtin_span(), previous: prev_span });
                continue;
            }
            let path: Vec<String> = spec.beskid_path.iter().map(|segment| (*segment).to_string()).collect();
            let qualifier =
                SymbolQualifier { package: BUILTIN_PACKAGE.to_string(), shape: SymbolShape::Builtin { path } };
            let symbol_id = self.symbols.intern(qualifier);
            self.by_symbol.insert(symbol_id, id);
            self.items.push(ItemInfo {
                id,
                parent_id: None,
                name,
                kind: ItemKind::Function,
                visibility: Visibility::Public,
                span: builtin_span(),
                source_path: self.current_source_path.clone(),
                symbol: Some(symbol_id),
            });
            self.builtin_items.insert(id, index);
        }
        self.declaring_package = saved_package;
    }

    fn collect_item(&mut self, item: &Spanned<Node>) {
        let (name, kind, visibility, method_receiver) = match &item.node {
            Node::HostDefinition(_) => {
                return;
            }
            // Preserve the literal fact without inventing a callable item or storage slot.
            Node::ConstantDefinition(definition) => {
                let key = (self.current_module, definition.node.name.node.name.clone());
                if let Some(previous) = self.constants.insert(key, definition.node.value.clone()) {
                    self.errors.push(ResolveError::DuplicateItem {
                        name: definition.node.name.node.name.clone(),
                        span: item.span,
                        previous: previous.span,
                    });
                }
                return;
            }
            Node::Function(def) => {
                (def.node.name.node.name.clone(), ItemKind::Function, def.node.visibility.node, None)
            }
            Node::Method(def) => {
                let receiver = type_name_for_method_receiver(&def.node.receiver_type);
                (
                    format!("{}::{}", receiver, def.node.name.node.name),
                    ItemKind::Method,
                    def.node.visibility.node,
                    Some(receiver),
                )
            }
            Node::ExtendTypeDefinition(def) => {
                for method in &def.node.methods {
                    let receiver = type_name_for_method_receiver(&method.node.receiver_type);
                    let method_name = format!("{}::{}", receiver, method.node.name.node.name);
                    self.push_item(
                        ItemId(self.items.len()),
                        None,
                        method_name,
                        ItemKind::Method,
                        method.node.visibility.node,
                        method.span,
                        Some(receiver),
                        self.current_module_path(),
                    );
                    let method_id = ItemId(self.items.len() - 1);
                    self.collect_member_items_for_method(method, method_id);
                }
                return;
            }
            Node::ImplBlock(def) => {
                for method in &def.node.methods {
                    let receiver = type_name_for_method_receiver(&method.node.receiver_type);
                    let method_name = format!("{}::{}", receiver, method.node.name.node.name);
                    self.push_item(
                        ItemId(self.items.len()),
                        None,
                        method_name,
                        ItemKind::Method,
                        method.node.visibility.node,
                        method.span,
                        Some(receiver),
                        self.current_module_path(),
                    );
                    let method_id = ItemId(self.items.len() - 1);
                    self.collect_member_items_for_method(method, method_id);
                }
                return;
            }
            Node::TestDefinition(def) => {
                (def.node.name.node.name.clone(), ItemKind::Test, def.node.visibility.node, None)
            }
            Node::TypeDefinition(def) => {
                (def.node.name.node.name.clone(), ItemKind::Type, def.node.visibility.node, None)
            }
            Node::EnumDefinition(def) => {
                (def.node.name.node.name.clone(), ItemKind::Enum, def.node.visibility.node, None)
            }
            Node::ContractDefinition(def) => {
                (def.node.name.node.name.clone(), ItemKind::Contract, def.node.visibility.node, None)
            }
            Node::ModuleDeclaration(def) => {
                (path_tail(&def.node.path), ItemKind::Module, def.node.visibility.node, None)
            }
            Node::InlineModule(def) => {
                (def.node.name.node.name.clone(), ItemKind::Module, def.node.visibility.node, None)
            }
            Node::UseDeclaration(def) => {
                self.collect_use_declaration(item, def);
                return;
            }
            Node::AttributeDeclaration(_) => {
                return;
            }
            Node::MacroDefinition(_) => {
                return;
            }
        };

        let id = ItemId(self.items.len());
        let module_id = match &item.node {
            Node::ModuleDeclaration(def) => {
                let segments: Vec<String> =
                    def.node.path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect();
                let parent_path = &segments[..segments.len().saturating_sub(1)];
                self.module_graph.ensure_module_path(parent_path)
            }
            _ => self.current_module,
        };
        if let Some(prev) = self.module_graph.insert_item(module_id, name.clone(), id) {
            let prev_span = self.items[prev.0].span;
            self.errors.push(ResolveError::DuplicateItem { name, span: item.span, previous: prev_span });
            return;
        }
        let push_module_path = match &item.node {
            Node::ModuleDeclaration(def) => {
                let segments: Vec<String> =
                    def.node.path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect();
                let mut path = self.current_module_path();
                path.extend_from_slice(&segments[..segments.len().saturating_sub(1)]);
                path
            }
            _ => self.current_module_path(),
        };
        self.push_item(id, None, name, kind, visibility, item.span, method_receiver, push_module_path);

        self.collect_member_items(item, id);

        if let Node::TypeDefinition(def) = &item.node {
            let type_name = def.node.name.node.name.clone();
            let field_names: std::collections::HashSet<_> =
                def.node.fields.iter().map(|field| field.node.name.node.name.as_str()).collect();
            for method in &def.node.methods {
                let method_name = method.node.name.node.name.as_str();
                if field_names.contains(method_name) {
                    self.errors.push(ResolveError::DuplicateItem {
                        name: format!("{}::{}", type_name, method_name),
                        span: method.span,
                        previous: def
                            .node
                            .fields
                            .iter()
                            .find(|field| field.node.name.node.name == method_name)
                            .map(|field| field.span)
                            .unwrap_or(method.span),
                    });
                    continue;
                }
                let receiver = type_name_for_method_receiver(&method.node.receiver_type);
                let qualified = format!("{}::{}", receiver, method_name);
                self.push_item(
                    ItemId(self.items.len()),
                    None,
                    qualified,
                    ItemKind::Method,
                    method.node.visibility.node,
                    method.span,
                    Some(receiver),
                    self.current_module_path(),
                );
                let method_id = ItemId(self.items.len() - 1);
                self.collect_member_items_for_method(method, method_id);
            }
        }

        if let Node::ModuleDeclaration(def) = &item.node {
            let module_path = def
                .node
                .path
                .node
                .segments
                .iter()
                .map(|segment| segment.node.name.node.name.clone())
                .collect::<Vec<_>>();
            self.module_graph.ensure_module_path(&module_path);
        }
        if let Node::InlineModule(def) = &item.node {
            let previous_module = self.current_module;
            let mut module_path =
                self.module_graph.module(self.current_module).map(|module| module.path.clone()).unwrap_or_default();
            module_path.push(def.node.name.node.name.clone());
            let child_module = self.module_graph.ensure_module_path(&module_path);
            self.current_module = child_module;
            self.collect_items(&def.node.items, None);
            self.current_module = previous_module;
        }
    }

    fn collect_use_declaration(&mut self, item: &Spanned<Node>, def: &Spanned<UseDeclaration>) {
        let alias = use_imported_name(&def.node);
        let module_path = resolver::path_segments(&def.node.path);
        if self.module_imports.contains_key(&alias) {
            self.errors.push(ResolveError::DuplicateItem {
                name: alias.clone(),
                span: item.span,
                previous: def.node.path.span,
            });
            return;
        }
        if self.module_graph.module_id(&module_path).is_none() {
            self.errors
                .push(ResolveError::UnknownModulePath { path: module_path.join("::"), span: def.node.path.span });
            return;
        }
        self.module_imports.insert(alias.clone(), module_path.clone());
        self.module_import_origins.insert(alias, def.node.path.span);
        self.import_public_items_from_module(&module_path, def.node.path.span);
    }

    /// Bring public items from a used module into the current module scope (types, enums, functions).
    ///
    /// A module's scope also holds what that module itself imported, so one `use` can make an
    /// item visible that another module declares. When a later `use` names the declaring module
    /// of the same item, that `use` becomes the item's origin: usage must credit the import that
    /// declares the item, not one that only passes it through.
    ///
    /// A public module declaration (`pub mod Beskid.Syntax.Nodes.NodeSpan;` in the hub
    /// `Beskid.Syntax.Nodes`) re-exports the public items of that child module, so `use` of the
    /// hub reaches them too. Re-export distance decides precedence, as in the semantic contract
    /// (`nearest_reexport_route`): an item of the used module shadows a re-exported one of the
    /// same name, and two different items of one name at the same distance are ambiguous and
    /// neither is imported.
    fn import_public_items_from_module(&mut self, module_path: &[String], origin_span: syntax::SpanInfo) {
        let Some(target_module_id) = self.module_graph.module_id(module_path) else {
            return;
        };
        let Some(target_module) = self.module_graph.module(target_module_id) else {
            return;
        };
        let mut imports: Vec<(String, ItemId, bool)> = target_module
            .scope
            .iter()
            .filter_map(|(name, item_id)| {
                let info = self.items.get(item_id.0)?;
                if info.visibility != Visibility::Public {
                    return None;
                }
                match info.kind {
                    ItemKind::Function | ItemKind::Enum | ItemKind::Type | ItemKind::Contract => {
                        let declared = !self.imported_scope_entries.contains(&(target_module_id, name.clone()));
                        Some((name.clone(), *item_id, declared))
                    }
                    _ => None,
                }
            })
            .collect();
        self.pending_reexport_imports.push((target_module_id, origin_span));
        self.insert_scope_imports(imports, origin_span);
    }

    /// Bind imported public items in the current module scope. A local declaration (or a
    /// different item already bound) shadows an import, which then keeps no origin.
    fn insert_scope_imports(&mut self, mut imports: Vec<(String, ItemId, bool)>, origin_span: syntax::SpanInfo) {
        imports.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.0.cmp(&right.1.0)));
        for (name, item_id, declared) in imports {
            if let Some(previous) = self.module_graph.insert_item(self.current_module, name.clone(), item_id) {
                let same_import = previous == item_id
                    && self.imported_scope_entries.contains(&(self.current_module, name.clone()));
                if !same_import {
                    continue;
                }
                let replace = match self.imported_scope_origins.contains_key(&name) {
                    false => true,
                    true => declared && !self.declaring_scope_origins.contains(&name),
                };
                if replace {
                    if declared {
                        self.declaring_scope_origins.insert(name.clone());
                    }
                    self.imported_scope_origins.insert(name, origin_span);
                }
                continue;
            }
            self.imported_scope_entries.insert((self.current_module, name.clone()));
            if declared {
                self.declaring_scope_origins.insert(name.clone());
            }
            self.imported_scope_origins.insert(name, origin_span);
        }
    }

    /// Bind the public re-exports of every module the current unit used, once all of the unit's
    /// direct imports are bound: an item a used module declares or imports itself always takes
    /// precedence over an item another used module only re-exports, whatever the `use` order.
    fn apply_pending_reexport_imports(&mut self) {
        for (module_id, origin_span) in std::mem::take(&mut self.pending_reexport_imports) {
            let items = self.reexported_public_items(module_id);
            self.insert_scope_imports(items, origin_span);
        }
    }

    /// Public items re-exported by the public child module declarations of `module_id`, nearest
    /// re-export distance first. Names the module's own public surface provides, or that a nearer
    /// distance provides, are shadowed; a name that two different items provide at one
    /// distance is ambiguous and omitted.
    fn reexported_public_items(&self, module_id: ModuleId) -> Vec<(String, ItemId, bool)> {
        let mut out = Vec::new();
        for level in self.reexport_levels(module_id) {
            for (name, items) in level {
                if let [item_id] = items.as_slice() {
                    out.push((name, *item_id, true));
                }
            }
        }
        out
    }

    /// The one rule for reaching an item through a module's public re-exports, shared by scope
    /// imports (`use Hub;`) and qualified paths (`Hub.Item`): the nearest re-export distance that
    /// provides the name wins; two different items at that distance are ambiguous and the lookup
    /// fails closed; the module's own public declarations shadow every re-export.
    pub(crate) fn reexported_item_route(&self, module_id: ModuleId, name: &str) -> ReexportRoute {
        for level in self.reexport_levels(module_id) {
            if let Some(items) = level.get(name) {
                return match items.as_slice() {
                    [item_id] => ReexportRoute::Found(*item_id),
                    items => ReexportRoute::Ambiguous(items.len()),
                };
            }
        }
        ReexportRoute::Missing
    }

    /// Breadth-first levels of the public re-export graph below `module_id`: for each distance,
    /// the public items its `pub mod` children declare, keyed by name, after shadowing by the
    /// module's own public surface and by every nearer distance. Only declared items of a child
    /// count; what a child merely imported is not re-exported.
    fn reexport_levels(&self, module_id: ModuleId) -> Vec<BTreeMap<String, Vec<ItemId>>> {
        let reexportable = |item: &ItemInfo| {
            item.visibility == Visibility::Public
                && matches!(item.kind, ItemKind::Function | ItemKind::Enum | ItemKind::Type | ItemKind::Contract)
        };
        let mut shadowed: HashSet<String> = self
            .module_graph
            .module(module_id)
            .map(|module| {
                module
                    .scope
                    .iter()
                    .filter(|(_, item_id)| self.items.get(item_id.0).is_some_and(reexportable))
                    .map(|(name, _)| name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let mut visited: HashSet<ModuleId> = HashSet::from([module_id]);
        let mut frontier = vec![module_id];
        let mut levels = Vec::new();
        while !frontier.is_empty() {
            let mut children = Vec::new();
            for module in &frontier {
                let Some(info) = self.module_graph.module(*module) else { continue };
                for (name, item_id) in &info.scope {
                    let Some(item) = self.items.get(item_id.0) else { continue };
                    if item.kind != ItemKind::Module || item.visibility != Visibility::Public {
                        continue;
                    }
                    let mut child_path = info.path.clone();
                    child_path.push(name.clone());
                    if let Some(child) = self.module_graph.module_id(&child_path)
                        && visited.insert(child)
                    {
                        children.push(child);
                    }
                }
            }
            let mut level: BTreeMap<String, Vec<ItemId>> = BTreeMap::new();
            for child in &children {
                let Some(info) = self.module_graph.module(*child) else { continue };
                for (name, item_id) in &info.scope {
                    if shadowed.contains(name) || self.imported_scope_entries.contains(&(*child, name.clone())) {
                        continue;
                    }
                    if self.items.get(item_id.0).is_some_and(reexportable) {
                        let candidates = level.entry(name.clone()).or_default();
                        if !candidates.contains(item_id) {
                            candidates.push(*item_id);
                        }
                    }
                }
            }
            shadowed.extend(level.keys().cloned());
            levels.push(level);
            frontier = children;
        }
        levels
    }

    pub(crate) fn mark_imported_scope_name_used(&mut self, name: &str) {
        if let Some(span) = self.imported_scope_origins.get(name) {
            self.tables.used_import_spans.insert(*span);
        }
    }

    pub(crate) fn mark_module_import_alias_used(&mut self, alias: &str) {
        if let Some(span) = self.module_import_origins.get(alias) {
            self.tables.used_import_spans.insert(*span);
        }
    }

    /// Credit every `use M;` that a resolved qualified path reaches through module `M`.
    ///
    /// `use Core.Output;` followed by the fully qualified `Core.Output.WriteLine(...)` reaches the
    /// imported module without spelling its alias first, yet the reference still goes through
    /// that module; the import is used. A path equal to `M` (`Beskid.Compiler.Workspace` after
    /// `use Beskid.Compiler.Workspace;`, in type or value position) names the module's homonymous
    /// item and goes through `M` too.
    pub(crate) fn mark_module_imports_covering(&mut self, segments: &[String]) {
        let covering = self
            .module_imports
            .iter()
            .filter(|(_, module_path)| {
                segments.starts_with(module_path)
                    && (segments.len() > module_path.len() || self.module_declares_homonymous_item(module_path))
            })
            .filter_map(|(alias, _)| self.module_import_origins.get(alias).copied())
            .collect::<Vec<_>>();
        self.tables.used_import_spans.extend(covering);
    }

    /// Whether module `module_path` declares an item named like its own last segment.
    fn module_declares_homonymous_item(&self, module_path: &[String]) -> bool {
        let Some(name) = module_path.last() else {
            return false;
        };
        self.module_graph
            .module_id(module_path)
            .and_then(|module_id| self.module_graph.module(module_id))
            .is_some_and(|module| module.scope.contains_key(name))
    }

    fn push_member_item(
        &mut self,
        name: String,
        kind: ItemKind,
        visibility: Visibility,
        span: syntax::SpanInfo,
        parent_id: ItemId,
    ) {
        let id = ItemId(self.items.len());
        self.push_item(id, Some(parent_id), name, kind, visibility, span, None, self.current_module_path());
    }

    fn push_item(
        &mut self,
        id: ItemId,
        parent_id: Option<ItemId>,
        name: String,
        kind: ItemKind,
        visibility: Visibility,
        span: syntax::SpanInfo,
        method_receiver: Option<String>,
        module_path: Vec<String>,
    ) {
        let parent_symbol = parent_id.and_then(|parent| self.items.get(parent.0).and_then(|info| info.symbol));
        let symbol = self.try_register_symbol(
            id,
            kind,
            &name,
            &module_path,
            method_receiver.as_deref(),
            parent_symbol,
            Some(name.as_str()),
            span,
        );
        self.items.push(ItemInfo {
            id,
            parent_id,
            name,
            kind,
            visibility,
            span,
            source_path: self.current_source_path.clone(),
            symbol,
        });
    }

    fn collect_member_items(&mut self, item: &Spanned<Node>, parent_id: ItemId) {
        let Some(parent) = self.items.get(parent_id.0) else {
            return;
        };
        let parent_name = parent.name.clone();
        let visibility = parent.visibility;
        for spec in member_items::collect_member_items(item, &parent_name) {
            self.push_member_item(spec.name, spec.kind, spec.visibility.unwrap_or(visibility), spec.span, parent_id);
        }
    }

    fn collect_member_items_for_method(
        &mut self,
        method: &Spanned<crate::syntax::MethodDefinition>,
        parent_id: ItemId,
    ) {
        let parent_name = self.items.get(parent_id.0).map(|item| item.name.clone()).unwrap_or_default();
        let visibility = self.items.get(parent_id.0).map(|item| item.visibility).unwrap_or(Visibility::Private);
        for parameter in &method.node.parameters {
            self.push_member_item(
                format!("{}::{}", parent_name, parameter.node.name.node.name),
                ItemKind::Parameter,
                visibility,
                parameter.span,
                parent_id,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::services::parse_program;

    use super::super::resolver::Resolver;

    #[test]
    fn resolves_the_core_namespace_without_an_explicit_use_declaration() {
        let core_output = parse_program("pub unit WriteLine(string text) {};").expect("parse Core.Output");
        let entry = parse_program("unit Main() { Core.Output.WriteLine(\"hello\"); }").expect("parse entry");

        let mut resolver = Resolver::new();
        resolver.collect_program_in_module(
            &core_output,
            &["Core".to_owned(), "Output".to_owned()],
            Some(&PathBuf::from("/toolchain/corelib/Core/Output.bd")),
        );

        let resolution = resolver.resolve_program(&entry).expect("resolve entry with implicit Core namespace");
        assert_eq!(resolution.module_imports.get("Core"), Some(&vec!["Core".to_owned()]));
    }

    #[test]
    fn records_the_exact_use_declaration_that_supplies_a_public_item() {
        let results =
            parse_program("pub enum Result<T, E> { Ok(T value), Error(E error) }").expect("parse Core.Results");
        let errors = parse_program("pub enum NetworkError { Closed() }").expect("parse Network.Errors");
        let entry = parse_program(
            "use Core.Results; use Network.Errors; Result<unit, NetworkError> Main() { return Result::Ok(()); }",
        )
        .expect("parse entry");
        let use_spans = entry
            .node
            .items
            .iter()
            .filter_map(|item| match &item.node {
                crate::syntax::Node::UseDeclaration(use_decl) => Some(use_decl.node.path.span),
                _ => None,
            })
            .collect::<Vec<_>>();

        let mut resolver = Resolver::new();
        resolver.collect_program_in_module(&results, &["Core".to_owned(), "Results".to_owned()], None);
        resolver.collect_program_in_module(&errors, &["Network".to_owned(), "Errors".to_owned()], None);

        let resolution = resolver.resolve_program(&entry).expect("resolve imported public items");

        assert_eq!(resolution.tables.used_import_spans.len(), 2);
        for span in use_spans {
            assert!(
                resolution.tables.used_import_spans.contains(&span),
                "resolved imported declaration must retain its originating use span: {span:?}"
            );
        }
    }

    /// `use` of a hub module reaches the public items of the child modules it declares with
    /// `pub mod`, the way the SDK hub `Beskid.Syntax.Nodes` exposes `NodeSpan`.
    #[test]
    fn hub_module_use_reaches_items_of_its_public_child_modules() {
        let span = parse_program("pub type NodeSpan { pub u64 start, }").expect("parse child");
        let field = parse_program("pub type Field { pub u64 id, }").expect("parse child");
        let hidden = parse_program("pub type Hidden { pub u64 id, }").expect("parse private child");
        let hub = parse_program(
            "pub mod Beskid.Syntax.Nodes.NodeSpan;\npub mod Beskid.Syntax.Nodes.Field;\nmod Beskid.Syntax.Nodes.Hidden;",
        )
        .expect("parse hub");
        let entry = parse_program("use Beskid.Syntax.Nodes;\npub NodeSpan Origin(Field field) { return NodeSpan { start: field.id }; }")
            .expect("parse entry");
        let nodes = ["Beskid".to_owned(), "Syntax".to_owned(), "Nodes".to_owned()];
        let child = |name: &str| {
            let mut path = nodes.to_vec();
            path.push(name.to_owned());
            path
        };

        let seeded = || {
            let mut resolver = Resolver::new();
            resolver.collect_program_in_module(&span, &child("NodeSpan"), None);
            resolver.collect_program_in_module(&field, &child("Field"), None);
            resolver.collect_program_in_module(&hidden, &child("Hidden"), None);
            resolver.collect_program_in_module(&hub, &nodes, None);
            resolver
        };
        seeded().resolve_program(&entry).expect("hub use reaches re-exported child items");

        let private = parse_program("use Beskid.Syntax.Nodes;\npub Hidden Leak() { return Hidden { id: 0 }; }")
            .expect("parse private probe");
        let errors = seeded().resolve_program(&private).expect_err("a private child module is not re-exported");
        assert!(
            errors.iter().any(|error| matches!(error, super::ResolveError::UnknownType { name, .. } if name == "Hidden")),
            "{errors:?}"
        );
    }

    /// Two public child modules providing one name at the same re-export distance are ambiguous:
    /// the hub `use` imports neither.
    #[test]
    fn hub_module_use_omits_names_two_children_provide() {
        let left = parse_program("pub type Shared { pub u64 left, }").expect("parse left");
        let right = parse_program("pub type Shared { pub u64 right, }").expect("parse right");
        let hub = parse_program("pub mod Hub.Left;\npub mod Hub.Right;").expect("parse hub");
        let entry = parse_program("use Hub;\npub Shared Pick(Shared value) { return value; }").expect("parse entry");

        let mut resolver = Resolver::new();
        resolver.collect_program_in_module(&left, &["Hub".to_owned(), "Left".to_owned()], None);
        resolver.collect_program_in_module(&right, &["Hub".to_owned(), "Right".to_owned()], None);
        resolver.collect_program_in_module(&hub, &["Hub".to_owned()], None);
        let errors = resolver.resolve_program(&entry).expect_err("ambiguous re-export fails closed");
        assert!(
            errors.iter().any(|error| matches!(error, super::ResolveError::UnknownType { name, .. } if name == "Shared")),
            "{errors:?}"
        );
    }

    /// A qualified path `Hub.Item` follows the same re-export rule as `use Hub;`: the hub's
    /// `pub mod` child provides the value, type, and callee, and the hub's own public declaration
    /// of a name shadows the re-exported one.
    #[test]
    fn qualified_hub_path_reaches_items_of_its_public_child_modules() {
        let option = parse_program(
            "pub type Option<T> { pub bool present, }\npub Option<T> Empty<T>() { return Option<T> { present: false }; }\npub i64 Depth() { return 2_i64; }",
        )
        .expect("parse child");
        let hub = parse_program("pub mod Core.Optional.Option;\npub i64 Depth() { return 1_i64; }").expect("parse hub");
        let entry = parse_program(
            "use Core.Optional;\npub Optional.Option<i64> Make() { Optional.Option<i64> value = Optional.Empty<i64>(); return value; }\npub i64 Which() { return Optional.Depth(); }",
        )
        .expect("parse entry");
        let mut resolver = Resolver::new();
        resolver.collect_program_in_module(&option, &["Core".to_owned(), "Optional".to_owned(), "Option".to_owned()], None);
        resolver.collect_program_in_module(&hub, &["Core".to_owned(), "Optional".to_owned()], None);
        let resolution = resolver.resolve_program(&entry).expect("qualified hub paths reach re-exported child items");

        let crate::syntax::Node::Function(which) = &entry.node.items[2].node else { panic!("function expected") };
        let crate::syntax::Statement::Return(returned) = &which.node.body.node.statements[0].node else {
            panic!("return expected")
        };
        let Some(expression) = &returned.node.value else { panic!("returned value expected") };
        let crate::syntax::Expression::Call(call) = &expression.node else { panic!("call expected") };
        let crate::syntax::Expression::Path(callee) = &call.node.callee.node else { panic!("path callee expected") };
        let Some(super::super::tables::ResolvedValue::Item(item)) =
            resolution.tables.resolved_values.get(&callee.node.path.span)
        else {
            panic!("`Optional.Depth` must resolve to an item")
        };
        let info = &resolution.items[item.0];
        assert_eq!(info.name, "Depth");
        assert!(
            info.source_path.is_none() && info.span == hub.node.items[1].span,
            "the hub's own `Depth` shadows the re-exported child `Depth`: {info:?}"
        );
    }

    /// Two public child modules providing one name at the same re-export distance make the
    /// qualified path ambiguous; it fails closed instead of picking either.
    #[test]
    fn qualified_hub_path_to_a_name_two_children_provide_is_ambiguous() {
        let left = parse_program("pub i64 Shared() { return 1_i64; }").expect("parse left");
        let right = parse_program("pub i64 Shared() { return 2_i64; }").expect("parse right");
        let hub = parse_program("pub mod Hub.Left;\npub mod Hub.Right;").expect("parse hub");
        let entry = parse_program("use Hub;\npub i64 Pick() { return Hub.Shared(); }").expect("parse entry");

        let mut resolver = Resolver::new();
        resolver.collect_program_in_module(&left, &["Hub".to_owned(), "Left".to_owned()], None);
        resolver.collect_program_in_module(&right, &["Hub".to_owned(), "Right".to_owned()], None);
        resolver.collect_program_in_module(&hub, &["Hub".to_owned()], None);
        let errors = resolver.resolve_program(&entry).expect_err("ambiguous re-export fails closed");
        assert!(
            errors.iter().any(|error| matches!(
                error,
                super::ResolveError::AmbiguousItemInModule { module_path, name, candidates: 2, .. }
                    if module_path == "Hub" && name == "Shared"
            )),
            "{errors:?}"
        );
    }

    /// A type a used module declares itself takes precedence over a same-named type another used
    /// hub only re-exports, whichever `use` comes first.
    #[test]
    fn direct_import_beats_hub_reexport_regardless_of_use_order() {
        let syntax_field = parse_program("pub type Field { pub u64 id, }").expect("parse syntax field");
        let hub = parse_program("pub mod Beskid.Syntax.Nodes.Field;").expect("parse hub");
        let semantic = parse_program("pub type Field { pub u64 token, }").expect("parse semantic");
        let entry = parse_program(
            "use Beskid.Syntax.Nodes;\nuse Beskid.Compiler.Semantic;\npub u64 Token(Field field) { return field.token; }",
        )
        .expect("parse entry");

        let mut resolver = Resolver::new();
        resolver.collect_program_in_module(
            &syntax_field,
            &["Beskid".to_owned(), "Syntax".to_owned(), "Nodes".to_owned(), "Field".to_owned()],
            None,
        );
        resolver.collect_program_in_module(&hub, &["Beskid".to_owned(), "Syntax".to_owned(), "Nodes".to_owned()], None);
        resolver.collect_program_in_module(
            &semantic,
            &["Beskid".to_owned(), "Compiler".to_owned(), "Semantic".to_owned()],
            None,
        );
        let resolution = resolver.resolve_program(&entry).expect("resolve entry");
        let crate::syntax::Node::Function(function) = &entry.node.items[2].node else { panic!("function expected") };
        let crate::syntax::Type::Complex(path) = &function.node.parameters[0].node.ty.node else {
            panic!("nominal parameter type expected")
        };
        let Some(super::super::tables::ResolvedType::Item(item)) = resolution.tables.resolved_types.get(&path.span)
        else {
            panic!("`Field` must resolve to a type")
        };
        // `Semantic.bd` is collected after the syntax `Field`, so its type has the larger id.
        let semantic_field = resolution
            .items
            .iter()
            .filter(|info| info.name == "Field" && info.kind == super::ItemKind::Type)
            .map(|info| info.id)
            .max_by_key(|id| id.0)
            .expect("Semantic.Field");
        assert_eq!(*item, semantic_field, "the directly imported Semantic.Field wins over the re-exported one");
    }
}
