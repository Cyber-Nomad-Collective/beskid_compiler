//! Line-level elements and braced bodies, shared by every container emitter.
//!
//! A container (file, module, type, block, match, ...) is a list of elements, each on its own
//! line. Writing every element through [`emit_line`] and every body through [`open_body`] and
//! [`EmitCtx::close_brace`] is what places source comments and blank lines consistently.

use crate::doc::LeadingDocComment;
use crate::format::emit::{EmitCtx, EmitError};
use crate::syntax::SpanInfo;
use std::fmt::Write;

/// Where one line-level element sits in the source, with its optional documentation.
#[derive(Clone, Copy)]
pub(crate) struct Line<'a> {
    pub span: SpanInfo,
    pub doc: Option<&'a LeadingDocComment>,
    /// One blank line before this element, from the house style (not from the author).
    pub policy_blank: bool,
}

impl<'a> Line<'a> {
    pub fn new(span: SpanInfo) -> Self {
        Self { span, doc: None, policy_blank: false }
    }

    pub fn doc(mut self, doc: Option<&'a LeadingDocComment>) -> Self {
        self.doc = doc;
        self
    }

    pub fn blank(mut self, policy_blank: bool) -> Self {
        self.policy_blank = policy_blank;
        self
    }

    fn anchor(&self) -> usize {
        match self.doc {
            Some(doc) => doc.span.start.min(self.span.start),
            None => self.span.start,
        }
    }
}

/// Emit one element on its own line: comments before it, separation, documentation,
/// indentation, the element, `suffix` (such as `,`), a same-line comment, and the newline.
pub(crate) fn emit_line<W: Write>(
    w: &mut W,
    cx: &mut EmitCtx,
    line: Line<'_>,
    suffix: &str,
    body: impl FnOnce(&mut W, &mut EmitCtx) -> Result<(), EmitError>,
) -> Result<(), EmitError> {
    cx.element_start(w, line.anchor(), line.policy_blank)?;
    emit_leading_doc_lines(line.doc, w, cx)?;
    cx.write_indent(w)?;
    body(w, cx)?;
    cx.token(w, suffix)?;
    cx.element_end(w, line.span.end)?;
    cx.nl(w)
}

/// Write the `{` of a body after its header, in the house brace style.
pub(crate) fn open_body<W: Write>(w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
    cx.before_open_brace(w)?;
    cx.open_brace(w)
}

/// True when a body has nothing to print: no elements and no comments before its end.
pub(crate) fn body_is_empty(cx: &EmitCtx, element_count: usize, container_end: usize) -> bool {
    element_count == 0 && !cx.has_comments_before(container_end)
}

/// Write `///` documentation lines at the current indentation.
pub(crate) fn emit_leading_doc_lines<W: Write>(
    doc: Option<&LeadingDocComment>,
    w: &mut W,
    cx: &mut EmitCtx,
) -> Result<(), EmitError> {
    let Some(d) = doc else {
        return Ok(());
    };
    for line in d.normalized_source.lines() {
        cx.write_indent(w)?;
        w.write_str("///")?;
        if !line.is_empty() {
            w.write_char(' ')?;
            w.write_str(line)?;
        }
        cx.nl(w)?;
    }
    Ok(())
}
