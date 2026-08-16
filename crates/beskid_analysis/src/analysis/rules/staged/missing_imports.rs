use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::rules::RuleContext;
use crate::resolve::Resolution;
use crate::syntax::{Expression, Node, Program, SpanInfo, Spanned};
use crate::syntax_query::{AstWalker, NodeRef, Visit};
use std::collections::HashSet;

impl SemanticPipelineRule {
    /// Suggest `use` imports for unresolved names that a known assembly module exports.
    ///
    /// Runs after name resolution. For each single-segment value path that the resolver
    /// could not bind, check whether any known assembly module path ends with `::Name`.
    /// If exactly one module exports the name, emit [`SemanticIssueKind::MissingImport`];
    /// if multiple do, emit [`SemanticIssueKind::MissingImportAmbiguous`].
    pub(super) fn stage_missing_imports(
        &self,
        ctx: &mut RuleContext,
        program: &Spanned<Program>,
        resolution: &Resolution,
    ) {
        // Only run when we have assembly module paths to suggest from.
        let Some(known_paths) = ctx.options.known_assembly_module_paths.clone() else {
            return;
        };

        let source_path = ctx.options.entry_source_path.clone();
        for item in &program.node.items {
            match &item.node {
                Node::Function(definition) => {
                    let visitor = MissingImportVisitor::new(ctx, resolution, &known_paths, source_path.as_ref());
                    let mut walker = AstWalker::new().with_visitor(Box::new(visitor));
                    walker.walk(NodeRef::from(&definition.node.body.node));
                }
                Node::Method(definition) => {
                    let visitor = MissingImportVisitor::new(ctx, resolution, &known_paths, source_path.as_ref());
                    let mut walker = AstWalker::new().with_visitor(Box::new(visitor));
                    walker.walk(NodeRef::from(&definition.node.body.node));
                }
                Node::ExtendTypeDefinition(definition) => {
                    for method in &definition.node.methods {
                        let visitor = MissingImportVisitor::new(ctx, resolution, &known_paths, source_path.as_ref());
                        let mut walker = AstWalker::new().with_visitor(Box::new(visitor));
                        walker.walk(NodeRef::from(&method.node.body.node));
                    }
                }
                Node::TestDefinition(definition) => {
                    let visitor = MissingImportVisitor::new(ctx, resolution, &known_paths, source_path.as_ref());
                    let mut walker = AstWalker::new().with_visitor(Box::new(visitor));
                    // `test` items have a flat statement list (no wrapping `Block`), so walk each
                    // statement individually; the walker recurses into their expressions.
                    for statement in &definition.node.statements {
                        walker.walk(NodeRef::from(&statement.node));
                    }
                }
                _ => {}
            }
        }
    }
}

/// Walks callable bodies and emits `MissingImport` / `MissingImportAmbiguous` suggestions for
/// single-segment value paths that the resolver could not bind but a known module exports.
struct MissingImportVisitor<'a> {
    ctx: &'a mut RuleContext,
    resolution: &'a Resolution,
    known_paths: &'a HashSet<String>,
    source_path: Option<&'a std::path::PathBuf>,
}

impl<'a> MissingImportVisitor<'a> {
    fn new(
        ctx: &'a mut RuleContext,
        resolution: &'a Resolution,
        known_paths: &'a HashSet<String>,
        source_path: Option<&'a std::path::PathBuf>,
    ) -> Self {
        Self { ctx, resolution, known_paths, source_path }
    }

    fn check_path(&mut self, span: SpanInfo, name: &str) {
        // Skip names the resolver successfully bound at this span.
        if self.resolution.tables.resolved_value_at(span, self.source_path).is_some() {
            return;
        }

        let candidates = modules_exporting(name, self.known_paths);
        match candidates.len() {
            0 => {}
            1 => {
                let module_path = candidates.into_iter().next().expect("exactly one candidate");
                self.ctx.emit_issue(span, SemanticIssueKind::MissingImport { name: name.to_string(), module_path });
            }
            _ => {
                self.ctx
                    .emit_issue(span, SemanticIssueKind::MissingImportAmbiguous { name: name.to_string(), candidates });
            }
        }
    }
}

impl Visit for MissingImportVisitor<'_> {
    fn enter(&mut self, node: NodeRef<'_>) {
        let Some(expression) = node.of::<Expression>() else { return };
        let Expression::Path(path_expr) = expression else { return };
        // Only single-segment paths can be bare unresolved names; qualified paths are
        // already module-scoped and handled by the resolver's module-path errors.
        if path_expr.node.path.node.segments.len() != 1 {
            return;
        }
        let Some(segment) = path_expr.node.path.node.segments.first() else {
            return;
        };
        let name = &segment.node.name.node.name;
        self.check_path(path_expr.node.path.span, name);
    }
}

/// Return the known assembly module paths that end with `::name` (i.e. export `name`).
fn modules_exporting(name: &str, known_paths: &HashSet<String>) -> Vec<String> {
    let suffix = format!("::{name}");
    known_paths.iter().filter(|path| path.ends_with(&suffix) || path.as_str() == name).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn modules_exporting_matches_suffix() {
        let mut paths = HashSet::new();
        paths.insert("Std::System::Console".to_string());
        paths.insert("Std::IO::Console".to_string());
        paths.insert("Std::System::Logger".to_string());

        let console = modules_exporting("Console", &paths);
        assert_eq!(console.len(), 2);
        assert!(console.contains(&"Std::System::Console".to_string()));
        assert!(console.contains(&"Std::IO::Console".to_string()));

        let logger = modules_exporting("Logger", &paths);
        assert_eq!(logger, vec!["Std::System::Logger".to_string()]);

        let missing = modules_exporting("Absent", &paths);
        assert!(missing.is_empty());
    }

    #[test]
    fn modules_exporting_matches_exact_name() {
        let mut paths = HashSet::new();
        paths.insert("Console".to_string());
        let result = modules_exporting("Console", &paths);
        assert_eq!(result, vec!["Console".to_string()]);
    }
}
