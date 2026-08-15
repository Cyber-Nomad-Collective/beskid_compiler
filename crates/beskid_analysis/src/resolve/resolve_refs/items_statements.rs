use crate::syntax::{Block, ContractNode, Node, Statement};
use crate::syntax::Spanned;

use super::super::errors::ResolveError;
use super::super::items::ItemKind;
use super::super::resolver::Resolver;
use super::super::tables::ResolvedType;

impl Resolver {
    pub(super) fn resolve_item(&mut self, item: &Spanned<Node>) {
        match &item.node {
            Node::HostDefinition(_) => {}
            // syntax-free constant facts are consumed before executable lowering; there are no
            // types, locals, or references for the retired syntax resolver to traverse.
            Node::ConstantDefinition(_) => {}
            Node::Function(def) => {
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
            Node::Method(def) => {
                self.push_scope();
                self.resolve_type(&def.node.receiver_type);
                let previous_receiver = self.current_receiver_item_id;
                self.current_receiver_item_id = self.receiver_item_id_for_type(&def.node.receiver_type);
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
            Node::ExtendTypeDefinition(def) => {
                self.resolve_type(&def.node.target_type);
                for method in &def.node.methods {
                    self.push_scope();
                    self.resolve_type(&method.node.receiver_type);
                    let previous_receiver = self.current_receiver_item_id;
                    self.current_receiver_item_id = self.receiver_item_id_for_type(&method.node.receiver_type);
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
            Node::TestDefinition(def) => {
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
            Node::InlineModule(def) => {
                self.push_scope();
                let previous_module = self.current_module;
                let mut module_path =
                    self.module_graph.module(self.current_module).map(|module| module.path.clone()).unwrap_or_default();
                module_path.push(def.node.name.node.name.clone());
                let child_id = self.module_graph.ensure_module_path(&module_path);
                self.current_module = child_id;
                for item in &def.node.items {
                    self.resolve_item(item);
                }
                self.current_module = previous_module;
                self.pop_scope();
            }
            Node::TypeDefinition(def) => {
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
                    if self.items.get(conformance_item_id.0).is_some_and(|info| info.kind == ItemKind::Contract) {
                        self.tables.insert_type_conformance(type_item_id, *conformance_item_id, conformance.span);
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
                    self.current_receiver_item_id = self.receiver_item_id_for_type(&method.node.receiver_type);
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
            Node::EnumDefinition(def) => {
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
            Node::ContractDefinition(def) => {
                for node in &def.node.items {
                    match &node.node {
                        ContractNode::MethodSignature(signature) => {
                            for param in &signature.node.parameters {
                                self.resolve_type(&param.node.ty);
                            }
                            if let Some(return_type) = &signature.node.return_type {
                                self.resolve_type(return_type);
                            }
                        }
                        ContractNode::Embedding(_) => {}
                    }
                }
            }
            Node::AttributeDeclaration(_) => {}
            Node::ModuleDeclaration(_) | Node::UseDeclaration(_) => {}
            Node::MacroDefinition(_) => {}
        }
    }

    pub(super) fn resolve_block(&mut self, block: &Spanned<Block>) {
        self.push_scope();
        for statement in &block.node.statements {
            self.resolve_statement(statement);
        }
        self.pop_scope();
    }

    pub(super) fn resolve_if_statement(&mut self, if_stmt: &Spanned<crate::syntax::IfStatement>) {
        self.resolve_expression(&if_stmt.node.condition);
        self.resolve_block(&if_stmt.node.then_block);
        if let Some(else_branch) = &if_stmt.node.else_branch {
            match &else_branch.node {
                crate::syntax::ElseBranch::Block(block) => self.resolve_block(block),
                crate::syntax::ElseBranch::If(nested) => self.resolve_if_statement(nested),
            }
        }
    }

    pub(super) fn resolve_statement(&mut self, statement: &Spanned<Statement>) {
        match &statement.node {
            Statement::Let(let_stmt) => {
                if let Some(type_annotation) = &let_stmt.node.type_annotation {
                    self.resolve_type(type_annotation);
                }
                self.insert_local(&let_stmt.node.name.node.name, let_stmt.node.name.span);
                self.resolve_expression(&let_stmt.node.value);
            }
            Statement::Return(return_stmt) => {
                if let Some(value) = &return_stmt.node.value {
                    self.resolve_expression(value);
                }
            }
            Statement::Break(_) | Statement::Continue(_) => {}
            Statement::While(while_stmt) => {
                self.resolve_expression(&while_stmt.node.condition);
                self.resolve_block(&while_stmt.node.body);
            }
            Statement::For(for_stmt) => {
                self.resolve_expression(&for_stmt.node.iterable);
                self.push_scope();
                self.insert_local(&for_stmt.node.iterator.node.name, for_stmt.node.iterator.span);
                for stmt in &for_stmt.node.body.node.statements {
                    self.resolve_statement(stmt);
                }
                self.pop_scope();
            }
            Statement::If(if_stmt) => {
                self.resolve_if_statement(if_stmt);
            }
            Statement::Expression(expr_stmt) => {
                self.resolve_expression(&expr_stmt.node.expression);
            }
            Statement::With(_) | Statement::Launch(_) => {}
        }
    }
}
