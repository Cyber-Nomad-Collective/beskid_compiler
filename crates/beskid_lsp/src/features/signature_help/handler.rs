use tower_lsp_server::ls_types::{
    Documentation, MarkupContent, MarkupKind, SignatureHelp, SignatureInformation, Uri,
};

use crate::features::project_manifest::api as project_manifest;
use crate::session::store::Document;

pub fn handle_signature_help(uri: &Uri, doc: &Document, offset: usize) -> Option<SignatureHelp> {
    if project_manifest::is_manifest_uri(uri) {
        return None;
    }
    let analysis = doc.analysis.as_ref()?;
    let (start, end) = callee_span_before_open_paren(&doc.text, offset)?;
    let mid = start.saturating_add((end.saturating_sub(start)) / 2).min(doc.text.len());
    let hover = beskid_analysis::services::hover_at_offset(analysis, mid)?;
    let label = hover
        .markdown
        .lines()
        .next()
        .unwrap_or(&hover.markdown)
        .to_string();
    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: hover.markdown,
            })),
            parameters: None,
            active_parameter: Some(0),
        }],
        active_signature: Some(0),
        active_parameter: Some(0),
    })
}

fn callee_span_before_open_paren(source: &str, offset: usize) -> Option<(usize, usize)> {
    let mut i = offset.min(source.len());
    while i > 0 {
        let prev = source.as_bytes().get(i - 1).copied()?;
        if prev == b' ' || prev == b'\t' || prev == b'\n' || prev == b'\r' {
            i -= 1;
            continue;
        }
        break;
    }
    if i == 0 {
        return None;
    }
    if *source.as_bytes().get(i - 1)? != b'(' {
        return None;
    }
    i -= 1;
    let mut end = i;
    while end > 0 {
        let b = *source.as_bytes().get(end - 1)?;
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            end -= 1;
            continue;
        }
        break;
    }
    let mut start = end;
    while start > 0 {
        let b = *source.as_bytes().get(start - 1)?;
        if b.is_ascii_alphanumeric() || b == b'_' {
            start -= 1;
            continue;
        }
        break;
    }
    if start == end {
        return None;
    }
    Some((start, end))
}
