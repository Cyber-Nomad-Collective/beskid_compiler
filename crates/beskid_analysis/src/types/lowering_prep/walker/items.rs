use crate::syntax::{MethodDefinition, Node, PrimitiveType, SpanInfo, Spanned};

use super::super::substitution::{find_generic_param, primitive_type_id};
use super::state::PrepWalker;

impl<'a> PrepWalker<'a> {
    pub(super) fn walk_item(&mut self, item: &Spanned<Node>) {
        match &item.node {
            Node::Function(def) => {
                self.with_source_path_from_item(item.span, |w| {
                    let mut generics = Vec::new();
                    for g in &def.node.generics {
                        if let Some(id) = find_generic_param(w.surfaces.types, &g.node.name) {
                            w.generic_params.insert(g.node.name.clone(), id);
                            generics.push(g.node.name.clone());
                        }
                    }
                    w.current_return_type = def
                        .node
                        .return_type
                        .as_ref()
                        .and_then(|t| w.type_id_for_program_type(t))
                        .or_else(|| w.return_type_for_item_span(item.span))
                        .or_else(|| primitive_type_id(w.surfaces.types, PrimitiveType::Unit));
                    w.walk_block(&def.node.body);
                    for name in generics {
                        w.generic_params.remove(&name);
                    }
                });
            }
            Node::Method(def) => self.walk_method_definition(item.span, def),
            Node::ExtendTypeDefinition(def) => {
                for m in &def.node.methods {
                    self.walk_method_definition(m.span, m);
                }
            }
            Node::TestDefinition(def) => self.with_source_path_from_item(item.span, |w| {
                w.current_return_type = primitive_type_id(w.surfaces.types, PrimitiveType::Unit);
                for stmt in &def.node.statements {
                    w.walk_statement(stmt);
                }
            }),
            Node::TypeDefinition(def) => {
                for m in &def.node.methods {
                    self.walk_method_definition(m.span, m);
                }
            }
            Node::InlineModule(m) => {
                for nested in &m.node.items {
                    self.walk_item(nested);
                }
            }
            _ => {}
        }
    }

    pub(super) fn walk_method_definition(&mut self, span: SpanInfo, def: &Spanned<MethodDefinition>) {
        self.with_source_path_from_item(span, |w| {
            w.current_return_type = def
                .node
                .return_type
                .as_ref()
                .and_then(|t| w.type_id_for_program_type(t))
                .or_else(|| w.return_type_for_item_span(span))
                .or_else(|| primitive_type_id(w.surfaces.types, PrimitiveType::Unit));
            w.walk_block(&def.node.body);
        });
    }

    pub(super) fn with_source_path_from_item(&mut self, span: SpanInfo, f: impl FnOnce(&mut Self)) {
        let prev = self.current_source_path.clone();
        if let Some(info) = self.resolution.items.iter().find(|i| i.span == span) {
            self.current_source_path = info.source_path.as_ref().map(|p| crate::paths::unit_path_key(p.as_path()));
        }
        f(self);
        self.current_source_path = prev;
    }
}
