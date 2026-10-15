//! Attribute completion: inside `[` at a declaration site, offer the attributes whose target
//! list admits that site; inside an `AttributeTargetList` (`attribute Name(|)`), offer the
//! target catalogue.
//!
//! The site is found by parsing the buffer with a placeholder attribute name at the cursor and
//! reading the parent node of the placeholder attribute, so the grammar (not a text heuristic)
//! decides which catalogue target the site is.

use std::sync::Arc;

use beskid_analysis::attribute_targets::{ATTRIBUTE_TARGET_CATALOGUE, attribute_site_target};
use beskid_analysis::syntax::{Attribute, AttributeDeclaration, SyntaxGenerationId};
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::CompletionContext;
use tower_lsp_server::ls_types::{CompletionItem, CompletionItemKind, CompletionTextEdit, TextEdit};

use crate::position::offset_range_to_lsp;
use crate::session::store::AttributeCompletionEntry;

const PLACEHOLDER: &str = "BeskidCompletionAttributePlaceholder";

/// Completion items when the cursor is in an attribute list or an attribute target list, or
/// `None` when it is in neither.
pub(crate) fn attribute_completion_items(
    text: &str,
    context: CompletionContext,
    catalogue: &[AttributeCompletionEntry],
) -> Option<Vec<CompletionItem>> {
    let prefix = text.get(context.replacement_start..context.cursor)?;
    if in_attribute_target_list(text, context.replacement_start) {
        let items = ATTRIBUTE_TARGET_CATALOGUE
            .iter()
            .enumerate()
            .filter(|(_, target)| target.starts_with(prefix))
            .map(|(position, target)| CompletionItem {
                label: (*target).to_string(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some("attribute target".to_string()),
                sort_text: Some(format!("{position:02}")),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: offset_range_to_lsp(text, context.replacement_start, context.replacement_end),
                    new_text: (*target).to_string(),
                })),
                ..CompletionItem::default()
            })
            .collect();
        return Some(items);
    }
    let (site, buffer_declarations) = attribute_site_at(text, context)?;
    let mut entries: Vec<AttributeCompletionEntry> = buffer_declarations;
    for entry in catalogue {
        if !entries.iter().any(|known| known.name == entry.name) {
            entries.push(entry.clone());
        }
    }
    for recognized in beskid_analysis::attribute_targets::COMPILER_RECOGNIZED_ATTRIBUTES {
        if !entries.iter().any(|known| known.name.as_ref() == recognized.name) {
            entries.push(AttributeCompletionEntry {
                name: Arc::from(recognized.name),
                targets: recognized.targets.iter().map(|target| Arc::from(*target)).collect(),
                recognized: true,
            });
        }
    }
    let mut items = entries
        .into_iter()
        .filter(|entry| entry.name.starts_with(prefix))
        .filter(|entry| entry.targets.iter().any(|target| target.as_ref() == site))
        .map(|entry| {
            let targets = entry.targets.iter().map(AsRef::as_ref).collect::<Vec<_>>().join(", ");
            CompletionItem {
                label: entry.name.to_string(),
                kind: Some(CompletionItemKind::CLASS),
                detail: Some(format!("attribute ({targets})")),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: offset_range_to_lsp(text, context.replacement_start, context.replacement_end),
                    new_text: entry.name.to_string(),
                })),
                ..CompletionItem::default()
            }
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| left.label.cmp(&right.label));
    Some(items)
}

/// Whether `offset` is inside `attribute Name( ... )`, after `(` or a `,` of the target list.
fn in_attribute_target_list(text: &str, offset: usize) -> bool {
    let before = &text[..offset];
    let is_ident = |ch: char| ch.is_alphanumeric() || ch == '_';
    // Walk back over earlier targets (`Ident ,`) to the opening parenthesis.
    let mut rest = before.trim_end();
    loop {
        if let Some(stripped) = rest.strip_suffix('(') {
            rest = stripped.trim_end();
            break;
        }
        let Some(stripped) = rest.strip_suffix(',') else {
            return false;
        };
        let stripped = stripped.trim_end();
        let without_ident = stripped.trim_end_matches(is_ident);
        if without_ident.len() == stripped.len() {
            return false;
        }
        rest = without_ident.trim_end();
    }
    let without_name = rest.trim_end_matches(is_ident);
    if without_name.len() == rest.len() {
        return false;
    }
    let before_name = without_name.trim_end();
    if before_name.len() == without_name.len() {
        return false;
    }
    let Some(head) = before_name.strip_suffix("attribute") else {
        return false;
    };
    head.chars().next_back().is_none_or(|ch| !is_ident(ch))
}

/// The catalogue target of the declaration site whose attribute list holds the cursor, plus the
/// attribute declarations of the current buffer (they win over every other declaration).
fn attribute_site_at(text: &str, context: CompletionContext) -> Option<(&'static str, Vec<AttributeCompletionEntry>)> {
    let head = &text[..context.replacement_start];
    let tail = &text[context.replacement_end..];
    if !matches!(head.trim_end().chars().next_back(), Some('[' | ',')) {
        return None;
    }
    let closes = matches!(tail.trim_start().chars().next(), Some(']' | ',' | '('));
    let mut candidates = vec![format!("{head}{PLACEHOLDER}{tail}")];
    if !closes {
        candidates.push(format!("{head}{PLACEHOLDER}]{tail}"));
    }
    for candidate in candidates {
        let Ok(program) = beskid_analysis::services::parse_program_with_source_name("<completion>", &candidate) else {
            continue;
        };
        let index = SyntaxIndex::from_program(&program, SyntaxGenerationId(0));
        let site = index.ids_of_kind(NodeKind::Attribute).find_map(|node| {
            let attribute = index.node_at(&program, node)?.of::<Attribute>()?;
            if attribute.name.node.name != PLACEHOLDER || attribute.name.span.start != context.replacement_start {
                return None;
            }
            let parent = index.metadata().get(node.0 as usize)?.parent?;
            attribute_site_target(index.kind(parent)?)
        });
        let Some(site) = site else {
            continue;
        };
        let declarations = index
            .ids_of_kind(NodeKind::AttributeDeclaration)
            .filter_map(|node| index.node_at(&program, node)?.of::<AttributeDeclaration>())
            .map(|declaration| AttributeCompletionEntry {
                name: Arc::from(declaration.name.node.name.as_str()),
                targets: declaration
                    .targets
                    .iter()
                    .map(|target| Arc::from(target.node.name.node.name.as_str()))
                    .collect(),
                recognized: false,
            })
            .fold(Vec::<AttributeCompletionEntry>::new(), |mut entries, entry| {
                match entries.iter_mut().find(|known| known.name == entry.name) {
                    Some(known) => {
                        let mut targets = known.targets.to_vec();
                        for target in entry.targets.iter() {
                            if !targets.contains(target) {
                                targets.push(target.clone());
                            }
                        }
                        known.targets = targets.into();
                    }
                    None => entries.push(entry),
                }
                entries
            });
        return Some((site, declarations));
    }
    None
}

#[cfg(test)]
mod tests {
    use beskid_queries::CompletionContext;

    use super::{attribute_completion_items, in_attribute_target_list};

    fn context_at(text: &str, offset: usize) -> CompletionContext {
        let is_ident = |ch: char| ch.is_alphanumeric() || ch == '_';
        let start = offset - text[..offset].chars().rev().take_while(|ch| is_ident(*ch)).count();
        let end = offset + text[offset..].chars().take_while(|ch| is_ident(*ch)).count();
        CompletionContext { cursor: offset, replacement_start: start, replacement_end: end }
    }

    fn labels(text: &str, offset: usize) -> Option<Vec<String>> {
        attribute_completion_items(text, context_at(text, offset), &[])
            .map(|items| items.into_iter().map(|item| item.label).collect())
    }

    const DECLARATIONS: &str = "pub attribute Trace(Parameter) { }\npub attribute Pure(LambdaExpression) { }\n";

    #[test]
    fn completion_before_a_lambda_offers_only_lambda_attributes() {
        let source = format!("{DECLARATIONS}i64 Run() {{\n    let square = [] (i64 x) => x * x;\n    return 0;\n}}\n");
        let offset = source.find("[]").expect("attribute list") + 1;
        let labels = labels(&source, offset).expect("attribute list completion");
        assert!(labels.contains(&"Pure".to_string()), "{labels:?}");
        assert!(!labels.contains(&"Trace".to_string()), "{labels:?}");
    }

    #[test]
    fn completion_with_a_typed_prefix_before_a_lambda_offers_pure() {
        let source =
            format!("{DECLARATIONS}i64 Run() {{\n    let square = [Pu] (i64 x) => x * x;\n    return 0;\n}}\n");
        let offset = source.find("[Pu").expect("attribute list") + 3;
        assert_eq!(labels(&source, offset).expect("attribute list completion"), vec!["Pure".to_string()]);
    }

    #[test]
    fn completion_on_a_parameter_offers_parameter_attributes() {
        let source = format!("{DECLARATIONS}i64 Add([] i64 left, i64 right) {{ return left + right; }}\n");
        let offset = source.find("[]").expect("attribute list") + 1;
        let labels = labels(&source, offset).expect("attribute list completion");
        assert!(labels.contains(&"Trace".to_string()), "{labels:?}");
        assert!(labels.contains(&"Captures".to_string()), "recognized Parameter attribute: {labels:?}");
        assert!(!labels.contains(&"Pure".to_string()), "{labels:?}");
    }

    #[test]
    fn completion_on_a_type_offers_recognized_type_attributes() {
        let source = "[Imm]\npub type Job { string id, i64 size }\n";
        let offset = source.find("Imm").expect("attribute") + 3;
        let labels = labels(source, offset).expect("attribute list completion");
        assert!(labels.contains(&"Immutable".to_string()), "{labels:?}");
        assert!(!labels.contains(&"ImmutableStorage".to_string()), "Field-only attribute: {labels:?}");
    }

    #[test]
    fn index_expressions_are_not_attribute_lists() {
        let source = "i64 Run(i64[] values) { return values[0]; }\n";
        let offset = source.find("[0]").expect("index") + 1;
        assert!(labels(source, offset).is_none_or(|labels| labels.is_empty()));
    }

    #[test]
    fn target_list_completion_offers_the_catalogue_in_order() {
        let source = "pub attribute Pure() { }\n";
        let offset = source.find("()").expect("target list") + 1;
        let offered = labels(source, offset).expect("target list completion");
        let catalogue = beskid_analysis::attribute_targets::ATTRIBUTE_TARGET_CATALOGUE
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        assert_eq!(offered, catalogue);

        let source = "pub attribute Pure(Parameter, Lam) { }\n";
        let offset = source.find("Lam").expect("prefix") + 3;
        assert_eq!(
            labels(source, offset).expect("target list completion"),
            vec!["LambdaExpression".to_string(), "LambdaParameter".to_string()]
        );
    }

    #[test]
    fn ordinary_calls_are_not_target_lists() {
        assert!(!in_attribute_target_list("i64 x = Run(", 12));
        assert!(!in_attribute_target_list("let attributes = Run(", 21));
        assert!(in_attribute_target_list("pub attribute Pure(", 19));
        assert!(in_attribute_target_list("attribute Pure(Parameter, ", 26));
    }
}
