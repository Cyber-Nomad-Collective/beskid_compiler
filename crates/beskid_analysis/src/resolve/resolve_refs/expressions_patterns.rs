use crate::syntax::Spanned;
use crate::syntax::{Expression, Pattern, StructLiteralField};

use super::super::resolver::Resolver;
use super::super::tables::ResolvedValue;

impl Resolver {
    pub(super) fn resolve_expression(&mut self, expression: &Spanned<Expression>) {
        match &expression.node {
            Expression::Match(match_expr) => {
                self.resolve_expression(&match_expr.node.scrutinee);
                for arm in &match_expr.node.arms {
                    self.resolve_match_arm(arm);
                }
            }
            Expression::Lambda(lambda_expr) => {
                // A lambda is its own executable callable: SDK callback admission names the
                // directly enclosing function, never a function around the lambda.
                let previous_function = self.current_function.take();
                self.push_scope();
                for parameter in &lambda_expr.node.parameters {
                    if let Some(ty) = &parameter.node.ty {
                        self.resolve_type(ty);
                    }
                    self.insert_local(&parameter.node.name.node.name, parameter.node.name.span);
                }
                self.resolve_expression(&lambda_expr.node.body);
                self.pop_scope();
                self.current_function = previous_function;
            }
            Expression::Assign(assign_expr) => {
                self.resolve_expression(&assign_expr.node.target);
                self.resolve_expression(&assign_expr.node.value);
            }
            Expression::Binary(binary_expr) => {
                self.resolve_expression(&binary_expr.node.left);
                self.resolve_expression(&binary_expr.node.right);
            }
            Expression::Unary(unary_expr) => {
                self.resolve_expression(&unary_expr.node.expr);
            }
            Expression::Call(call_expr) => {
                // `i32(x)`/`i64(x)`/`u32(x)`/`u8(x)`/`byte(x)`/`word(x)`/`f64(x)` are primitive
                // numeric conversion call forms classified purely from AST shape by
                // `primitive_numeric_conversion_target` (beskid_queries::semantic_contract::calls::facts).
                // Their callee is not a declared item, so resolving it as an ordinary value would
                // always fail with an unknown-value error; skip callee resolution for this shape.
                if let Some((path, wrapper)) = self.native_mod_callback_callee(&call_expr.node) {
                    // A source-issued SDK callback: its signature is the wrapper's own
                    // (`call_abi_signature_for_call` in `beskid_queries`), so the callee
                    // resolves to the wrapper function item.
                    self.tables.insert_value(path.span, ResolvedValue::Item(wrapper));
                } else if let Some(path) = self.unqualified_sibling_method_callee(&call_expr.node) {
                    // `Name(...)` inside a type-body method calls the unique sibling method
                    // `Name` on the implicit receiver, exactly as `this.Name(...)` does. The
                    // callee records the receiver local; the checker types it as a method
                    // dispatch on that receiver.
                    if let Some(this_local) = self.resolve_local("this") {
                        self.tables.insert_value(path.span, ResolvedValue::Local(this_local));
                    }
                } else if !super::super::resolver::is_primitive_numeric_conversion_call(&call_expr.node) {
                    self.resolve_expression(&call_expr.node.callee);
                }
                for arg in &call_expr.node.args {
                    self.resolve_expression(arg);
                }
            }
            Expression::Member(member_expr) => {
                self.resolve_expression(&member_expr.node.target);
            }
            Expression::Literal(_) => {}
            Expression::Path(path_expr) => {
                self.resolve_path_type_arguments(&path_expr.node.path);
                self.resolve_value_path(&path_expr.node.path);
            }
            Expression::StructLiteral(literal) => {
                self.resolve_type_path(&literal.node.path);
                for field in &literal.node.fields {
                    self.resolve_struct_literal_field(field);
                }
            }
            Expression::EnumConstructor(constructor) => {
                self.resolve_enum_path(&constructor.node.path);
                for arg in &constructor.node.args {
                    self.resolve_expression(arg);
                }
            }
            Expression::Block(block_expr) => {
                self.resolve_block(&block_expr.node.block);
            }
            Expression::Grouped(grouped_expr) => {
                self.resolve_expression(&grouped_expr.node.expr);
            }
            Expression::Try(try_expr) => {
                self.resolve_expression(&try_expr.node.expr);
            }
            Expression::Spawn(spawn_expr) => {
                self.resolve_expression(&spawn_expr.node.callee);
            }
            Expression::Index(index_expr) => {
                self.resolve_expression(&index_expr.node.target);
                self.resolve_expression(&index_expr.node.index);
            }
            Expression::ArrayLiteral(lit) => {
                for element in &lit.node.elements {
                    self.resolve_expression(element);
                }
            }
            Expression::MacroInvocation(_) | Expression::MacroMetavariable(_) => {}
            Expression::CodeString(_) => {}
            Expression::ClifBlock(_) => {}
        }
    }

    pub(super) fn resolve_match_arm(&mut self, arm: &Spanned<crate::syntax::MatchArm>) {
        self.push_scope();
        self.resolve_pattern(&arm.node.pattern);
        if let Some(guard) = &arm.node.guard {
            self.resolve_expression(guard);
        }
        self.resolve_expression(&arm.node.value);
        self.pop_scope();
    }

    pub(super) fn resolve_pattern(&mut self, pattern: &Spanned<Pattern>) {
        match &pattern.node {
            Pattern::Wildcard => {}
            Pattern::Identifier(identifier) => {
                self.insert_local(&identifier.node.name, identifier.span);
            }
            Pattern::Literal(_) => {}
            Pattern::Enum(enum_pattern) => {
                self.resolve_enum_path(&enum_pattern.node.path);
                for item in &enum_pattern.node.items {
                    self.resolve_pattern(item);
                }
            }
        }
    }

    pub(super) fn resolve_struct_literal_field(&mut self, field: &Spanned<StructLiteralField>) {
        self.resolve_expression(&field.node.value);
    }

    /// The callee path of `Name(...)` when it names the enclosing type-body method's unique
    /// sibling method (see `Resolver::is_unique_sibling_method`).
    fn unqualified_sibling_method_callee<'call>(
        &self,
        call: &'call crate::syntax::CallExpression,
    ) -> Option<&'call Spanned<crate::syntax::Path>> {
        let Expression::Path(path_expr) = &call.callee.node else {
            return None;
        };
        let [segment] = path_expr.node.path.node.segments.as_slice() else {
            return None;
        };
        (segment.node.type_args.is_empty() && self.is_unique_sibling_method(segment.node.name.node.name.as_str()))
            .then_some(&path_expr.node.path)
    }

    /// The callee path of `__mod_semantic_*(...)` / `__mod_query_*(...)` and its wrapper
    /// function item when production admits the callback: an unqualified, non-generic callee in
    /// the exact canonical SDK source for that operation, called from the function body whose
    /// name the operation fixes (mirrors `native_mod_callback_for` in `beskid_queries`).
    fn native_mod_callback_callee<'call>(
        &self,
        call: &'call crate::syntax::CallExpression,
    ) -> Option<(&'call Spanned<crate::syntax::Path>, super::super::ids::ItemId)> {
        let Expression::Path(path_expr) = &call.callee.node else {
            return None;
        };
        let [segment] = path_expr.node.path.node.segments.as_slice() else {
            return None;
        };
        if !segment.node.type_args.is_empty() {
            return None;
        }
        let (wrapper_name, source_path) =
            super::super::sdk_authority::native_mod_callback_wrapper(segment.node.name.node.name.as_str())?;
        if self.sdk_source_authority != Some(source_path) {
            return None;
        }
        let (function_name, function_span) = self.current_function.as_ref()?;
        if function_name != wrapper_name {
            return None;
        }
        let wrapper = self.resolve_item_in_scope(function_name)?;
        self.items
            .get(wrapper.0)
            .is_some_and(|info| info.kind == super::super::items::ItemKind::Function && info.span == *function_span)
            .then_some((&path_expr.node.path, wrapper))
    }
}
