//! Build a [`BsolDocument`] from pest pairs.

use pest::Parser;
use pest::iterators::Pair;

use crate::projects::error::ProjectError;

use super::ast::{
    BsolAssignment, BsolBlock, BsolBlockHeader, BsolBodyItem, BsolBracketList, BsolDocument,
    BsolListItem, BsolNestedBlock, BsolNestedBlockKind, BsolQuotedString, BsolReservedBlockKind,
    BsolSpan, BsolValue,
};
use super::parser::{BsolParser, Rule};

/// Parse manifest source into the normative Bsol AST.
pub fn parse_bsol_document(source: &str) -> Result<BsolDocument, ProjectError> {
    let mut pairs =
        BsolParser::parse(Rule::document, source).map_err(|err| pest_error(source, err))?;
    let document_pair = pairs.next().ok_or_else(|| {
        ProjectError::Parse("Bsol parse produced no document pair".to_string())
    })?;
    build_document(document_pair, source)
}

fn build_document(pair: Pair<Rule>, source: &str) -> Result<BsolDocument, ProjectError> {
    let mut blocks = Vec::new();
    for child in pair.into_inner() {
        if child.as_rule() == Rule::block {
            blocks.push(build_block(child, source)?);
        }
    }
    Ok(BsolDocument { blocks })
}

fn build_block(pair: Pair<Rule>, source: &str) -> Result<BsolBlock, ProjectError> {
    let span = BsolSpan::from_pest(pair.as_span(), source);
    let mut inner = pair.into_inner();
    let block_pair = inner
        .next()
        .ok_or_else(|| parse_at(span, "empty block"))?;
    match block_pair.as_rule() {
        Rule::project_root_block => build_project_root_block(span, block_pair, source),
        Rule::labeled_reserved_block => build_labeled_reserved_block(span, block_pair, source),
        Rule::unlabeled_reserved_block => build_unlabeled_reserved_block(span, block_pair, source),
        other => Err(parse_at(
            span,
            format!("unexpected block rule `{other:?}`"),
        )),
    }
}

fn build_project_root_block(
    span: BsolSpan,
    pair: Pair<Rule>,
    source: &str,
) -> Result<BsolBlock, ProjectError> {
    let mut inner = pair.into_inner();
    let ident_pair = inner
        .next()
        .ok_or_else(|| parse_at(span, "missing project root identifier"))?;
    let ident = ident_pair.as_str().to_string();
    let project_body = inner
        .next()
        .ok_or_else(|| parse_at(span, "missing project root body"))?;
    let body = build_project_body(project_body, source)?;
    Ok(BsolBlock {
        span,
        header: BsolBlockHeader::ProjectRoot { ident },
        body,
    })
}

fn build_labeled_reserved_block(
    span: BsolSpan,
    pair: Pair<Rule>,
    source: &str,
) -> Result<BsolBlock, ProjectError> {
    let mut inner = pair.into_inner();
    let kind_pair = inner
        .next()
        .ok_or_else(|| parse_at(span, "missing labeled block kind"))?;
    let kind = reserved_kind_from_str(kind_pair.as_str(), span)?;
    let mut label = None;
    let mut assignments_pair = None;
    for child in inner {
        match child.as_rule() {
            Rule::quoted_string => label = Some(build_quoted_string(child, source)),
            Rule::block_assignments => assignments_pair = Some(child),
            _ => {}
        }
    }
    let assignments_pair =
        assignments_pair.ok_or_else(|| parse_at(span, "missing labeled block assignments"))?;
    let body = build_flat_assignments(assignments_pair, source)?;
    Ok(BsolBlock {
        span,
        header: BsolBlockHeader::Reserved { kind, label },
        body,
    })
}

fn build_unlabeled_reserved_block(
    span: BsolSpan,
    pair: Pair<Rule>,
    source: &str,
) -> Result<BsolBlock, ProjectError> {
    let mut inner = pair.into_inner();
    let kind_pair = inner
        .next()
        .ok_or_else(|| parse_at(span, "missing unlabeled block kind"))?;
    let kind = reserved_kind_from_str(kind_pair.as_str(), span)?;
    let assignments_pair = inner
        .next()
        .ok_or_else(|| parse_at(span, "missing unlabeled block assignments"))?;
    let body = build_flat_assignments(assignments_pair, source)?;
    Ok(BsolBlock {
        span,
        header: BsolBlockHeader::Reserved {
            kind,
            label: None,
        },
        body,
    })
}

fn reserved_kind_from_str(text: &str, span: BsolSpan) -> Result<BsolReservedBlockKind, ProjectError> {
    match text {
        "target" => Ok(BsolReservedBlockKind::Target),
        "dependency" => Ok(BsolReservedBlockKind::Dependency),
        "link" => Ok(BsolReservedBlockKind::Link),
        "workspace" => Ok(BsolReservedBlockKind::Workspace),
        "member" => Ok(BsolReservedBlockKind::Member),
        "override" => Ok(BsolReservedBlockKind::Override),
        "registry" => Ok(BsolReservedBlockKind::Registry),
        other => Err(parse_at(span, format!("unknown reserved block kind `{other}`"))),
    }
}

fn build_project_body(pair: Pair<Rule>, source: &str) -> Result<Vec<BsolBodyItem>, ProjectError> {
    let mut items = Vec::new();
    for child in pair.into_inner() {
        let item_pair = if child.as_rule() == Rule::project_body_item {
            let span = BsolSpan::from_pest(child.as_span(), source);
            child
                .into_inner()
                .next()
                .ok_or_else(|| parse_at(span, "empty project body item"))?
        } else {
            child
        };
        match item_pair.as_rule() {
            Rule::block_assignment => {
                items.push(BsolBodyItem::Assignment(build_assignment(item_pair, source)?));
            }
            Rule::mod_block => {
                items.push(BsolBodyItem::NestedBlock(build_mod_block(item_pair, source)?));
            }
            Rule::template_block => {
                items.push(BsolBodyItem::NestedBlock(build_template_block(
                    item_pair, source,
                )?));
            }
            other => {
                return Err(parse_at(
                    BsolSpan::from_pest(item_pair.as_span(), source),
                    format!("unexpected project body item `{other:?}`"),
                ));
            }
        }
    }
    Ok(items)
}

fn build_mod_block(pair: Pair<Rule>, source: &str) -> Result<BsolNestedBlock, ProjectError> {
    let span = BsolSpan::from_pest(pair.as_span(), source);
    let mut inner = pair.into_inner();
    let keyword = inner
        .next()
        .ok_or_else(|| parse_at(span, "missing mod block keyword"))?;
    let kind = match keyword.as_str() {
        "mod" => BsolNestedBlockKind::Mod,
        "meta" => BsolNestedBlockKind::Meta,
        other => {
            return Err(parse_at(
                span,
                format!("unknown mod block keyword `{other}`"),
            ));
        }
    };
    let assignments_pair = inner
        .next()
        .ok_or_else(|| parse_at(span, "missing mod block assignments"))?;
    let assignments = build_mod_assignments(assignments_pair, source)?;
    Ok(BsolNestedBlock {
        span,
        kind,
        assignments,
    })
}

fn build_template_block(pair: Pair<Rule>, source: &str) -> Result<BsolNestedBlock, ProjectError> {
    let span = BsolSpan::from_pest(pair.as_span(), source);
    let mut inner = pair.into_inner();
    let assignments_pair = inner
        .next()
        .ok_or_else(|| parse_at(span, "missing template block assignments"))?;
    let assignments = build_template_assignments(assignments_pair, source)?;
    Ok(BsolNestedBlock {
        span,
        kind: BsolNestedBlockKind::Template,
        assignments,
    })
}

fn build_flat_assignments(
    pair: Pair<Rule>,
    source: &str,
) -> Result<Vec<BsolBodyItem>, ProjectError> {
    let mut items = Vec::new();
    for child in pair.into_inner() {
        if child.as_rule() == Rule::block_assignment {
            items.push(BsolBodyItem::Assignment(build_assignment(child, source)?));
        }
    }
    Ok(items)
}

fn build_mod_assignments(
    pair: Pair<Rule>,
    source: &str,
) -> Result<Vec<BsolAssignment>, ProjectError> {
    build_named_assignments(pair, Rule::mod_assignment, source)
}

fn build_template_assignments(
    pair: Pair<Rule>,
    source: &str,
) -> Result<Vec<BsolAssignment>, ProjectError> {
    build_named_assignments(pair, Rule::template_assignment, source)
}

fn build_named_assignments(
    pair: Pair<Rule>,
    assignment_rule: Rule,
    source: &str,
) -> Result<Vec<BsolAssignment>, ProjectError> {
    let mut assignments = Vec::new();
    for child in pair.into_inner() {
        if child.as_rule() == assignment_rule {
            assignments.push(build_assignment(child, source)?);
        }
    }
    Ok(assignments)
}

fn build_assignment(pair: Pair<Rule>, source: &str) -> Result<BsolAssignment, ProjectError> {
    let span = BsolSpan::from_pest(pair.as_span(), source);
    let mut inner = pair.into_inner();
    let key_pair = inner
        .next()
        .ok_or_else(|| parse_at(span, "missing assignment key"))?;
    let key = key_pair.as_str().to_string();
    let value_pair = inner
        .next()
        .ok_or_else(|| parse_at(span, "missing assignment value"))?;
    let value = build_value(value_pair, source)?;
    Ok(BsolAssignment { span, key, value })
}

fn build_value(pair: Pair<Rule>, source: &str) -> Result<BsolValue, ProjectError> {
    let inner = pair
        .into_inner()
        .next()
        .ok_or_else(|| ProjectError::Parse("empty Bsol value".to_string()))?;
    match inner.as_rule() {
        Rule::quoted_string => Ok(BsolValue::QuotedString(build_quoted_string(inner, source))),
        Rule::bare_token => {
            let text = inner.as_str();
            Ok(BsolValue::Ident(text.to_string()))
        }
        Rule::bracket_list => Ok(BsolValue::BracketList(build_bracket_list(inner, source)?)),
        other => Err(parse_at(
            BsolSpan::from_pest(inner.as_span(), source),
            format!("unexpected value rule `{other:?}`"),
        )),
    }
}

fn build_quoted_string(pair: Pair<Rule>, source: &str) -> BsolQuotedString {
    let span = BsolSpan::from_pest(pair.as_span(), source);
    BsolQuotedString::new(span, pair.as_str())
}

fn build_bracket_list(pair: Pair<Rule>, source: &str) -> Result<BsolBracketList, ProjectError> {
    let span = BsolSpan::from_pest(pair.as_span(), source);
    let mut items = Vec::new();
    for child in pair.into_inner() {
        match child.as_rule() {
            Rule::list_content => {
                for item in child.into_inner() {
                    items.push(build_list_item(item, source)?);
                }
            }
            Rule::list_item => items.push(build_list_item(child, source)?),
            _ => {}
        }
    }
    Ok(BsolBracketList { span, items })
}

fn build_list_item(pair: Pair<Rule>, source: &str) -> Result<BsolListItem, ProjectError> {
    let span = BsolSpan::from_pest(pair.as_span(), source);
    let text = pair.as_str().trim();
    if text == "default" {
        return Ok(BsolListItem::Default);
    }
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::quoted_string => {
                return Ok(BsolListItem::QuotedString(build_quoted_string(
                    inner, source,
                )));
            }
            Rule::ident => return Ok(BsolListItem::Ident(inner.as_str().to_string())),
            _ => {}
        }
    }
    Err(parse_at(
        span,
        format!("unexpected list item `{text}`"),
    ))
}

fn parse_at(span: BsolSpan, message: impl Into<String>) -> ProjectError {
    ProjectError::ParseAt {
        line: span.line,
        message: message.into(),
        start: Some(span.start),
        end: Some(span.end),
    }
}

fn pest_error(source: &str, err: pest::error::Error<Rule>) -> ProjectError {
    use pest::error::InputLocation;

    let start = match err.location {
        InputLocation::Pos(pos) => pos,
        InputLocation::Span((start, _)) => start,
    };
    let line = source[..start.min(source.len())].lines().count().max(1);
    ProjectError::ParseAt {
        line,
        message: err.to_string(),
        start: Some(start),
        end: source.get(start..).map(|tail| start + tail.len().min(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::bsol::BsolBlockHeader;

    #[test]
    fn parse_minimal_project_manifest() {
        let src = r#"p {
  name = "p"
  version = "0.1.0"
}
target "t" {
  kind = Lib
  entry = "Main.bd"
}
"#;
        let doc = parse_bsol_document(src).expect("parse");
        assert_eq!(doc.blocks.len(), 2);
        assert!(matches!(
            doc.blocks[0].header,
            BsolBlockHeader::ProjectRoot { ref ident } if ident == "p"
        ));
    }
}
