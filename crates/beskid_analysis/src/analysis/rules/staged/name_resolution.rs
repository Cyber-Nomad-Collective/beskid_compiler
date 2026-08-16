use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::rules::{RuleContext, resolve};
use crate::resolve::{Resolution, Resolver};
use crate::syntax::{Block, Expression, ForStatement, LetStatement, Node, Path, Program, Statement, UseDeclaration};
use crate::syntax::{SpanInfo, Spanned};
use crate::syntax_query::{AstWalker, NodeKind, NodeRef, Visit};
use std::collections::{HashMap, HashSet};

impl SemanticPipelineRule {
    pub(super) fn stage1_name_resolution(
        &self,
        ctx: &mut RuleContext,
        program: &Spanned<Program>,
    ) -> Option<Resolution> {
        self.check_ambiguous_imports(ctx, program);
        self.check_unknown_import_paths(ctx, program);
        self.check_use_before_declaration(ctx, program);

        let resolution = if let Some(assembly) = ctx.options.program_assembly.as_ref() {
            match assembly.module_index.resolve_entry_program(
                program,
                ctx.options.entry_source_path.as_deref(),
                assembly,
            ) {
                Ok(resolution) => resolution,
                Err(errors) => {
                    for error in errors {
                        resolve::emit_resolve_error(ctx, error);
                    }
                    return None;
                }
            }
        } else {
            let mut resolver = Resolver::new();
            match resolver.resolve_program(program) {
                Ok(resolution) => resolution,
                Err(errors) => {
                    for error in errors {
                        resolve::emit_resolve_error(ctx, error);
                    }
                    return None;
                }
            }
        };

        for warning in &resolution.warnings {
            resolve::emit_resolve_warning(ctx, warning);
        }

        Some(resolution)
    }

    fn check_ambiguous_imports(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        let mut seen: HashMap<String, SpanInfo> = HashMap::new();

        for item in &program.node.items {
            let Node::UseDeclaration(use_decl) = &item.node else {
                continue;
            };
            let imported_name = self.imported_name_local(&use_decl.node);
            let imported_span = use_decl.node.alias.as_ref().map(|alias| alias.span).unwrap_or(use_decl.node.path.span);
            let Some(previous_span) = seen.insert(imported_name.clone(), imported_span) else {
                continue;
            };

            ctx.emit_issue(
                imported_span,
                SemanticIssueKind::AmbiguousImport { name: imported_name, previous: previous_span },
            );
        }
    }

    fn import_path_known_in_assembly(use_path: &str, known_paths: &HashSet<String>) -> bool {
        if known_paths.contains(use_path) {
            return true;
        }
        let dotted = use_path.replace("::", ".");
        if known_paths.contains(&dotted) {
            return true;
        }
        let colon = use_path.replace('.', "::");
        known_paths.contains(&colon)
    }

    fn check_unknown_import_paths(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        if let Some(known_paths) = ctx.options.known_assembly_module_paths.clone() {
            for item in &program.node.items {
                let Node::UseDeclaration(use_decl) = &item.node else {
                    continue;
                };
                let path = self.path_to_string_local(&use_decl.node.path);
                if Self::import_path_known_in_assembly(&path, &known_paths) {
                    continue;
                }
                ctx.emit_issue(use_decl.node.path.span, SemanticIssueKind::UnknownImportPath { path });
            }
            return;
        }

        let mut known_roots = HashSet::new();
        known_roots.insert("std".to_string());
        for item in &program.node.items {
            match &item.node {
                Node::ModuleDeclaration(module_decl) => {
                    if let Some(segment) = module_decl.node.path.node.segments.first() {
                        known_roots.insert(segment.node.name.node.name.clone());
                    }
                }
                Node::Function(def) => {
                    known_roots.insert(def.node.name.node.name.clone());
                }
                Node::TypeDefinition(def) => {
                    known_roots.insert(def.node.name.node.name.clone());
                }
                Node::EnumDefinition(def) => {
                    known_roots.insert(def.node.name.node.name.clone());
                }
                Node::ContractDefinition(def) => {
                    known_roots.insert(def.node.name.node.name.clone());
                }
                _ => {}
            }
        }

        for item in &program.node.items {
            let Node::UseDeclaration(use_decl) = &item.node else {
                continue;
            };
            let Some(root) = use_decl.node.path.node.segments.first() else {
                continue;
            };
            if known_roots.contains(&root.node.name.node.name) {
                continue;
            }

            ctx.emit_issue(
                use_decl.node.path.span,
                SemanticIssueKind::UnknownImportPath { path: self.path_to_string_local(&use_decl.node.path) },
            );
        }
    }

    fn check_use_before_declaration(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        let mut walker = AstWalker::new().with_visitor(Box::new(UseBeforeDeclVisitor::new(ctx)));

        for item in &program.node.items {
            match &item.node {
                Node::Function(definition) => {
                    walker.walk(NodeRef::from(&definition.node.body.node));
                }
                Node::Method(definition) => {
                    walker.walk(NodeRef::from(&definition.node.body.node));
                }
                _ => {}
            }
        }
    }

    fn path_tail_local(&self, path: &Spanned<Path>) -> String {
        path.node.segments.last().map(|segment| segment.node.name.node.name.clone()).unwrap_or_default()
    }

    fn path_to_string_local(&self, path: &Spanned<Path>) -> String {
        path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>().join(".")
    }

    fn imported_name_local(&self, use_decl: &UseDeclaration) -> String {
        use_decl
            .alias
            .as_ref()
            .map(|alias| alias.node.name.clone())
            .unwrap_or_else(|| self.path_tail_local(&use_decl.path))
    }
}

struct DeclFrame {
    pending: HashSet<String>,
    start_declared_len: usize,
}

struct UseBeforeDeclVisitor<'a> {
    ctx: &'a mut RuleContext,
    declared_stack: Vec<String>,
    block_frames: Vec<DeclFrame>,
    kind_stack: Vec<NodeKind>,
    for_iterators: Vec<String>,
}

impl<'a> UseBeforeDeclVisitor<'a> {
    fn new(ctx: &'a mut RuleContext) -> Self {
        Self {
            ctx,
            declared_stack: Vec::new(),
            block_frames: Vec::new(),
            kind_stack: Vec::new(),
            for_iterators: Vec::new(),
        }
    }
}

impl Visit for UseBeforeDeclVisitor<'_> {
    fn enter(&mut self, node: NodeRef<'_>) {
        let parent = self.kind_stack.last().copied();

        if let Some(for_statement) = node.of::<ForStatement>() {
            self.for_iterators.push(for_statement.iterator.node.name.clone());
        }

        if let Some(block) = node.of::<Block>() {
            let pending = block
                .statements
                .iter()
                .filter_map(|statement| match &statement.node {
                    Statement::Let(let_statement) => Some(let_statement.node.name.node.name.clone()),
                    _ => None,
                })
                .collect::<HashSet<_>>();

            let start_declared_len = self.declared_stack.len();
            self.block_frames.push(DeclFrame { pending, start_declared_len });

            if parent == Some(NodeKind::ForStatement)
                && let Some(iterator_name) = self.for_iterators.last().cloned()
            {
                self.declared_stack.push(iterator_name);
            }
        }

        if let Some(expression) = node.of::<Expression>()
            && let Expression::Path(path_expr) = expression
            && path_expr.node.path.node.segments.len() == 1
            && let Some(name) = path_expr.node.path.node.segments.first()
        {
            let name_value = &name.node.name.node.name;
            if let Some(frame) = self.block_frames.last()
                && !self.declared_stack.iter().any(|declared| declared == name_value)
                && frame.pending.contains(name_value)
            {
                self.ctx.emit_issue(
                    path_expr.node.path.span,
                    SemanticIssueKind::UseBeforeDeclaration { name: name_value.clone() },
                );
            }
        }

        self.kind_stack.push(node.node_kind());
    }

    fn exit(&mut self, node: NodeRef<'_>) {
        if let Some(let_statement) = node.of::<LetStatement>() {
            let name = let_statement.name.node.name.clone();
            if let Some(frame) = self.block_frames.last_mut() {
                frame.pending.remove(&name);
            }
            self.declared_stack.push(name);
        }

        if node.of::<Block>().is_some()
            && let Some(frame) = self.block_frames.pop()
        {
            self.declared_stack.truncate(frame.start_declared_len);
        }

        if node.of::<ForStatement>().is_some() {
            self.for_iterators.pop();
        }

        self.kind_stack.pop();
    }
}
