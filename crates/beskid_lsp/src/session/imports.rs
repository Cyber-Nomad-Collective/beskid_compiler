use std::collections::HashMap;

use beskid_analysis::syntax::SpanInfo;

/// Recoverable current-buffer facts needed to bound imported-member completion.
pub(crate) struct RecoverableCompletionSyntax {
    imported_bindings: HashMap<String, Vec<String>>,
    path_expression_spans: Vec<SpanInfo>,
}

/// Placeholder member written after a dangling `.` (see [`completion_repaired_text`]).
const PLACEHOLDER_MEMBER: &[u8] = b"x;";

/// The buffer with every dangling member access completed, for completion only, or `None` when
/// it has none. A dangling member access is a `receiver.` or `receiver.Partial` that ends its
/// line (or the buffer) outside a comment or string: the state of a statement while its member
/// is being typed (`String.` above `return 0;`). Without repair the parser reads the next line
/// as the member (`String.return`) and the receiver is lost. The repair writes a placeholder
/// member and `;` (`x;`), or only `;` after a partial member, over the whitespace that follows,
/// so every byte offset of the buffer is unchanged and the completion context of the original
/// buffer applies to the repaired one. At the end of the buffer it is appended.
/// Returns the repaired buffer and the offset just after each repaired dot.
pub(crate) fn completion_repaired_text(text: &str) -> Option<(String, Vec<usize>)> {
    let mut repaired = text.as_bytes().to_vec();
    let mut appended = Vec::new();
    let mut offsets = Vec::new();
    for (dot, _) in text.match_indices('.') {
        let line_start = text[..dot].rfind('\n').map_or(0, |newline| newline + 1);
        let line_before = &text[line_start..dot];
        if line_before.contains("//") || line_before.matches('"').count() % 2 == 1 {
            continue;
        }
        let receiver = line_before
            .rsplit(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == ')' || ch == ']'))
            .next()
            .unwrap_or_default();
        if receiver.is_empty() || receiver.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        // The member typed so far (possibly none), then nothing else on the line.
        let after = &text[dot + 1..];
        let member = after.len() - after.trim_start_matches(|ch: char| ch.is_alphanumeric() || ch == '_').len();
        let rest = &after[member..];
        let line_rest = &rest[..rest.find('\n').unwrap_or(rest.len())];
        if !line_rest.trim().is_empty() {
            continue;
        }
        let whitespace = rest.len() - rest.trim_start().len();
        let at = dot + 1 + member;
        let fill: &[u8] = if member == 0 { PLACEHOLDER_MEMBER } else { b";" };
        if rest.is_empty() {
            appended.extend_from_slice(fill);
        } else if whitespace >= fill.len() {
            repaired[at..at + fill.len()].copy_from_slice(fill);
        } else if whitespace >= 1 {
            repaired[at] = b';';
        } else {
            continue;
        }
        let start = dot + 1;
        offsets.push(start);
    }
    repaired.extend_from_slice(&appended);
    (!offsets.is_empty())
        .then(|| (String::from_utf8(repaired).expect("ASCII written over ASCII whitespace"), offsets))
}

impl RecoverableCompletionSyntax {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let parse =
            |text: &str| beskid_analysis::services::parse_program_with_source_name("completion buffer", text).ok();
        let program =
            completion_repaired_text(text).and_then(|(repaired, _)| parse(&repaired)).or_else(|| parse(text))?;
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

    use super::{RecoverableCompletionSyntax, completion_repaired_text};

    #[test]
    fn a_dangling_member_access_is_repaired_without_moving_any_offset() {
        let text = "use Core.String;\ni32 Main() {\n    String.\n    point.\n    return 0; // end.\n}\nstring s = \"a.\";\nf64 n = 1.\n";
        let (repaired, offsets) = completion_repaired_text(text).expect("dangling accesses");
        assert_eq!(offsets, [text.find("String.\n").unwrap() + 7, text.find("point.").unwrap() + 6]);
        assert_eq!(repaired.len(), text.len());
        assert!(repaired.contains("String.x;   point.x;   return 0; // end.\n"), "{repaired}");
        assert!(repaired.ends_with("\"a.\";\nf64 n = 1.\n"), "comments, strings and numbers stay: {repaired}");
        assert_eq!(completion_repaired_text("i32 Main() { return 0; }"), None);
        let (partial, offsets) = completion_repaired_text("i32 Main() {\n    String.Le\n    return 0;\n}\n").unwrap();
        assert_eq!(partial, "i32 Main() {\n    String.Le;    return 0;\n}\n");
        assert_eq!(offsets, [24]);
        assert_eq!(completion_repaired_text("use Core.String;\nString."), Some(("use Core.String;\nString.x;".into(), vec![24])));
        let text = "use Core.String;\ni32 Main() {\n    String.\n    return 0;\n}\n";
        let syntax = RecoverableCompletionSyntax::parse(text).expect("the repaired buffer parses");
        assert!(syntax.imports("String", &["Core".into(), "String".into()]));
        let receiver = text.find("String.\n").unwrap();
        assert!(syntax.contains_member_access(receiver, receiver + "String.".len()));
    }

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
