use std::collections::HashMap;

use beskid_analysis::syntax::SpanInfo;

/// Recoverable current-buffer facts needed to bound imported-member completion.
pub(crate) struct RecoverableCompletionSyntax {
    imported_bindings: HashMap<String, Vec<String>>,
    path_expression_spans: Vec<SpanInfo>,
}

impl RecoverableCompletionSyntax {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let program = beskid_analysis::services::parse_program_with_source_name("completion buffer", text).ok()?;
        let imported_bindings = program
            .node
            .items
            .iter()
            .filter_map(|item| {
                let beskid_analysis::syntax::Node::UseDeclaration(declaration) = &item.node else {
                    return None;
                };
                let path = declaration
                    .node
                    .path
                    .node
                    .segments
                    .iter()
                    .map(|segment| segment.node.name.node.name.clone())
                    .collect::<Vec<_>>();
                let binding = declaration
                    .node
                    .alias
                    .as_ref()
                    .map(|alias| alias.node.name.clone())
                    .or_else(|| path.last().cloned())?;
                Some((binding, path))
            })
            .collect();
        let index = beskid_analysis::syntax_query::SyntaxIndex::from_program(
            &program,
            beskid_analysis::syntax::SyntaxGenerationId(0),
        );
        let path_expression_spans = index
            .metadata()
            .iter()
            .filter(|metadata| metadata.kind == beskid_analysis::syntax_query::NodeKind::PathExpression)
            .filter_map(|metadata| metadata.span)
            .collect();
        Some(Self { imported_bindings, path_expression_spans })
    }

    pub(crate) fn imports(&self, receiver: &str, import_path: &[std::sync::Arc<str>]) -> bool {
        self.imported_bindings
            .get(receiver)
            .is_some_and(|current| current.iter().map(String::as_str).eq(import_path.iter().map(AsRef::as_ref)))
    }

    pub(crate) fn contains_member_access(&self, receiver_start: usize, access_end: usize) -> bool {
        self.path_expression_spans.iter().any(|span| span.start <= receiver_start && span.end >= access_end)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::RecoverableCompletionSyntax;

    #[test]
    fn import_identity_includes_path_not_only_alias() {
        let syntax =
            RecoverableCompletionSyntax::parse("use New.Tools;\ni32 Main() { Tools.; }").expect("recoverable syntax");

        assert!(syntax.imports("Tools", &[Arc::from("New"), Arc::from("Tools")]));
        assert!(!syntax.imports("Tools", &[Arc::from("Old"), Arc::from("Tools")]));
    }

    #[test]
    fn aliased_import_identity_includes_retargeted_path() {
        let syntax = RecoverableCompletionSyntax::parse("use New.Tools as Api;\ni32 Main() { Api.; }")
            .expect("recoverable syntax");

        assert!(syntax.imports("Api", &[Arc::from("New"), Arc::from("Tools")]));
        assert!(!syntax.imports("Api", &[Arc::from("Old"), Arc::from("Tools")]));
    }
}
