use std::collections::HashMap;

use crate::syntax::items::{InlineModule, MacroDefinition, Node, Program};
use crate::syntax::{Identifier, Spanned};

/// Maps macro name (identifier text) to its definition in the current compilation unit.
#[derive(Debug, Default)]
pub struct MacroRegistry {
    defs: HashMap<String, Spanned<MacroDefinition>>,
}

impl MacroRegistry {
    pub fn from_program(program: &Program) -> Self {
        let mut registry = Self::default();
        registry.collect_items(&program.items);
        registry
    }

    pub fn get(&self, name: &str) -> Option<&Spanned<MacroDefinition>> {
        self.defs.get(name)
    }

    fn collect_items(&mut self, items: &[Spanned<Node>]) {
        for item in items {
            self.collect_node(item);
        }
    }

    fn collect_node(&mut self, item: &Spanned<Node>) {
        match &item.node {
            Node::MacroDefinition(def) => {
                let key = def.node.name.node.name.clone();
                self.defs.insert(key, def.clone());
            }
            Node::InlineModule(m) => self.collect_inline_module(m),
            _ => {}
        }
    }

    fn collect_inline_module(&mut self, module: &Spanned<InlineModule>) {
        self.collect_items(&module.node.items);
    }
}

pub fn macro_name_key(name: &Spanned<Identifier>) -> String {
    name.node.name.clone()
}
