//! Opinionated pretty-printing: `Emit` trait and formatting context (mirrors bsharp `emit_trait.rs`, without JSONL instrumentation).

use crate::format::policy;
use crate::syntax::{Block, Program, Spanned, Statement};
use std::fmt::{self, Write};

#[derive(Debug)]
pub struct EmitError(pub fmt::Error);

impl From<fmt::Error> for EmitError {
    fn from(e: fmt::Error) -> Self {
        EmitError(e)
    }
}

#[derive(Default)]
pub struct EmitCtx {
    pub indent: usize,
    pub policy_blank_line_between_members: bool,
}

impl EmitCtx {
    pub fn new() -> Self {
        Self {
            indent: 0,
            policy_blank_line_between_members: true,
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

    pub fn between_top_level_declarations<W: Write>(&mut self, w: &mut W) -> Result<(), EmitError> {
        policy::between_top_level_declarations(self, w)
    }

    pub fn between_members<W: Write>(&mut self, w: &mut W) -> Result<(), EmitError> {
        if !self.policy_blank_line_between_members {
            return Ok(());
        }
        policy::between_members(self, w)
    }

    pub fn between_block_items<W: Write>(
        &mut self,
        w: &mut W,
        prev: &Statement,
        next: &Statement,
    ) -> Result<(), EmitError> {
        policy::between_block_items(self, w, prev, next)
    }

    pub fn open_brace<W: Write>(&mut self, w: &mut W) -> Result<(), EmitError> {
        w.write_char('{')?;
        self.nl(w)?;
        self.push_indent();
        Ok(())
    }

    pub fn close_brace<W: Write>(&mut self, w: &mut W) -> Result<(), EmitError> {
        self.pop_indent();
        self.write_indent(w)?;
        w.write_char('}')?;
        Ok(())
    }
}

pub trait Emit {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError>;
}

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

/// Format a parsed program (each top-level item on its own line; trailing newline after each item).
pub fn format_program(program: &Spanned<Program>) -> Result<String, EmitError> {
    Emitter::new().write(&program.node)
}

impl Emit for Spanned<Block> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

impl Emit for Block {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        if self.statements.is_empty() {
            w.write_str("{ }")?;
            return Ok(());
        }
        w.write_char('{')?;
        cx.nl(w)?;
        cx.push_indent();
        for (i, s) in self.statements.iter().enumerate() {
            cx.write_indent(w)?;
            s.node.emit(w, cx)?;
            cx.nl(w)?;
            if i + 1 < self.statements.len() {
                let next = &self.statements[i + 1].node;
                cx.between_block_items(w, &s.node, next)?;
            }
        }
        cx.pop_indent();
        cx.write_indent(w)?;
        w.write_char('}')?;
        Ok(())
    }
}
