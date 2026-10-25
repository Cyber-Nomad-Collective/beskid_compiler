use crate::format::attribute_lists::emit_attribute_block;
use crate::format::emit::{Emit, EmitCtx, EmitError};
use crate::format::items::helpers::{emit_generics_list, emit_method_parameter_list, emit_parameter_list};
use crate::syntax::{Expression, FunctionDefinition, MethodDefinition, Spanned, Statement, WhereBound};
use std::fmt::Write;

fn emit_where_bounds<W: Write>(bounds: &[WhereBound], w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
    if bounds.is_empty() {
        return Ok(());
    }
    cx.space(w)?;
    cx.token(w, "where")?;
    cx.space(w)?;
    for (i, bound) in bounds.iter().enumerate() {
        if i > 0 {
            cx.token(w, ", ")?;
        }
        bound.parameter.emit(w, cx)?;
        w.write_char(':')?;
        cx.space(w)?;
        bound.contract.emit(w, cx)?;
    }
    Ok(())
}

impl Emit for FunctionDefinition {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        emit_attribute_block(&self.attributes, w, cx)?;
        self.visibility.emit(w, cx)?;
        if let Some(rt) = &self.return_type {
            rt.emit(w, cx)?;
            cx.space(w)?;
        }
        self.name.emit(w, cx)?;
        emit_generics_list(&self.generics, w, cx)?;
        emit_parameter_list(&self.parameters, &self.parameter_docs, w, cx)?;
        emit_where_bounds(&self.where_bounds, w, cx)?;
        cx.before_open_brace(w)?;
        self.body.emit(w, cx)
    }
}

impl Emit for Spanned<FunctionDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}

/// The expression of an expression-bodied method (`=> value`), which the parser stores as a
/// block holding one `return`, and whether the source ends it with `;`.
///
/// A written block starts with `{`; the lowered one carries the whole method's span instead.
fn expression_body<'a>(method: &'a MethodDefinition, cx: &EmitCtx) -> Option<(&'a Spanned<Expression>, bool)> {
    let text = cx.source_text(method.body.span.start, method.body.span.end)?;
    if text.trim_start().starts_with('{') {
        return None;
    }
    let [statement] = method.body.node.statements.as_slice() else { return None };
    let Statement::Return(ret) = &statement.node else { return None };
    let value = ret.node.value.as_ref()?;
    Some((value, text.trim_end().ends_with(';')))
}

impl Emit for MethodDefinition {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        emit_attribute_block(&self.attributes, w, cx)?;
        self.visibility.emit(w, cx)?;
        if let Some(rt) = &self.return_type {
            rt.emit(w, cx)?;
            cx.space(w)?;
        }
        self.name.emit(w, cx)?;
        emit_generics_list(&self.generics, w, cx)?;
        emit_method_parameter_list(self.receiver.as_ref(), &self.parameters, &self.parameter_docs, w, cx)?;
        emit_where_bounds(&self.where_bounds, w, cx)?;
        if let Some((value, semicolon)) = expression_body(self, cx) {
            cx.token(w, " => ")?;
            value.emit(w, cx)?;
            if semicolon {
                cx.token(w, ";")?;
            }
            return Ok(());
        }
        cx.before_open_brace(w)?;
        self.body.emit(w, cx)
    }
}

impl Emit for Spanned<MethodDefinition> {
    fn emit<W: Write>(&self, w: &mut W, cx: &mut EmitCtx) -> Result<(), EmitError> {
        self.node.emit(w, cx)
    }
}
