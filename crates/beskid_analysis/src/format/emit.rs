//! The `Emit` trait and the formatting context, including the comment and blank-line hooks.

use crate::format::policy;
use crate::format::trivia::{self, Comment};
use std::fmt::{self, Write};

#[derive(Debug)]
pub struct EmitError(pub fmt::Error);

impl fmt::Display for EmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for EmitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<fmt::Error> for EmitError {
    fn from(e: fmt::Error) -> Self {
        EmitError(e)
    }
}

/// Comments of the source being formatted and the position of the next one to emit.
struct Trivia {
    source: String,
    comments: Vec<Comment>,
    next: usize,
    /// Byte ranges of string literal tokens, sorted.
    strings: Vec<(usize, usize)>,
}

/// Formatting state: indentation, the source comments still to emit, and blank-line tracking.
///
/// Every line-level element (item, member, statement, arm) is written between
/// [`EmitCtx::element_start`] and [`EmitCtx::element_end`], and every braced container is closed
/// with [`EmitCtx::close_brace`]. These three hooks place each source comment exactly once, so the
/// formatter never drops a comment; [`crate::format::format_source`] checks this afterwards.
#[derive(Default)]
pub struct EmitCtx {
    pub indent: usize,
    trivia: Option<Trivia>,
    /// Source offset just after the last emitted element or comment.
    last_end: usize,
    /// Nothing was emitted yet in the innermost container (no blank line may follow its `{`).
    fresh: bool,
}

impl EmitCtx {
    pub fn new() -> Self {
        Self { indent: 0, trivia: None, last_end: 0, fresh: true }
    }

    /// A context that re-emits the comments of `source` (the text the AST was parsed from).
    pub(crate) fn with_source(source: &str, comments: Vec<Comment>, strings: Vec<(usize, usize)>) -> Self {
        Self {
            indent: 0,
            trivia: Some(Trivia { source: source.to_string(), comments, next: 0, strings }),
            last_end: 0,
            fresh: true,
        }
    }

    pub fn push_indent(&mut self) {
        self.indent += 1;
    }

    pub fn pop_indent(&mut self) {
        if self.indent > 0 {
            self.indent -= 1;
        }
    }

    pub fn write_indent<W: Write>(&mut self, w: &mut W) -> Result<(), EmitError> {
        for _ in 0..self.indent {
            w.write_str("    ")?;
        }
        Ok(())
    }

    pub fn nl<W: Write>(&mut self, w: &mut W) -> Result<(), EmitError> {
        w.write_char('\n')?;
        Ok(())
    }

    pub fn ln<W: Write>(&mut self, w: &mut W) -> Result<(), EmitError> {
        self.nl(w)?;
        self.write_indent(w)
    }

    pub fn space<W: Write>(&mut self, w: &mut W) -> Result<(), EmitError> {
        w.write_char(' ')?;
        Ok(())
    }

    pub fn token<W: Write>(&mut self, w: &mut W, s: &str) -> Result<(), EmitError> {
        w.write_str(s)?;
        Ok(())
    }

    /// Separate a declaration header from its opening brace (the house brace style).
    pub fn before_open_brace<W: Write>(&mut self, w: &mut W) -> Result<(), EmitError> {
        policy::before_open_brace(self, w)
    }

    /// Write `{`, end the line and indent; the next element is the first of a new container.
    pub fn open_brace<W: Write>(&mut self, w: &mut W) -> Result<(), EmitError> {
        w.write_char('{')?;
        self.nl(w)?;
        self.push_indent();
        self.fresh = true;
        Ok(())
    }

    /// Emit the comments left before the container's closing brace, then write `}`.
    ///
    /// `container_end` is the source offset just after the container's `}`.
    pub fn close_brace<W: Write>(&mut self, w: &mut W, container_end: usize) -> Result<(), EmitError> {
        self.flush_own_line_comments_before(w, container_end, false)?;
        self.pop_indent();
        self.write_indent(w)?;
        w.write_char('}')?;
        self.fresh = false;
        Ok(())
    }

    /// The source text of the string literal that contains `start..end`, if any.
    ///
    /// The parser lowers an interpolated string (`"a ${b} c"`) to `+` concatenation; the
    /// formatter prints the literal as written instead.
    pub(crate) fn string_literal_containing(&self, start: usize, end: usize) -> Option<String> {
        let t = self.trivia.as_ref()?;
        let index = t.strings.partition_point(|(s, _)| *s <= start);
        let (s, e) = *t.strings.get(index.checked_sub(1)?)?;
        (s <= start && end <= e && end > s).then(|| t.source[s..e].to_string())
    }

    /// The source text of `start..end`, when formatting a source.
    pub(crate) fn source_text(&self, start: usize, end: usize) -> Option<String> {
        self.trivia.as_ref().and_then(|t| t.source.get(start..end)).map(str::to_string)
    }

    /// True when a source comment lies before `offset` and has not been emitted yet.
    ///
    /// Empty containers use it to decide between `{ }` and a braced body that holds comments.
    pub fn has_comments_before(&self, offset: usize) -> bool {
        self.trivia.as_ref().is_some_and(|t| t.comments.get(t.next).is_some_and(|c| c.start < offset))
    }

    /// Start a line-level element whose source (including its documentation) begins at `anchor`.
    ///
    /// Emits the pending comments before `anchor` on their own lines, then the blank line that
    /// separates the element from what came before: one when the policy asks for it
    /// (`policy_blank`) or when the author left one, never right after an opening brace. The caller
    /// writes the indentation and the element next.
    pub fn element_start<W: Write>(&mut self, w: &mut W, anchor: usize, policy_blank: bool) -> Result<(), EmitError> {
        let emitted = self.flush_own_line_comments_before(w, anchor, policy_blank)?;
        self.separate(w, anchor, policy_blank && !emitted)?;
        self.fresh = false;
        Ok(())
    }

    /// Finish a line-level element whose source ends at `end`, before the caller ends the line.
    ///
    /// A comment on the same source line after the element stays there. Comments inside the
    /// element that no inner element placed (for example inside a multi-line expression) follow
    /// the element on their own lines, in source order.
    pub fn element_end<W: Write>(&mut self, w: &mut W, end: usize) -> Result<(), EmitError> {
        let indent = self.indent;
        let Some(pending) = self.trivia.as_mut() else {
            self.last_end = end;
            return Ok(());
        };
        let first = pending.next;
        let mut inner_end = first;
        while pending.comments.get(inner_end).is_some_and(|c| c.start < end) {
            inner_end += 1;
        }
        let trailing = pending
            .comments
            .get(inner_end)
            .is_some_and(|c| c.start >= end && trivia::is_same_line_gap(&pending.source[end..c.start]));
        let last = if trailing { inner_end + 1 } else { inner_end };
        self.last_end = end;
        if first == last {
            return Ok(());
        }
        if inner_end == first {
            let comment = &pending.comments[first];
            w.write_char(' ')?;
            w.write_str(&comment.text)?;
        } else {
            for comment in &pending.comments[first..last] {
                w.write_char('\n')?;
                for _ in 0..indent {
                    w.write_str("    ")?;
                }
                w.write_str(&comment.text)?;
            }
        }
        self.last_end = pending.comments[last - 1].end.max(end);
        pending.next = last;
        Ok(())
    }

    /// Emit every comment that is still pending (the end of the file).
    pub(crate) fn finish<W: Write>(&mut self, w: &mut W) -> Result<(), EmitError> {
        self.flush_own_line_comments_before(w, usize::MAX, false)?;
        Ok(())
    }

    /// Emit the pending comments that start before `offset`, each on its own line at the current
    /// indentation. Returns whether at least one was emitted.
    fn flush_own_line_comments_before<W: Write>(
        &mut self,
        w: &mut W,
        offset: usize,
        policy_blank: bool,
    ) -> Result<bool, EmitError> {
        let mut emitted = false;
        loop {
            let Some(trivia) = self.trivia.as_ref() else {
                return Ok(emitted);
            };
            let Some(comment) = trivia.comments.get(trivia.next).filter(|c| c.start < offset).cloned() else {
                return Ok(emitted);
            };
            self.separate(w, comment.start, policy_blank && !emitted)?;
            self.write_indent(w)?;
            w.write_str(&comment.text)?;
            self.nl(w)?;
            self.last_end = comment.end;
            self.fresh = false;
            emitted = true;
            if let Some(trivia) = self.trivia.as_mut() {
                trivia.next += 1;
            }
        }
    }

    /// Write the blank line before the thing that starts at `start`, if one belongs there.
    fn separate<W: Write>(&mut self, w: &mut W, start: usize, policy_blank: bool) -> Result<(), EmitError> {
        if self.fresh {
            return Ok(());
        }
        let author_blank = self.trivia.as_ref().is_some_and(|t| {
            let from = self.last_end.min(t.source.len());
            let to = start.min(t.source.len());
            from < to && trivia::has_blank_line(&t.source[from..to])
        });
        if policy_blank || author_blank {
            self.nl(w)?;
        }
        Ok(())
    }
}

/// Pretty-print one syntactic construct into `w` using indentation and spacing policy in `cx`.
pub trait Emit {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError>;
}

/// Stateless formatter: delegates to [`Emit`] impls (typically the syntax AST).
pub struct Emitter;

impl Default for Emitter {
    fn default() -> Self {
        Self::new()
    }
}

impl Emitter {
    pub fn new() -> Self {
        Self
    }

    pub fn write<T: Emit>(&self, item: &T) -> Result<String, EmitError> {
        let mut cx = EmitCtx::new();
        self.write_with_ctx(item, &mut cx)
    }

    pub fn write_with_ctx<T: Emit>(&self, item: &T, cx: &mut EmitCtx) -> Result<String, EmitError> {
        let mut s = String::new();
        item.emit(&mut s, cx)?;
        Ok(s)
    }
}
