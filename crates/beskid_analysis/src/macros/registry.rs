use std::collections::{HashMap, HashSet};

use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::syntax::items::{InlineModule, MacroDefinition, Node, Program};
use crate::syntax::{Identifier, SpanInfo, Spanned};

/// Maps macro name (identifier text) to its definition in the current compilation unit.
#[derive(Debug, Default)]
pub struct MacroRegistry {
    defs: HashMap<String, Spanned<MacroDefinition>>,
    pub registry_issues: Vec<(SpanInfo, SemanticIssueKind)>,
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
            Node::MacroDefinition(def) => self.register_definition(def),
            Node::InlineModule(m) => self.collect_inline_module(m),
            _ => {}
        }
    }

    fn register_definition(&mut self, def: &Spanned<MacroDefinition>) {
        let key = def.node.name.node.name.clone();
        if self.defs.contains_key(&key) {
            self.registry_issues.push((
                def.span,
                SemanticIssueKind::MacroAmbiguousName { name: key.clone() },
            ));
        }
        self.check_duplicate_parameters(def);
        self.defs.insert(key, def.clone());
    }

    fn check_duplicate_parameters(&mut self, def: &Spanned<MacroDefinition>) {
        let mut seen = HashSet::new();
        for param in &def.node.parameters {
            let name = param.node.name.node.name.clone();
            if !seen.insert(name.clone()) {
                self.registry_issues.push((
                    param.span,
                    SemanticIssueKind::MacroDuplicateParameter {
                        name: def.node.name.node.name.clone(),
                        parameter: name,
                    },
                ));
            }
        }
    }

    fn collect_inline_module(&mut self, module: &Spanned<InlineModule>) {
        self.collect_items(&module.node.items);
    }
}

pub fn macro_name_key(name: &Spanned<Identifier>) -> String {
    name.node.name.clone()
}

#[cfg(test)]
mod tests {
    use crate::services::parse_program_with_source_name;

    use super::*;

    #[test]
    fn collects_top_level_and_inline_module_macros() {
        let source = r#"
mod Inner {
    macro inner (expression x) { $x; }
}
macro outer (expression y) { $y; }
"#;
        let program = parse_program_with_source_name("M.bd", source).expect("parse");
        let registry = MacroRegistry::from_program(&program.node);
        assert!(registry.get("outer").is_some());
        assert!(registry.get("inner").is_some());
    }

    #[test]
    fn duplicate_macro_name_last_definition_wins_in_registry() {
        let source = "macro dup (expression x) { $x; }\nmacro dup (expression y) { $y; }\n";
        let program = parse_program_with_source_name("M.bd", source).expect("parse");
        let registry = MacroRegistry::from_program(&program.node);
        let def = registry.get("dup").expect("definition");
        assert_eq!(def.node.parameters[0].node.name.node.name, "y");
        assert_eq!(registry.registry_issues.len(), 1);
    }
}
