use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::rules::RuleContext;
use crate::syntax::{Block, Expression, Node, Pattern, Program, Statement};
use crate::syntax::{SpanInfo, Spanned};
use crate::syntax_query::{AstWalker, NodeRef, Query, Visit};
use std::collections::{HashMap, HashSet};

impl SemanticPipelineRule {
    pub(super) fn stage3_control_flow_and_patterns(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        let enum_variants = self.collect_enum_variants(ctx, program);
        let variant_to_enum = self.collect_variant_to_enum(ctx, program);

        let mut walker = AstWalker::new().with_visitor(Box::new(ControlFlowVisitor::new(
            self,
            ctx,
            &enum_variants,
            &variant_to_enum,
        )));

        for item in &program.node.items {
            match &item.node {
                Node::Function(definition) => {
                    walker.walk(NodeRef::from(&definition.node.body.node));
                }
                Node::Method(definition) => {
                    walker.walk(NodeRef::from(&definition.node.body.node));
                }
                _ => {}
            }
        }
    }

    fn collect_variant_to_enum(&self, ctx: &RuleContext, program: &Spanned<Program>) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for item in &program.node.items {
            let Node::EnumDefinition(definition) = &item.node else {
                continue;
            };
            let enum_name = definition.node.name.node.name.clone();
            for variant in Query::from(&definition.node).of::<crate::syntax::EnumVariant>() {
                result.insert(variant.name.node.name.clone(), enum_name.clone());
            }
        }
        for unit_program in self.assembly_programs_excluding_entry(ctx) {
            for item in &unit_program.node.items {
                let Node::EnumDefinition(definition) = &item.node else {
                    continue;
                };
                let enum_name = definition.node.name.node.name.clone();
                for variant in Query::from(&definition.node).of::<crate::syntax::EnumVariant>() {
                    result.entry(variant.name.node.name.clone()).or_insert(enum_name.clone());
                }
            }
        }
        result
    }

    fn collect_enum_variants(
        &self,
        ctx: &RuleContext,
        program: &Spanned<Program>,
    ) -> HashMap<String, HashMap<String, usize>> {
        let mut result = HashMap::new();

        for item in &program.node.items {
            let Node::EnumDefinition(definition) = &item.node else {
                continue;
            };

            let mut variants = HashMap::new();
            for variant in Query::from(&definition.node).of::<crate::syntax::EnumVariant>() {
                variants.insert(variant.name.node.name.clone(), variant.fields.len());
            }
            result.insert(definition.node.name.node.name.clone(), variants);
        }

        for unit_program in self.assembly_programs_excluding_entry(ctx) {
            for item in &unit_program.node.items {
                let Node::EnumDefinition(definition) = &item.node else {
                    continue;
                };
                let enum_name = definition.node.name.node.name.clone();
                if result.contains_key(&enum_name) {
                    continue;
                }
                let mut variants = HashMap::new();
                for variant in Query::from(&definition.node).of::<crate::syntax::EnumVariant>() {
                    variants.insert(variant.name.node.name.clone(), variant.fields.len());
                }
                result.insert(enum_name, variants);
            }
        }

        result
    }

    /// Programs from the assembled dependency closure, excluding the entry unit (which is
    /// processed separately so local definitions take precedence over imported ones).
    pub(super) fn assembly_programs_excluding_entry<'a>(&self, ctx: &'a RuleContext) -> Vec<&'a Spanned<Program>> {
        let Some(assembly) = ctx.options.program_assembly.as_ref() else {
            return Vec::new();
        };
        assembly
            .units
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != assembly.entry_index)
            .map(|(_, unit)| &unit.program)
            .collect()
    }

    fn check_match_semantics(
        &self,
        ctx: &mut RuleContext,
        match_expression: &Spanned<crate::syntax::MatchExpression>,
        enum_variants: &HashMap<String, HashMap<String, usize>>,
    ) {
        let mut arm_kind: Option<&'static str> = None;
        let mut wildcard_seen = false;
        let mut enum_name: Option<String> = None;
        let mut covered_variants = HashSet::new();

        for arm in &match_expression.node.arms {
            if let Some(guard) = &arm.node.guard
                && !self.is_boolean_like_guard(guard)
            {
                ctx.emit_issue(guard.span, SemanticIssueKind::MatchGuardMustBeBoolean);
            }

            if let Some(kind) = self.literal_kind(&arm.node.value) {
                if let Some(previous_kind) = arm_kind {
                    if previous_kind != kind {
                        ctx.emit_issue(
                            arm.node.value.span,
                            SemanticIssueKind::MatchArmTypeMismatch {
                                expected: previous_kind.to_string(),
                                actual: kind.to_string(),
                            },
                        );
                    }
                } else {
                    arm_kind = Some(kind);
                }
            }

            match &arm.node.pattern.node {
                Pattern::Wildcard => wildcard_seen = true,
                Pattern::Enum(enum_pattern) => {
                    let current_enum = enum_pattern
                        .node
                        .path
                        .node
                        .type_path
                        .node
                        .segments
                        .last()
                        .map(|segment| segment.node.name.node.name.clone())
                        .unwrap_or_default();
                    let current_variant = enum_pattern.node.path.node.variant.node.name.clone();
                    covered_variants.insert(current_variant);
                    if let Some(existing) = &enum_name {
                        if existing != &current_enum {
                            enum_name = None;
                        }
                    } else {
                        enum_name = Some(current_enum);
                    }
                }
                _ => {
                    enum_name = None;
                }
            }
        }

        if wildcard_seen {
            return;
        }
        let Some(enum_name) = enum_name else {
            return;
        };
        let Some(variants) = enum_variants.get(&enum_name) else {
            return;
        };
        if variants.keys().all(|variant| covered_variants.contains(variant)) {
            return;
        }

        ctx.emit_issue(match_expression.span, SemanticIssueKind::MatchNonExhaustive { enum_name });
    }

    fn is_boolean_like_guard(&self, expression: &Spanned<Expression>) -> bool {
        match &expression.node {
            Expression::Literal(literal) => {
                matches!(literal.node.literal.node, crate::syntax::Literal::Bool(_))
            }
            Expression::Unary(unary_expression) => self.is_boolean_like_guard(&unary_expression.node.expr),
            Expression::Binary(binary_expression) => {
                self.is_boolean_like_guard(&binary_expression.node.left)
                    || self.is_boolean_like_guard(&binary_expression.node.right)
            }
            Expression::Grouped(grouped_expression) => self.is_boolean_like_guard(&grouped_expression.node.expr),
            _ => true,
        }
    }

    fn literal_kind(&self, expression: &Spanned<Expression>) -> Option<&'static str> {
        match &expression.node {
            Expression::Literal(literal) => match &literal.node.literal.node {
                crate::syntax::Literal::Integer(_) => Some("int"),
                crate::syntax::Literal::Float(_) => Some("float"),
                crate::syntax::Literal::String(_) => Some("string"),
                crate::syntax::Literal::Char(_) => Some("char"),
                crate::syntax::Literal::Bool(_) => Some("bool"),
                crate::syntax::Literal::Unit => Some("unit"),
            },
            Expression::Grouped(grouped_expression) => self.literal_kind(&grouped_expression.node.expr),
            _ => None,
        }
    }

    fn collect_pattern_bindings(
        &self,
        ctx: &mut RuleContext,
        pattern: &Spanned<Pattern>,
        names: &mut HashSet<String>,
        enum_variants: &HashMap<String, HashMap<String, usize>>,
    ) {
        match &pattern.node {
            Pattern::Identifier(identifier) => {
                let name = identifier.node.name.clone();
                if names.insert(name.clone()) {
                    return;
                }

                ctx.emit_issue(identifier.span, SemanticIssueKind::DuplicatePatternBinding { name });
            }
            Pattern::Enum(enum_pattern) => {
                let enum_name = enum_pattern
                    .node
                    .path
                    .node
                    .type_path
                    .node
                    .segments
                    .last()
                    .map(|segment| segment.node.name.node.name.clone())
                    .unwrap_or_default();
                let variant_name = enum_pattern.node.path.node.variant.node.name.clone();
                let Some(variants) = enum_variants.get(&enum_name) else {
                    ctx.emit_issue(
                        enum_pattern.node.path.span,
                        SemanticIssueKind::UnknownEnumPath { enum_name, variant_name },
                    );
                    return;
                };

                let Some(expected_arity) = variants.get(&variant_name) else {
                    ctx.emit_issue(
                        enum_pattern.node.path.span,
                        SemanticIssueKind::UnknownEnumPath { enum_name, variant_name },
                    );
                    return;
                };

                if enum_pattern.node.items.len() != *expected_arity {
                    ctx.emit_issue(
                        enum_pattern.span,
                        SemanticIssueKind::PatternArityMismatch {
                            expected: *expected_arity,
                            actual: enum_pattern.node.items.len(),
                        },
                    );
                }

                for item in &enum_pattern.node.items {
                    self.collect_pattern_bindings(ctx, item, names, enum_variants);
                }
            }
            Pattern::Wildcard | Pattern::Literal(_) => {}
        }
    }
}

struct ControlFlowVisitor<'a> {
    rule: &'a SemanticPipelineRule,
    ctx: &'a mut RuleContext,
    loop_depth: usize,
    enum_variants: &'a HashMap<String, HashMap<String, usize>>,
    variant_to_enum: &'a HashMap<String, String>,
}

impl<'a> ControlFlowVisitor<'a> {
    fn new(
        rule: &'a SemanticPipelineRule,
        ctx: &'a mut RuleContext,
        enum_variants: &'a HashMap<String, HashMap<String, usize>>,
        variant_to_enum: &'a HashMap<String, String>,
    ) -> Self {
        Self { rule, ctx, loop_depth: 0, enum_variants, variant_to_enum }
    }

    fn scan_unreachable_in_block(&mut self, block: &Block) {
        let mut terminated = false;
        for statement in &block.statements {
            if terminated {
                self.ctx.emit_issue(statement.span, SemanticIssueKind::UnreachableCode);
                continue;
            }
            terminated = self.statement_terminates(statement);
        }
    }

    fn statement_terminates(&mut self, statement: &Spanned<Statement>) -> bool {
        match &statement.node {
            Statement::Return(_) => true,
            Statement::Break(_) => {
                if self.loop_depth == 0 {
                    self.ctx.emit_issue(statement.span, SemanticIssueKind::BreakOutsideLoop);
                    false
                } else {
                    true
                }
            }
            Statement::Continue(_) => {
                if self.loop_depth == 0 {
                    self.ctx.emit_issue(statement.span, SemanticIssueKind::ContinueOutsideLoop);
                    false
                } else {
                    true
                }
            }
            Statement::Let(_)
            | Statement::While(_)
            | Statement::For(_)
            | Statement::If(_)
            | Statement::With(_)
            | Statement::Launch(_)
            | Statement::Expression(_) => false,
        }
    }

    fn check_call_expression(&mut self, call_expression: &Spanned<crate::syntax::CallExpression>) {
        if let Expression::Path(path_expression) = &call_expression.node.callee.node
            && path_expression.node.path.node.segments.len() == 1
            && let Some(name) = path_expression.node.path.node.segments.first()
        {
            let name_value = &name.node.name.node.name;
            if let Some(enum_name) = self.variant_to_enum.get(name_value) {
                self.ctx.emit_issue(
                    path_expression.node.path.span,
                    SemanticIssueKind::UnqualifiedEnumConstructor {
                        variant_name: name_value.clone(),
                        enum_name: enum_name.clone(),
                    },
                );
            }
        }
    }

    fn check_enum_constructor_expression(
        &mut self,
        constructor_expression: &Spanned<crate::syntax::EnumConstructorExpression>,
    ) {
        if constructor_expression.node.has_empty_parens
            && constructor_expression.node.args.is_empty()
            && let Some(span) = Self::explicit_empty_constructor_parens_span(constructor_expression, self.ctx.source())
        {
            self.ctx.emit_issue(span, SemanticIssueKind::RedundantEnumConstructorParens);
        }
        let enum_name = constructor_expression
            .node
            .path
            .node
            .type_path
            .node
            .segments
            .last()
            .map(|segment| segment.node.name.node.name.clone())
            .unwrap_or_default();
        let variant_name = constructor_expression.node.path.node.variant.node.name.clone();
        let Some(variants) = self.enum_variants.get(&enum_name) else {
            self.ctx.emit_issue(
                constructor_expression.node.path.span,
                SemanticIssueKind::UnknownEnumPath { enum_name, variant_name },
            );
            return;
        };

        let Some(expected_arity) = variants.get(&variant_name) else {
            self.ctx.emit_issue(
                constructor_expression.node.path.span,
                SemanticIssueKind::UnknownEnumPath { enum_name, variant_name },
            );
            return;
        };

        if constructor_expression.node.args.len() != *expected_arity {
            self.ctx.emit_issue(
                constructor_expression.span,
                SemanticIssueKind::EnumConstructorArityMismatch {
                    expected: *expected_arity,
                    actual: constructor_expression.node.args.len(),
                },
            );
        }
    }

    fn explicit_empty_constructor_parens_span(
        constructor_expression: &Spanned<crate::syntax::EnumConstructorExpression>,
        source: &str,
    ) -> Option<SpanInfo> {
        let source_bytes = source.as_bytes();
        if source_bytes.is_empty() {
            return None;
        }

        let expr_end = constructor_expression.span.end.min(source_bytes.len());
        let path_end = constructor_expression.node.path.span.end.min(expr_end);
        if path_end > expr_end {
            return None;
        }

        let source_slice = &source_bytes[path_end..expr_end];
        let open = source_slice.iter().position(|b| *b == b'(').map(|offset| path_end + offset);
        let close = source_slice.iter().rposition(|b| *b == b')').map(|offset| path_end + offset + 1);

        match (open, close) {
            (Some(open), Some(close)) if open < close => Some(SpanInfo::from_byte_range_in_source(source, open, close)),
            _ => None,
        }
    }

    fn check_match_expression(&mut self, match_expression: &Spanned<crate::syntax::MatchExpression>) {
        for arm in &match_expression.node.arms {
            let mut names = HashSet::new();
            self.rule.collect_pattern_bindings(self.ctx, &arm.node.pattern, &mut names, self.enum_variants);
        }
        self.rule.check_match_semantics(self.ctx, match_expression, self.enum_variants);
    }
}

impl Visit for ControlFlowVisitor<'_> {
    fn enter(&mut self, node: NodeRef<'_>) {
        if let Some(statement) = node.of::<Statement>() {
            match statement {
                Statement::While(_) | Statement::For(_) => {
                    self.loop_depth += 1;
                }
                _ => {}
            }
        }

        if let Some(block) = node.of::<Block>() {
            self.scan_unreachable_in_block(block);
        }

        if let Some(expression) = node.of::<Expression>() {
            match expression {
                Expression::Match(match_expression) => self.check_match_expression(match_expression),
                Expression::Call(call_expression) => self.check_call_expression(call_expression),
                Expression::EnumConstructor(constructor_expression) => {
                    self.check_enum_constructor_expression(constructor_expression)
                }
                _ => {}
            }
        }
    }

    fn exit(&mut self, node: NodeRef<'_>) {
        if let Some(statement) = node.of::<Statement>() {
            match statement {
                Statement::While(_) | Statement::For(_) => {
                    self.loop_depth = self.loop_depth.saturating_sub(1);
                }
                _ => {}
            }
        }
    }
}
