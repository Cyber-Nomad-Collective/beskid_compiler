use std::borrow::ToOwned;

use crate::syntax::{Expression, Literal, Node, Program, Spanned, TestDefinition};

use super::model::{AnalysisSymbolKind, DocumentAnalysisSnapshot, DocumentSymbolInfo, TestCaseInfo};

pub fn collect_test_cases(program: &Spanned<Program>) -> Vec<TestCaseInfo> {
    let mut out = Vec::new();
    for item in &program.node.items {
        collect_test_cases_from_node(item, &mut Vec::new(), &mut out);
    }
    out
}

fn collect_test_cases_from_node(item: &Spanned<Node>, module_path: &mut Vec<String>, out: &mut Vec<TestCaseInfo>) {
    match &item.node {
        Node::TestDefinition(definition) => out.push(test_case_info(definition, module_path)),
        Node::InlineModule(module) => {
            module_path.push(module.node.name.node.name.clone());
            for nested in &module.node.items {
                collect_test_cases_from_node(nested, module_path, out);
            }
            module_path.pop();
        }
        _ => {}
    }
}

fn test_case_info(definition: &Spanned<TestDefinition>, module_path: &[String]) -> TestCaseInfo {
    let name = definition.node.name.node.name.clone();
    let qualified_name =
        if module_path.is_empty() { name.clone() } else { format!("{}::{}", module_path.join("::"), name) };
    let mut tags = Vec::new();
    let mut group = None;
    if let Some(meta) = &definition.node.meta {
        for entry in &meta.node.entries {
            let key = entry.node.name.node.name.as_str();
            if key == "group" {
                group = literal_string(&entry.node.value);
            } else if key == "tags" {
                tags = literal_tags(&entry.node.value);
            }
        }
    }
    let mut skip_condition = None;
    let mut skip_reason = None;
    if let Some(skip) = &definition.node.skip {
        for entry in &skip.node.entries {
            let key = entry.node.name.node.name.as_str();
            if key == "condition" {
                skip_condition = literal_bool(&entry.node.value);
            } else if key == "reason" {
                skip_reason = literal_string(&entry.node.value);
            }
        }
    }
    let (definition_line, definition_column) = definition.node.name.span.line_col_start;
    TestCaseInfo {
        name,
        qualified_name,
        tags,
        group,
        skip_condition,
        skip_reason,
        selection_start: definition.node.name.span.start,
        selection_end: definition.node.name.span.end,
        definition_line,
        definition_column,
    }
}

fn literal_string(expression: &Spanned<Expression>) -> Option<String> {
    let Expression::Literal(literal) = &expression.node else {
        return None;
    };
    crate::syntax::expressions::try_decode_string_literal(&literal.node.literal.node)
}

fn literal_tags(expression: &Spanned<Expression>) -> Vec<String> {
    literal_string(expression)
        .map(|value| value.split(',').map(str::trim).filter(|token| !token.is_empty()).map(ToOwned::to_owned).collect())
        .unwrap_or_default()
}

fn literal_bool(expression: &Spanned<Expression>) -> Option<bool> {
    let Expression::Literal(literal) = &expression.node else {
        return None;
    };
    let Literal::Bool(value) = &literal.node.literal.node else {
        return None;
    };
    Some(*value)
}

pub fn collect_document_symbols(snapshot: &DocumentAnalysisSnapshot) -> Vec<DocumentSymbolInfo> {
    snapshot
        .program
        .node
        .items
        .iter()
        .filter_map(|item| match &item.node {
            Node::ConstantDefinition(definition) => Some(DocumentSymbolInfo {
                name: definition.node.name.node.name.clone(),
                kind: AnalysisSymbolKind::Constant,
                selection_start: definition.node.name.span.start,
                selection_end: definition.node.name.span.end,
            }),
            Node::Function(definition) => Some(DocumentSymbolInfo {
                name: definition.node.name.node.name.clone(),
                kind: AnalysisSymbolKind::Function,
                selection_start: definition.node.name.span.start,
                selection_end: definition.node.name.span.end,
            }),
            Node::Method(definition) => Some(DocumentSymbolInfo {
                name: definition.node.name.node.name.clone(),
                kind: AnalysisSymbolKind::Method,
                selection_start: definition.node.name.span.start,
                selection_end: definition.node.name.span.end,
            }),
            Node::ExtendTypeDefinition(_) => None,
            Node::TestDefinition(definition) => Some(DocumentSymbolInfo {
                name: definition.node.name.node.name.clone(),
                kind: AnalysisSymbolKind::Test,
                selection_start: definition.node.name.span.start,
                selection_end: definition.node.name.span.end,
            }),
            Node::TypeDefinition(definition) => Some(DocumentSymbolInfo {
                name: definition.node.name.node.name.clone(),
                kind: AnalysisSymbolKind::Type,
                selection_start: definition.node.name.span.start,
                selection_end: definition.node.name.span.end,
            }),
            Node::EnumDefinition(definition) => Some(DocumentSymbolInfo {
                name: definition.node.name.node.name.clone(),
                kind: AnalysisSymbolKind::Enum,
                selection_start: definition.node.name.span.start,
                selection_end: definition.node.name.span.end,
            }),
            Node::ContractDefinition(definition) => Some(DocumentSymbolInfo {
                name: definition.node.name.node.name.clone(),
                kind: AnalysisSymbolKind::Contract,
                selection_start: definition.node.name.span.start,
                selection_end: definition.node.name.span.end,
            }),
            Node::AttributeDeclaration(_) => None,
            Node::ModuleDeclaration(definition) => {
                let segment = definition.node.path.node.segments.last()?;
                Some(DocumentSymbolInfo {
                    name: segment.node.name.node.name.clone(),
                    kind: AnalysisSymbolKind::Module,
                    selection_start: segment.span.start,
                    selection_end: segment.span.end,
                })
            }
            Node::InlineModule(definition) => Some(DocumentSymbolInfo {
                name: definition.node.name.node.name.clone(),
                kind: AnalysisSymbolKind::Module,
                selection_start: definition.node.name.span.start,
                selection_end: definition.node.name.span.end,
            }),
            Node::MacroDefinition(_) => None,
            Node::HostDefinition(_) => None,
            Node::UseDeclaration(definition) => {
                if let Some(alias) = &definition.node.alias {
                    return Some(DocumentSymbolInfo {
                        name: alias.node.name.clone(),
                        kind: AnalysisSymbolKind::Use,
                        selection_start: alias.span.start,
                        selection_end: alias.span.end,
                    });
                }
                let segment = definition.node.path.node.segments.last()?;
                Some(DocumentSymbolInfo {
                    name: segment.node.name.node.name.clone(),
                    kind: AnalysisSymbolKind::Use,
                    selection_start: segment.span.start,
                    selection_end: segment.span.end,
                })
            }
        })
        .collect()
}
