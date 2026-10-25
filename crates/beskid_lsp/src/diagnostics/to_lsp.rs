//! Conversion of compiler diagnostics into editor facts and of facts into LSP diagnostics, and
//! the quick fixes that travel in `Diagnostic.data`.

use beskid_analysis::analysis::diagnostic_codes;
use beskid_analysis::{Applicability, DiagnosticTag as BeskidTag, SemanticDiagnostic, Severity};
use tower_lsp_server::ls_types::*;

use crate::position::offset_range_to_lsp;
use crate::session::store::{SyntaxDiagnostic, SyntaxDiagnosticSeverity};
use crate::workspace_scan::path_to_uri;

pub(super) fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default().trim()
}

/// The `file:` URI string for a diagnostic file name (a URI string or an absolute path).
pub(super) fn file_uri(name: &str) -> Option<String> {
    if name.starts_with("file:") {
        return Some(name.to_string());
    }
    let path = std::path::Path::new(name);
    path.is_absolute().then(|| path_to_uri(path)).flatten().map(|uri| uri.to_string())
}

/// Help, related locations, fixes and tags of one editor diagnostic. Buffer places are byte
/// offsets into the buffer the fact belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticDetail {
    pub help: Option<String>,
    /// `file:` URI of the buffer, the target of related information in the buffer.
    pub uri: Option<String>,
    pub related: Vec<RelatedFact>,
    pub fixes: Vec<FixFact>,
    pub tags: Vec<BeskidTag>,
}

/// A labelled related place of a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedFact {
    pub label: String,
    pub place: RelatedPlace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelatedPlace {
    /// Byte range in the diagnostic's own buffer.
    Buffer { start: usize, end: usize },
    /// A range in another file.
    Foreign { uri: String, range: Range },
}

/// One quick fix with its edits as byte ranges in the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixFact {
    pub title: String,
    pub applicability: Applicability,
    pub edits: Vec<(usize, usize, String)>,
}

/// One fix as it travels in `Diagnostic.data` between publish and `textDocument/codeAction`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixData {
    title: String,
    applicability: String,
    edits: Vec<TextEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct DiagnosticData {
    /// Fingerprint of the exact buffer text the fixes were computed for ([`text_fingerprint`]).
    text: String,
    fixes: Vec<FixData>,
}

/// A stable fingerprint of a buffer text (length and 64-bit FNV-1a), so a fix computed for one
/// revision is never applied to another, whatever the client's version numbering.
fn text_fingerprint(text: &str) -> String {
    let hash = text
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3));
    format!("{}:{hash:016x}", text.len())
}

/// Quick fixes for one diagnostic the client sent back with its `data`: every fix the compiler
/// attached, preferred only when machine-applicable, with a title that says to check the result
/// otherwise. Fixes computed for another revision of the buffer than `text` are refused.
pub(crate) fn quick_fixes_from_data(uri: &Uri, text: &str, diagnostic: &Diagnostic) -> Vec<CodeAction> {
    let Some(data) = diagnostic.data.clone().and_then(|data| serde_json::from_value::<DiagnosticData>(data).ok())
    else {
        return Vec::new();
    };
    if data.text != text_fingerprint(text) {
        return Vec::new();
    }
    data.fixes
        .into_iter()
        .filter_map(|fix| {
            let applicability = Applicability::parse(&fix.applicability)?;
            let mut changes = std::collections::HashMap::new();
            changes.insert(uri.clone(), fix.edits);
            Some(CodeAction {
                title: fix.title,
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diagnostic.clone()]),
                edit: Some(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }),
                is_preferred: Some(applicability == Applicability::MachineApplicable),
                ..CodeAction::default()
            })
        })
        .collect()
}

/// Convert generation-bound facts into LSP diagnostics for the given source text.
///
/// Each fact becomes one diagnostic plus one Hint marker per related place in the buffer.
pub fn lsp_diagnostics_from_syntax(source: &str, facts: &[SyntaxDiagnostic]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::with_capacity(facts.len());
    for fact in facts {
        syntax_to_lsp_diagnostics(source, fact, &mut diagnostics);
    }
    diagnostics
}

pub(super) fn syntax_diagnostic_from_semantic(diag: SemanticDiagnostic) -> SyntaxDiagnostic {
    let (start, end) = diag.range();
    let end = end.max(start.saturating_add(1));
    let related = diag
        .related
        .iter()
        .filter_map(|related| {
            let (start, end) = (related.span.offset(), related.span.offset() + related.span.len());
            let place = match &related.source {
                None => RelatedPlace::Buffer { start, end },
                Some(source) => RelatedPlace::Foreign {
                    uri: file_uri(&source.name)?,
                    range: offset_range_to_lsp(&source.text, start, end),
                },
            };
            Some(RelatedFact { label: related.label.clone(), place })
        })
        .collect();
    let fixes = diag
        .fixes
        .iter()
        .map(|fix| FixFact {
            title: fix.editor_title(),
            applicability: fix.applicability,
            edits: fix
                .edits
                .iter()
                .map(|edit| {
                    let (start, end) = edit.range();
                    (start, end, edit.replacement.clone())
                })
                .collect(),
        })
        .collect();
    let detail = DiagnosticDetail {
        help: diag.help.clone(),
        uri: file_uri(diag.file_name()),
        related,
        fixes,
        tags: diag.tags.clone(),
    };
    // Origin tag routes code actions: `None` (compiler) → "beskid";
    // `Some("beskid:mod:<type_id>")` (mod) → that tag.
    let source = diag.origin.unwrap_or_else(|| "beskid".to_string());
    SyntaxDiagnostic {
        start,
        end,
        severity: match diag.severity {
            Severity::Error => SyntaxDiagnosticSeverity::Error,
            Severity::Warning => SyntaxDiagnosticSeverity::Warning,
            Severity::Note => SyntaxDiagnosticSeverity::Note,
        },
        code: diag.code,
        message: diag.message,
        source,
        detail,
    }
}

/// The editor message: the cause sentence and, on the cause only, one `Help:` line.
fn editor_message(fact: &SyntaxDiagnostic) -> String {
    match fact.detail.help.as_deref().map(first_line).filter(|help| !help.is_empty()) {
        Some(help) => format!("{}\nHelp: {help}", first_line(&fact.message)),
        None => first_line(&fact.message).to_string(),
    }
}

fn syntax_to_lsp_diagnostics(source: &str, fact: &SyntaxDiagnostic, out: &mut Vec<Diagnostic>) {
    let range = offset_range_to_lsp(source, fact.start, fact.end.max(fact.start.saturating_add(1)));
    let code = fact.code.clone().map(NumberOrString::String);
    let code_description = fact
        .code
        .as_deref()
        .and_then(diagnostic_codes::docs_url)
        .and_then(|href| href.parse::<Uri>().ok())
        .map(|href| CodeDescription { href });
    let buffer_uri = fact.detail.uri.as_deref().and_then(|uri| uri.parse::<Uri>().ok());
    let buffer_range = |start: usize, end: usize| offset_range_to_lsp(source, start, end.max(start));
    let related_information = fact
        .detail
        .related
        .iter()
        .filter_map(|related| {
            let location = match &related.place {
                RelatedPlace::Buffer { start, end } => {
                    Location { uri: buffer_uri.clone()?, range: buffer_range(*start, *end) }
                }
                RelatedPlace::Foreign { uri, range } => Location { uri: uri.parse().ok()?, range: *range },
            };
            Some(DiagnosticRelatedInformation { location, message: related.label.clone() })
        })
        .collect::<Vec<_>>();
    let tags = fact
        .detail
        .tags
        .iter()
        .map(|tag| match tag {
            BeskidTag::Unnecessary => DiagnosticTag::UNNECESSARY,
            BeskidTag::Deprecated => DiagnosticTag::DEPRECATED,
        })
        .collect::<Vec<_>>();
    let data = (!fact.detail.fixes.is_empty()).then(|| {
        let fixes = fact
            .detail
            .fixes
            .iter()
            .map(|fix| FixData {
                title: fix.title.clone(),
                applicability: fix.applicability.as_str().to_string(),
                edits: fix
                    .edits
                    .iter()
                    .map(|(start, end, text)| TextEdit { range: buffer_range(*start, *end), new_text: text.clone() })
                    .collect(),
            })
            .collect();
        serde_json::to_value(DiagnosticData { text: text_fingerprint(source), fixes })
            .expect("diagnostic data serializes")
    });
    let source_tag = Some(fact.source.clone());
    out.push(Diagnostic {
        range,
        severity: Some(match fact.severity {
            SyntaxDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
            SyntaxDiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
            SyntaxDiagnosticSeverity::Note => DiagnosticSeverity::INFORMATION,
        }),
        code: code.clone(),
        code_description: code_description.clone(),
        source: source_tag.clone(),
        message: editor_message(fact),
        related_information: (!related_information.is_empty()).then_some(related_information),
        tags: (!tags.is_empty()).then_some(tags),
        data,
    });
    // Light markers: a Hint at each related place in this buffer, pointing back to the cause.
    for related in &fact.detail.related {
        let RelatedPlace::Buffer { start, end } = related.place else { continue };
        let back = buffer_uri.clone().map(|uri| {
            vec![DiagnosticRelatedInformation {
                location: Location { uri, range },
                message: first_line(&fact.message).to_string(),
            }]
        });
        out.push(Diagnostic {
            range: buffer_range(start, end),
            severity: Some(DiagnosticSeverity::HINT),
            code: code.clone(),
            code_description: code_description.clone(),
            source: source_tag.clone(),
            message: related.label.clone(),
            related_information: back,
            ..Diagnostic::default()
        });
    }
}
