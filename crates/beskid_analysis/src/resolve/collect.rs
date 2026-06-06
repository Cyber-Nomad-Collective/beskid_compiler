use std::path::PathBuf;

use crate::hir::{HirItem, HirType, HirUseDeclaration, HirVisibility};
use crate::syntax::{self, Spanned};

use super::errors::ResolveError;
use super::ids::ItemId;
use super::items::{ItemInfo, ItemKind};
use super::member_items;
use super::module_graph::ModuleGraph;
use super::resolver::{self, Resolver};
use super::symbol::{
    symbol_shape_for_item, symbol_to_string, BUILTIN_PACKAGE, SymbolId, SymbolQualifier,
    SymbolShape,
};
use crate::builtins::builtin_specs;

pub(super) fn type_name_for_method_receiver(receiver_type: &Spanned<HirType>) -> String {
    match &receiver_type.node {
        HirType::Primitive(primitive) => format!("{:?}", primitive.node),
        HirType::Complex(path) => path
            .node
            .segments
            .iter()
            .map(|segment| segment.node.name.node.name.clone())
            .collect::<Vec<_>>()
            .join("."),
        HirType::Array(_) => "Array".to_string(),
        HirType::Function { .. } => "Function".to_string(),
    }
}

pub(super) fn path_tail(path: &Spanned<crate::hir::HirPath>) -> String {
    path.node
        .segments
        .last()
        .map(|segment| segment.node.name.node.name.clone())
        .unwrap_or_else(|| "<unnamed>".to_string())
}

pub(super) fn builtin_span() -> syntax::SpanInfo {
    syntax::SpanInfo {
        start: 0,
        end: 0,
        line_col_start: (1, 1),
        line_col_end: (1, 1),
    }
}

pub(super) fn use_imported_name(use_decl: &HirUseDeclaration) -> String {
    use_decl
        .alias
        .as_ref()
        .map(|alias| alias.node.name.clone())
        .unwrap_or_else(|| path_tail(&use_decl.path))
}

impl Resolver {
    pub(crate) fn set_current_source_path(&mut self, source_path: Option<PathBuf>) {
        self.current_source_path = source_path.map(|path| crate::paths::unit_path_key(&path));
    }

    pub(crate) fn set_declaring_package(&mut self, package: String) {
        self.declaring_package = package;
    }

    pub fn with_module_prefetch(
        items: Vec<ItemInfo>,
        module_graph: ModuleGraph,
        builtin_items: std::collections::HashMap<ItemId, usize>,
        symbols: super::symbol::SymbolRegistry,
        by_symbol: std::collections::HashMap<SymbolId, ItemId>,
    ) -> Self {
        Self {
            items,
            module_graph,
            builtin_items,
            symbols,
            by_symbol,
            ..Self::default()
        }
    }

    fn current_module_path(&self) -> Vec<String> {
        self.module_graph
            .module(self.current_module)
            .map(|module| module.path.clone())
            .unwrap_or_default()
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
        let shape = symbol_shape_for_item(
            kind,
            module_path,
            name,
            method_receiver,
            parent_symbol,
            member_name,
        )?;
        let qualifier = SymbolQualifier {
            package: self.declaring_package.clone(),
            shape,
        };
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
        program: &Spanned<crate::hir::HirProgram>,
        module_path: &[String],
        source_path: Option<&PathBuf>,
    ) {
        self.current_source_path = source_path.map(|path| crate::paths::unit_path_key(&path));
        if resolver::file_scoped_module_index(program).is_some() {
            self.collect_program(program);
            return;
        }
        self.current_module = self.module_graph.ensure_module_path(module_path);
        for item in &program.node.items {
            self.collect_item(item);
        }
    }

    pub fn collect_program(&mut self, program: &Spanned<crate::hir::HirProgram>) {
        self.module_imports.clear();
        let file_scoped_module_index = resolver::file_scoped_module_index(program);
        self.current_module = resolver::file_scoped_module_path(program)
            .map(|path| self.module_graph.ensure_module_path(&path))
            .unwrap_or(self.module_graph.root());
        for (index, item) in program.node.items.iter().enumerate() {
            if Some(index) == file_scoped_module_index {
                continue;
            }
            self.collect_item(item);
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
                self.errors.push(ResolveError::DuplicateItem {
                    name,
                    span: builtin_span(),
                    previous: prev_span,
                });
                continue;
            }
            let path: Vec<String> = spec
                .beskid_path
                .iter()
                .map(|segment| (*segment).to_string())
                .collect();
            let qualifier = SymbolQualifier {
                package: BUILTIN_PACKAGE.to_string(),
                shape: SymbolShape::Builtin { path },
            };
            let symbol_id = self.symbols.intern(qualifier);
            self.by_symbol.insert(symbol_id, id);
            self.items.push(ItemInfo {
                id,
                parent_id: None,
                name,
                kind: ItemKind::Function,
                visibility: HirVisibility::Public,
                span: builtin_span(),
                source_path: self.current_source_path.clone(),
                symbol: Some(symbol_id),
            });
            self.builtin_items.insert(id, index);
        }
        self.declaring_package = saved_package;
    }

    fn collect_item(&mut self, item: &Spanned<HirItem>) {
        let (name, kind, visibility, method_receiver) = match &item.node {
            HirItem::HostDefinition(_) => {
                return;
            }
            HirItem::FunctionDefinition(def) => (
                def.node.name.node.name.clone(),
                ItemKind::Function,
                def.node.visibility.node,
                None,
            ),
            HirItem::MethodDefinition(def) => {
                let receiver = type_name_for_method_receiver(&def.node.receiver_type);
                (
                    format!("{}::{}", receiver, def.node.name.node.name),
                    ItemKind::Method,
                    def.node.visibility.node,
                    Some(receiver),
                )
            }
            HirItem::ExtendTypeDefinition(def) => {
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
            HirItem::TestDefinition(def) => (
                def.node.name.node.name.clone(),
                ItemKind::Test,
                def.node.visibility.node,
                None,
            ),
            HirItem::TypeDefinition(def) => (
                def.node.name.node.name.clone(),
                ItemKind::Type,
                def.node.visibility.node,
                None,
            ),
            HirItem::EnumDefinition(def) => (
                def.node.name.node.name.clone(),
                ItemKind::Enum,
                def.node.visibility.node,
                None,
            ),
            HirItem::ContractDefinition(def) => (
                def.node.name.node.name.clone(),
                ItemKind::Contract,
                def.node.visibility.node,
                None,
            ),
            HirItem::ModuleDeclaration(def) => (
                path_tail(&def.node.path),
                ItemKind::Module,
                def.node.visibility.node,
                None,
            ),
            HirItem::InlineModule(def) => (
                def.node.name.node.name.clone(),
                ItemKind::Module,
                def.node.visibility.node,
                None,
            ),
            HirItem::UseDeclaration(def) => {
                self.collect_use_declaration(item, def);
                return;
            }
            HirItem::AttributeDeclaration(_) => {
                return;
            }
            HirItem::MacroDefinition(_) => {
                return;
            }
        };

        let id = ItemId(self.items.len());
        let module_id = match &item.node {
            HirItem::ModuleDeclaration(def) => {
                let segments: Vec<String> = def
                    .node
                    .path
                    .node
                    .segments
                    .iter()
                    .map(|segment| segment.node.name.node.name.clone())
                    .collect();
                let parent_path = &segments[..segments.len().saturating_sub(1)];
                self.module_graph.ensure_module_path(parent_path)
            }
            _ => self.current_module,
        };
        if let Some(prev) = self.module_graph.insert_item(module_id, name.clone(), id) {
            let prev_span = self.items[prev.0].span;
            self.errors.push(ResolveError::DuplicateItem {
                name,
                span: item.span,
                previous: prev_span,
            });
            return;
        }
        let push_module_path = match &item.node {
            HirItem::ModuleDeclaration(def) => {
                let segments: Vec<String> = def
                    .node
                    .path
                    .node
                    .segments
                    .iter()
                    .map(|segment| segment.node.name.node.name.clone())
                    .collect();
                let mut path = self.current_module_path();
                path.extend_from_slice(&segments[..segments.len().saturating_sub(1)]);
                path
            }
            _ => self.current_module_path(),
        };
        self.push_item(
            id,
            None,
            name,
            kind,
            visibility,
            item.span,
            method_receiver,
            push_module_path,
        );

        self.collect_member_items(item, id);

        if let HirItem::TypeDefinition(def) = &item.node {
            let type_name = def.node.name.node.name.clone();
            let field_names: std::collections::HashSet<_> = def
                .node
                .fields
                .iter()
                .map(|field| field.node.name.node.name.as_str())
                .collect();
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

        if let HirItem::ModuleDeclaration(def) = &item.node {
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
        if let HirItem::InlineModule(def) = &item.node {
            let previous_module = self.current_module;
            let mut module_path = self
                .module_graph
                .module(self.current_module)
                .map(|module| module.path.clone())
                .unwrap_or_default();
            module_path.push(def.node.name.node.name.clone());
            let child_module = self.module_graph.ensure_module_path(&module_path);
            self.current_module = child_module;
            for nested in &def.node.items {
                self.collect_item(nested);
            }
            self.current_module = previous_module;
        }
    }

    fn collect_use_declaration(
        &mut self,
        item: &Spanned<HirItem>,
        def: &Spanned<HirUseDeclaration>,
    ) {
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
            self.errors.push(ResolveError::UnknownModulePath {
                path: module_path.join("::"),
                span: def.node.path.span,
            });
            return;
        }
        self.module_imports.insert(alias, module_path.clone());
        self.import_public_items_from_module(&module_path);
    }

    /// Bring public items from a prelude or `use` module into the current module scope.
    pub(crate) fn apply_prelude_imports(&mut self, module_path: &[String]) {
        self.import_public_items_from_module(module_path);
    }

    /// Bring public items from a used module into the current module scope (types, enums, functions).
    fn import_public_items_from_module(&mut self, module_path: &[String]) {
        let Some(target_module_id) = self.module_graph.module_id(module_path) else {
            return;
        };
        let Some(target_module) = self.module_graph.module(target_module_id) else {
            return;
        };
        let imports: Vec<(String, ItemId)> = target_module
            .scope
            .iter()
            .filter_map(|(name, item_id)| {
                let info = self.items.get(item_id.0)?;
                if info.visibility != HirVisibility::Public {
                    return None;
                }
                match info.kind {
                    ItemKind::Function
                    | ItemKind::Enum
                    | ItemKind::Type
                    | ItemKind::Contract => Some((name.clone(), *item_id)),
                    _ => None,
                }
            })
            .collect();
        for (name, item_id) in imports {
            if let Some(_prev) = self.module_graph.insert_item(self.current_module, name, item_id) {
                // Import collides with an existing local declaration — silently skip
                continue;
            }
        }
    }

    fn push_member_item(
        &mut self,
        name: String,
        kind: ItemKind,
        visibility: HirVisibility,
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
        visibility: HirVisibility,
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

    fn collect_member_items(&mut self, item: &Spanned<HirItem>, parent_id: ItemId) {
        let Some(parent) = self.items.get(parent_id.0) else {
            return;
        };
        let parent_name = parent.name.clone();
        let visibility = parent.visibility;
        for spec in member_items::collect_member_items(item, &parent_name) {
            self.push_member_item(spec.name, spec.kind, visibility, spec.span, parent_id);
        }
    }

    fn collect_member_items_for_method(
        &mut self,
        method: &Spanned<crate::hir::HirMethodDefinition>,
        parent_id: ItemId,
    ) {
        let parent_name = self
            .items
            .get(parent_id.0)
            .map(|item| item.name.clone())
            .unwrap_or_default();
        let visibility = self
            .items
            .get(parent_id.0)
            .map(|item| item.visibility)
            .unwrap_or(HirVisibility::Private);
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