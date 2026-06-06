use std::collections::HashMap;

use crate::hir::{
    HirBlock, HirContractNode, HirEnumPath, HirExpressionNode, HirItem, HirPath, HirPattern,
    HirProgram, HirStatementNode, HirStructLiteralField, HirType, HirVisibility,
};
use crate::syntax::{self, Spanned};

use super::errors::{ResolveError, ResolveResult, ResolveWarning};
use super::ids::{ItemId, LocalId};
use super::items::ItemKind;
use super::module_graph::ModuleGraph;
use super::resolver::{self, Resolution, Resolver};
use super::tables::{ResolutionTables, ResolvedType, ResolvedValue};

enum ModulePathLookup {
    Found(ItemId),
    ModuleMissing,
    NameMissing { module_path: String, name: String },
    NotVisible { module_path: String, name: String },
}

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

    pub fn resolve_collected_program(
        &mut self,
        program: &Spanned<HirProgram>,
    ) -> ResolveResult<Resolution> {
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

        if self.errors.is_empty() {
            Ok(self.take_resolution())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
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
                resolver::file_scoped_module_path(program)
                    .map(|path| self.module_graph.ensure_module_path(&path))
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
        Resolution {
            items: std::mem::take(&mut self.items),
            module_graph: std::mem::take(&mut self.module_graph),
            tables: std::mem::take(&mut self.tables),
            warnings: std::mem::take(&mut self.warnings),
            builtin_items: std::mem::take(&mut self.builtin_items),
            module_imports: std::mem::take(&mut self.module_imports),
            symbols: std::mem::take(&mut self.symbols),
            by_symbol: std::mem::take(&mut self.by_symbol),
        }
    }

    pub(crate) fn into_prefetch_parts(
        self,
    ) -> (
        Vec<super::items::ItemInfo>,
        ModuleGraph,
        HashMap<ItemId, usize>,
        super::symbol::SymbolRegistry,
        HashMap<super::symbol::SymbolId, ItemId>,
    ) {
        (
            self.items,
            self.module_graph,
            self.builtin_items,
            self.symbols,
            self.by_symbol,
        )
    }

    fn resolve_item(&mut self, item: &Spanned<HirItem>) {
        match &item.node {
            HirItem::HostDefinition(_) => {}
            HirItem::FunctionDefinition(def) => {
                self.push_generic_scope();
                for generic in &def.node.generics {
                    self.insert_generic(&generic.node.name);
                }
                self.push_scope();
                for param in &def.node.parameters {
                    self.resolve_type(&param.node.ty);
                    self.insert_local(&param.node.name.node.name, param.node.name.span);
                }
                if let Some(return_type) = &def.node.return_type {
                    self.resolve_type(return_type);
                }
                self.resolve_block(&def.node.body);
                self.pop_scope();
                self.pop_generic_scope();
            }
            HirItem::MethodDefinition(def) => {
                self.push_scope();
                self.resolve_type(&def.node.receiver_type);
                let previous_receiver = self.current_receiver_item_id;
                self.current_receiver_item_id =
                    self.receiver_item_id_for_type(&def.node.receiver_type);
                self.insert_local("this", def.node.receiver_type.span);
                for param in &def.node.parameters {
                    self.resolve_type(&param.node.ty);
                    self.insert_local(&param.node.name.node.name, param.node.name.span);
                }
                if let Some(return_type) = &def.node.return_type {
                    self.resolve_type(return_type);
                }
                self.resolve_block(&def.node.body);
                self.current_receiver_item_id = previous_receiver;
                self.pop_scope();
            }
            HirItem::ExtendTypeDefinition(def) => {
                self.resolve_type(&def.node.target_type);
                for method in &def.node.methods {
                    self.push_scope();
                    self.resolve_type(&method.node.receiver_type);
                    let previous_receiver = self.current_receiver_item_id;
                    self.current_receiver_item_id =
                        self.receiver_item_id_for_type(&method.node.receiver_type);
                    self.insert_local("this", method.node.receiver_type.span);
                    for param in &method.node.parameters {
                        self.resolve_type(&param.node.ty);
                        self.insert_local(&param.node.name.node.name, param.node.name.span);
                    }
                    if let Some(return_type) = &method.node.return_type {
                        self.resolve_type(return_type);
                    }
                    self.resolve_block(&method.node.body);
                    self.current_receiver_item_id = previous_receiver;
                    self.pop_scope();
                }
            }
            HirItem::TestDefinition(def) => {
                self.push_scope();
                if let Some(meta) = &def.node.meta {
                    for entry in &meta.node.entries {
                        self.resolve_expression(&entry.node.value);
                    }
                }
                if let Some(skip) = &def.node.skip {
                    for entry in &skip.node.entries {
                        self.resolve_expression(&entry.node.value);
                    }
                }
                self.resolve_block(&def.node.body);
                self.pop_scope();
            }
            HirItem::InlineModule(def) => {
                self.push_scope();
                let previous_module = self.current_module;
                let mut module_path = self
                    .module_graph
                    .module(self.current_module)
                    .map(|module| module.path.clone())
                    .unwrap_or_default();
                module_path.push(def.node.name.node.name.clone());
                let child_id = self.module_graph.ensure_module_path(&module_path);
                self.current_module = child_id;
                for item in &def.node.items {
                    self.resolve_item(item);
                }
                self.current_module = previous_module;
                self.pop_scope();
            }
            HirItem::TypeDefinition(def) => {
                self.push_generic_scope();
                for generic in &def.node.generics {
                    self.insert_generic(&generic.node.name);
                }
                let type_item_id = self.resolve_item_in_scope(&def.node.name.node.name);
                for conformance in &def.node.conformances {
                    self.resolve_type_path(conformance);
                    let Some(type_item_id) = type_item_id else {
                        continue;
                    };
                    let Some(ResolvedType::Item(conformance_item_id)) =
                        self.tables.resolved_types.get(&conformance.span)
                    else {
                        continue;
                    };
                    if self
                        .items
                        .get(conformance_item_id.0)
                        .is_some_and(|info| info.kind == ItemKind::Contract)
                    {
                        self.tables.insert_type_conformance(
                            type_item_id,
                            *conformance_item_id,
                            conformance.span,
                        );
                    } else if let Some(item) = self.items.get(conformance_item_id.0) {
                        self.errors.push(ResolveError::InvalidConformanceTarget {
                            name: item.name.clone(),
                            span: conformance.span,
                        });
                    }
                }
                for field in &def.node.fields {
                    self.resolve_type(&field.node.ty);
                }
                for method in &def.node.methods {
                    self.push_scope();
                    self.resolve_type(&method.node.receiver_type);
                    let previous_receiver = self.current_receiver_item_id;
                    self.current_receiver_item_id =
                        self.receiver_item_id_for_type(&method.node.receiver_type);
                    self.insert_local("this", method.node.receiver_type.span);
                    for param in &method.node.parameters {
                        self.resolve_type(&param.node.ty);
                        self.insert_local(&param.node.name.node.name, param.node.name.span);
                    }
                    if let Some(return_type) = &method.node.return_type {
                        self.resolve_type(return_type);
                    }
                    self.resolve_block(&method.node.body);
                    self.current_receiver_item_id = previous_receiver;
                    self.pop_scope();
                }
                self.pop_generic_scope();
            }
            HirItem::EnumDefinition(def) => {
                self.push_generic_scope();
                for generic in &def.node.generics {
                    self.insert_generic(&generic.node.name);
                }
                for variant in &def.node.variants {
                    for field in &variant.node.fields {
                        self.resolve_type(&field.node.ty);
                    }
                }
                self.pop_generic_scope();
            }
            HirItem::ContractDefinition(def) => {
                for node in &def.node.items {
                    match &node.node {
                        HirContractNode::MethodSignature(signature) => {
                            for param in &signature.node.parameters {
                                self.resolve_type(&param.node.ty);
                            }
                            if let Some(return_type) = &signature.node.return_type {
                                self.resolve_type(return_type);
                            }
                        }
                        HirContractNode::Embedding(_) => {}
                    }
                }
            }
            HirItem::AttributeDeclaration(_) => {}
            HirItem::ModuleDeclaration(_) | HirItem::UseDeclaration(_) => {}
            HirItem::MacroDefinition(_) => {}
        }
    }

    fn resolve_block(&mut self, block: &Spanned<HirBlock>) {
        self.push_scope();
        for statement in &block.node.statements {
            self.resolve_statement(statement);
        }
        self.pop_scope();
    }

    fn resolve_if_statement(&mut self, if_stmt: &Spanned<crate::hir::HirIfStatement>) {
        self.resolve_expression(&if_stmt.node.condition);
        self.resolve_block(&if_stmt.node.then_block);
        if let Some(else_branch) = &if_stmt.node.else_branch {
            match &else_branch.node {
                crate::hir::HirElseBranch::Block(block) => self.resolve_block(block),
                crate::hir::HirElseBranch::If(nested) => self.resolve_if_statement(nested),
            }
        }
    }

    fn resolve_statement(&mut self, statement: &Spanned<HirStatementNode>) {
        match &statement.node {
            HirStatementNode::LetStatement(let_stmt) => {
                if let Some(type_annotation) = &let_stmt.node.type_annotation {
                    self.resolve_type(type_annotation);
                }
                self.insert_local(&let_stmt.node.name.node.name, let_stmt.node.name.span);
                self.resolve_expression(&let_stmt.node.value);
            }
            HirStatementNode::ReturnStatement(return_stmt) => {
                if let Some(value) = &return_stmt.node.value {
                    self.resolve_expression(value);
                }
            }
            HirStatementNode::BreakStatement(_) | HirStatementNode::ContinueStatement(_) => {}
            HirStatementNode::WhileStatement(while_stmt) => {
                self.resolve_expression(&while_stmt.node.condition);
                self.resolve_block(&while_stmt.node.body);
            }
            HirStatementNode::ForStatement(for_stmt) => {
                self.resolve_expression(&for_stmt.node.iterable);
                self.push_scope();
                self.insert_local(
                    &for_stmt.node.iterator.node.name,
                    for_stmt.node.iterator.span,
                );
                for stmt in &for_stmt.node.body.node.statements {
                    self.resolve_statement(stmt);
                }
                self.pop_scope();
            }
            HirStatementNode::IfStatement(if_stmt) => {
                self.resolve_if_statement(if_stmt);
            }
            HirStatementNode::ExpressionStatement(expr_stmt) => {
                self.resolve_expression(&expr_stmt.node.expression);
            }
            HirStatementNode::WithStatement(_) | HirStatementNode::LaunchStatement(_) => {}
        }
    }

    fn resolve_expression(&mut self, expression: &Spanned<HirExpressionNode>) {
        match &expression.node {
            HirExpressionNode::MatchExpression(match_expr) => {
                self.resolve_expression(&match_expr.node.scrutinee);
                for arm in &match_expr.node.arms {
                    self.resolve_match_arm(arm);
                }
            }
            HirExpressionNode::LambdaExpression(lambda_expr) => {
                self.push_scope();
                for parameter in &lambda_expr.node.parameters {
                    if let Some(ty) = &parameter.node.ty {
                        self.resolve_type(ty);
                    }
                    self.insert_local(&parameter.node.name.node.name, parameter.node.name.span);
                }
                self.resolve_expression(&lambda_expr.node.body);
                self.pop_scope();
            }
            HirExpressionNode::AssignExpression(assign_expr) => {
                self.resolve_expression(&assign_expr.node.target);
                self.resolve_expression(&assign_expr.node.value);
            }
            HirExpressionNode::BinaryExpression(binary_expr) => {
                self.resolve_expression(&binary_expr.node.left);
                self.resolve_expression(&binary_expr.node.right);
            }
            HirExpressionNode::UnaryExpression(unary_expr) => {
                self.resolve_expression(&unary_expr.node.expr);
            }
            HirExpressionNode::CallExpression(call_expr) => {
                self.resolve_expression(&call_expr.node.callee);
                for arg in &call_expr.node.args {
                    self.resolve_expression(arg);
                }
            }
            HirExpressionNode::MemberExpression(member_expr) => {
                self.resolve_expression(&member_expr.node.target);
            }
            HirExpressionNode::LiteralExpression(_) => {}
            HirExpressionNode::PathExpression(path_expr) => {
                self.resolve_value_path(&path_expr.node.path);
            }
            HirExpressionNode::StructLiteralExpression(literal) => {
                self.resolve_type_path(&literal.node.path);
                for field in &literal.node.fields {
                    self.resolve_struct_literal_field(field);
                }
            }
            HirExpressionNode::EnumConstructorExpression(constructor) => {
                self.resolve_enum_path(&constructor.node.path);
                for arg in &constructor.node.args {
                    self.resolve_expression(arg);
                }
            }
            HirExpressionNode::BlockExpression(block_expr) => {
                self.resolve_block(&block_expr.node.block);
            }
            HirExpressionNode::GroupedExpression(grouped_expr) => {
                self.resolve_expression(&grouped_expr.node.expr);
            }
            HirExpressionNode::TryExpression(try_expr) => {
                self.resolve_expression(&try_expr.node.expr);
            }
            HirExpressionNode::SpawnExpression(spawn_expr) => {
                self.resolve_expression(&spawn_expr.node.callee);
            }
            HirExpressionNode::IndexExpression(index_expr) => {
                self.resolve_expression(&index_expr.node.target);
                self.resolve_expression(&index_expr.node.index);
            }
            HirExpressionNode::ArrayLiteralExpression(lit) => {
                for element in &lit.node.elements {
                    self.resolve_expression(element);
                }
            }
            HirExpressionNode::MacroInvocation(_) | HirExpressionNode::MacroMetavariable(_) => {}
        }
    }

    fn resolve_match_arm(&mut self, arm: &Spanned<crate::hir::HirMatchArm>) {
        self.push_scope();
        self.resolve_pattern(&arm.node.pattern);
        if let Some(guard) = &arm.node.guard {
            self.resolve_expression(guard);
        }
        self.resolve_expression(&arm.node.value);
        self.pop_scope();
    }

    fn resolve_pattern(&mut self, pattern: &Spanned<HirPattern>) {
        match &pattern.node {
            HirPattern::Wildcard => {}
            HirPattern::Identifier(identifier) => {
                self.insert_local(&identifier.node.name, identifier.span);
            }
            HirPattern::Literal(_) => {}
            HirPattern::Enum(enum_pattern) => {
                self.resolve_enum_path(&enum_pattern.node.path);
                for item in &enum_pattern.node.items {
                    self.resolve_pattern(item);
                }
            }
        }
    }

    fn resolve_struct_literal_field(&mut self, field: &Spanned<HirStructLiteralField>) {
        self.resolve_expression(&field.node.value);
    }

    fn resolve_type(&mut self, ty: &Spanned<HirType>) {
        match &ty.node {
            HirType::Primitive(_) => {}
            HirType::Complex(path) => self.resolve_type_path(path),
            HirType::Array(inner) => self.resolve_type(inner),
            HirType::Function {
                return_type,
                parameters,
            } => {
                self.resolve_type(return_type);
                for parameter in parameters {
                    self.resolve_type(parameter);
                }
            }
        }
    }

    fn resolve_value_path(&mut self, path: &Spanned<HirPath>) {
        let segments = resolver::path_segments(path);
        if segments.is_empty() {
            self.errors.push(ResolveError::UnknownValue {
                name: "<unnamed>".to_string(),
                span: path.span,
            });
            return;
        }
        if segments.len() == 1 {
            let name = &segments[0];
            if let Some(local) = self.resolve_local(name) {
                self.tables
                    .insert_value(path.span, ResolvedValue::Local(local));
                return;
            }
            if self.receiver_has_field(name)
                && let Some(this_local) = self.resolve_local("this")
            {
                self.tables
                    .insert_value(path.span, ResolvedValue::Local(this_local));
                return;
            }
            if let Some(item) = self.resolve_item_in_scope(name) {
                self.tables
                    .insert_value(path.span, ResolvedValue::Item(item));
                return;
            }
            self.errors.push(ResolveError::UnknownValue {
                name: (*name).clone(),
                span: path.span,
            });
            return;
        }
        if segments.len() >= 2 {
            if let Some(local) = self.resolve_local(&segments[0]) {
                self.tables
                    .insert_value(path.span, ResolvedValue::Local(local));
                return;
            }
            if self.resolve_item_in_scope(&segments[0]).is_none()
                && self
                    .module_graph
                    .module_id(std::slice::from_ref(&segments[0]))
                    .is_none()
                && self.module_imports.get(&segments[0]).is_none()
            {
                self.errors.push(ResolveError::UnknownValue {
                    name: segments[0].clone(),
                    span: path.span,
                });
                return;
            }
        }
        let lookup_segments = self.expand_import_alias(&segments);
        match self.resolve_item_in_module_path(&segments, &lookup_segments) {
            ModulePathLookup::Found(item) => {
                self.tables
                    .insert_value(path.span, ResolvedValue::Item(item));
            }
            ModulePathLookup::ModuleMissing => {
                if let Some(local) = self.resolve_local(&segments[0]) {
                    self.tables
                        .insert_value(path.span, ResolvedValue::Local(local));
                } else if let Some(item) = self.resolve_item_in_scope(&segments[0])
                    && self
                        .items
                        .get(item.0)
                        .is_some_and(|info| info.kind == ItemKind::Contract)
                {
                    self.tables
                        .insert_value(path.span, ResolvedValue::Item(item));
                } else {
                    self.errors.push(ResolveError::UnknownModulePath {
                        path: segments[..segments.len() - 1].join("::"),
                        span: path.span,
                    });
                }
            }
            ModulePathLookup::NameMissing { module_path, name } => {
                self.errors.push(ResolveError::UnknownValueInModule {
                    module_path,
                    name,
                    span: path.span,
                });
            }
            ModulePathLookup::NotVisible { module_path, name } => {
                self.errors.push(ResolveError::PrivateItemInModule {
                    module_path,
                    name,
                    span: path.span,
                });
            }
        }
    }

    fn resolve_type_path(&mut self, path: &Spanned<HirPath>) {
        let segments = resolver::path_segments(path);
        if segments.is_empty() {
            self.errors.push(ResolveError::UnknownType {
                name: "<unnamed>".to_string(),
                span: path.span,
            });
            return;
        }
        if segments.len() == 1 {
            let name = &segments[0];
            if self.is_generic(name) {
                self.tables
                    .insert_type(path.span, ResolvedType::Generic(name.clone()));
                return;
            }
            if let Some(item) = self.resolve_item_in_scope(name) {
                self.tables.insert_type(path.span, ResolvedType::Item(item));
                return;
            }
            self.errors.push(ResolveError::UnknownType {
                name: (*name).clone(),
                span: path.span,
            });
            return;
        }
        let lookup_segments = self.expand_import_alias(&segments);
        match self.resolve_item_in_module_path(&segments, &lookup_segments) {
            ModulePathLookup::Found(item) => {
                self.tables.insert_type(path.span, ResolvedType::Item(item));
            }
            ModulePathLookup::ModuleMissing => {
                self.errors.push(ResolveError::UnknownModulePath {
                    path: segments[..segments.len() - 1].join("::"),
                    span: path.span,
                });
            }
            ModulePathLookup::NameMissing { module_path, name } => {
                self.errors.push(ResolveError::UnknownTypeInModule {
                    module_path,
                    name,
                    span: path.span,
                });
            }
            ModulePathLookup::NotVisible { module_path, name } => {
                self.errors.push(ResolveError::PrivateItemInModule {
                    module_path,
                    name,
                    span: path.span,
                });
            }
        }
    }

    fn resolve_enum_path(&mut self, path: &Spanned<HirEnumPath>) {
        self.resolve_type_path(&path.node.type_path);
        if let Some(resolved) = self
            .tables
            .resolved_types
            .get(&path.node.type_path.span)
            .cloned()
        {
            self.tables.insert_type(path.span, resolved);
        }
    }

    fn insert_generic(&mut self, name: &str) {
        let scope = match self.generic_scopes.last_mut() {
            Some(scope) => scope,
            None => return,
        };
        scope.insert(name.to_string(), ());
    }

    fn is_generic(&self, name: &str) -> bool {
        self.generic_scopes
            .iter()
            .rev()
            .any(|scope| scope.contains_key(name))
    }

    fn resolve_local(&self, name: &str) -> Option<LocalId> {
        for scope in self.local_scopes.iter().rev() {
            if let Some(local) = scope.get(name).copied() {
                return Some(local);
            }
        }
        None
    }

    fn resolve_item_in_scope(&self, name: &str) -> Option<ItemId> {
        let mut current = Some(self.current_module);
        while let Some(module_id) = current {
            let module = self.module_graph.module(module_id)?;
            if let Some(item) = module.scope.get(name).copied() {
                return Some(item);
            }
            current = module.parent;
        }
        None
    }

    fn expand_import_alias(&self, segments: &[String]) -> Vec<String> {
        if segments.len() < 2 {
            return segments.to_vec();
        }
        let Some(module_path) = self.module_imports.get(&segments[0]) else {
            return segments.to_vec();
        };
        let mut expanded = module_path.clone();
        expanded.extend_from_slice(&segments[1..]);
        expanded
    }

    fn resolve_item_in_module_path(
        &self,
        original_segments: &[String],
        lookup_segments: &[String],
    ) -> ModulePathLookup {
        if lookup_segments.len() < 2 {
            return ModulePathLookup::ModuleMissing;
        }
        let primary = self.lookup_item_in_parent_module(lookup_segments);
        if matches!(primary, ModulePathLookup::Found(_)) {
            return primary;
        }

        // `use Console.Controls.ProgressBar; ProgressBar.ProgressBar.New()` — member in aliased module.
        if original_segments.len() >= 3 {
            if let Some(base_module) = self.module_imports.get(&original_segments[0]) {
                let member = &original_segments[original_segments.len() - 1];
                if let ModulePathLookup::Found(item) =
                    self.lookup_named_item_in_module(base_module, member)
                {
                    return ModulePathLookup::Found(item);
                }
            }
        }

        // `Console.Controls.Panel.Panel.Render` — skip homonymous type segment in fully qualified paths.
        if original_segments.len() >= 4 {
            let member = &original_segments[original_segments.len() - 1];
            let module_path: Vec<String> =
                original_segments[..original_segments.len() - 2].to_vec();
            if let ModulePathLookup::Found(item) =
                self.lookup_named_item_in_module(&module_path, member)
            {
                return ModulePathLookup::Found(item);
            }
        }

        // `Concurrency.Channel`, `Ansi.StyleChain` — homonymous type in leaf module path.
        if let ModulePathLookup::Found(item) = self.lookup_homonymous_module_item(lookup_segments) {
            return ModulePathLookup::Found(item);
        }

        primary
    }

    fn lookup_item_in_parent_module(&self, segments: &[String]) -> ModulePathLookup {
        let (module_path, tail) = segments.split_at(segments.len() - 1);
        self.lookup_named_item_in_module(module_path, &tail[0])
    }

    fn lookup_named_item_in_module(&self, module_path: &[String], name: &str) -> ModulePathLookup {
        let Some(module_id) = self.module_graph.module_id(module_path) else {
            return ModulePathLookup::ModuleMissing;
        };
        let Some(module) = self.module_graph.module(module_id) else {
            return ModulePathLookup::ModuleMissing;
        };

        let module_path_string = module_path.join("::");
        if let Some(item) = module.scope.get(name).copied() {
            if !module_path.is_empty()
                && self
                    .items
                    .get(item.0)
                    .is_some_and(|info| info.visibility == HirVisibility::Private)
            {
                ModulePathLookup::NotVisible {
                    module_path: module_path_string,
                    name: name.to_string(),
                }
            } else {
                ModulePathLookup::Found(item)
            }
        } else {
            ModulePathLookup::NameMissing {
                module_path: module_path_string,
                name: name.to_string(),
            }
        }
    }

    /// When `Foo.Bar` names module `Foo.Bar` and public item `Bar` inside it.
    fn lookup_homonymous_module_item(&self, segments: &[String]) -> ModulePathLookup {
        if segments.len() < 2 {
            return ModulePathLookup::ModuleMissing;
        }
        let item_name = segments[segments.len() - 1].clone();
        self.lookup_named_item_in_module(segments, &item_name)
    }

    fn receiver_item_id_for_type(&self, receiver_type: &Spanned<HirType>) -> Option<ItemId> {
        match self.tables.resolved_types.get(&receiver_type.span) {
            Some(ResolvedType::Item(item_id)) => Some(*item_id),
            _ => None,
        }
    }

    fn receiver_has_field(&self, field_name: &str) -> bool {
        let Some(receiver_item_id) = self.current_receiver_item_id else {
            return false;
        };
        let Some(receiver) = self.items.get(receiver_item_id.0) else {
            return false;
        };
        let member_name = format!("{}::{}", receiver.name, field_name);
        self.items
            .iter()
            .any(|info| info.kind == ItemKind::Field && info.name == member_name)
    }

    fn insert_local(&mut self, name: &str, span: syntax::SpanInfo) {
        if let Some((_, previous_span)) = self.find_shadowed_local(name) {
            self.warnings.push(ResolveWarning::ShadowedLocal {
                name: name.to_string(),
                span,
                previous: previous_span,
            });
        } else if let Some(previous_item) = self.resolve_item_in_scope(name) {
            let previous_span = self
                .items
                .get(previous_item.0)
                .map(|item| item.span)
                .unwrap_or(span);
            self.warnings.push(ResolveWarning::ShadowedLocal {
                name: name.to_string(),
                span,
                previous: previous_span,
            });
        }
        let scope = match self.local_scopes.last_mut() {
            Some(scope) => scope,
            None => return,
        };
        if let Some(prev) = scope.get(name).copied() {
            let previous = self
                .tables
                .local_info(prev)
                .map(|info| info.span)
                .unwrap_or(span);
            self.errors.push(ResolveError::DuplicateLocal {
                name: name.to_string(),
                span,
                previous,
            });
            return;
        }
        let id = self
            .tables
            .intern_local(name.to_string(), span, self.current_source_path.clone());
        scope.insert(name.to_string(), id);
    }

    fn find_shadowed_local(&self, name: &str) -> Option<(LocalId, syntax::SpanInfo)> {
        for scope in self.local_scopes.iter().rev().skip(1) {
            if let Some(local) = scope.get(name).copied() {
                let span = self
                    .tables
                    .local_info(local)
                    .map(|info| info.span)
                    .unwrap_or_else(|| syntax::SpanInfo {
                        start: 0,
                        end: 0,
                        line_col_start: (1, 1),
                        line_col_end: (1, 1),
                    });
                return Some((local, span));
            }
        }
        None
    }

    fn push_scope(&mut self) {
        self.local_scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.local_scopes.pop();
    }

    fn push_generic_scope(&mut self) {
        self.generic_scopes.push(HashMap::new());
    }

    fn pop_generic_scope(&mut self) {
        self.generic_scopes.pop();
    }
}
