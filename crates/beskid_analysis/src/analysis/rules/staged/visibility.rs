use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::rules::RuleContext;
use crate::hir::{
    HirBlock, HirExpressionNode, HirItem, HirPath, HirProgram, HirStatementNode, HirType, HirUseDeclaration,
    HirVisibility,
};
use crate::syntax::Spanned;
use crate::syntax_query::HirQuery;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

impl SemanticPipelineRule {
    pub(super) fn stage5_modules_and_visibility(&self, ctx: &mut RuleContext, hir: &Spanned<HirProgram>) {
        self.check_module_not_found(ctx, hir);
        self.check_visibility_violations(ctx, hir);
        self.check_extend_type_private_member_access(ctx, hir);
        self.check_unused_imports(ctx, hir);
        self.check_unused_private_items(ctx, hir);
    }

    fn check_module_not_found(&self, ctx: &mut RuleContext, hir: &Spanned<HirProgram>) {
        if self.file_scoped_module_index(hir).is_some() {
            return;
        }

        let source = PathBuf::from(ctx.source_name());
        let Some(parent) = source.parent() else {
            return;
        };

        for item in &hir.node.items {
            let HirItem::ModuleDeclaration(module) = &item.node else {
                continue;
            };
            let module_path = self.path_to_string_stage5(&module.node.path).replace('.', "/");
            let file_candidate = parent.join(format!("{module_path}.bd"));
            let mod_candidate = parent.join(module_path).join("mod.bd");
            if file_candidate.exists() || mod_candidate.exists() {
                continue;
            }

            ctx.emit_issue(
                module.node.path.span,
                SemanticIssueKind::VisibilityModuleNotFound {
                    module_path: self.path_to_string_stage5(&module.node.path),
                    file_candidate: file_candidate.display().to_string(),
                    mod_candidate: mod_candidate.display().to_string(),
                },
            );
        }
    }

    fn check_visibility_violations(&self, ctx: &mut RuleContext, hir: &Spanned<HirProgram>) {
        let private_items = self.collect_private_item_spans(hir);

        for item in &hir.node.items {
            let HirItem::UseDeclaration(use_decl) = &item.node else {
                continue;
            };
            if use_decl.node.path.node.segments.len() < 2 {
                continue;
            }
            let tail = self.path_tail_stage5(&use_decl.node.path);
            let Some(private_span) = private_items.get(&tail) else {
                continue;
            };
            let root = &use_decl.node.path.node.segments[0].node.name.node.name;
            if root == &tail {
                continue;
            }

            ctx.emit_issue(
                use_decl.node.path.span,
                SemanticIssueKind::VisibilityViolationImportPrivate { name: tail, private_span: *private_span },
            );
        }
    }

    fn check_unused_imports(&self, ctx: &mut RuleContext, hir: &Spanned<HirProgram>) {
        let used_names = self.collect_used_value_names(hir);

        for item in &hir.node.items {
            let HirItem::UseDeclaration(use_decl) = &item.node else {
                continue;
            };
            let imported_name = self.imported_name_stage5(&use_decl.node);
            if used_names.contains(&imported_name) {
                continue;
            }
            ctx.emit_issue(
                use_decl.node.path.span,
                SemanticIssueKind::UnusedImport { path: self.path_to_string_stage5(&use_decl.node.path) },
            );
        }
    }

    fn check_extend_type_private_member_access(&self, ctx: &mut RuleContext, hir: &Spanned<HirProgram>) {
        let field_visibility = self.collect_type_field_visibility(hir);

        for item in &hir.node.items {
            let HirItem::ExtendTypeDefinition(extension) = &item.node else {
                continue;
            };
            let Some(type_name) = self.type_name_stage5(&extension.node.target_type) else {
                continue;
            };
            let Some(fields) = field_visibility.get(&type_name) else {
                continue;
            };
            let private_fields = fields
                .iter()
                .filter_map(|(field_name, (visibility, span))| {
                    (*visibility == HirVisibility::Private).then_some((field_name, *span))
                })
                .collect::<HashMap<_, _>>();
            if private_fields.is_empty() {
                continue;
            }

            for method in &extension.node.methods {
                let mut locals = HashSet::from(["this".to_string()]);
                for parameter in &method.node.parameters {
                    if self.type_name_stage5(&parameter.node.ty).as_deref() == Some(type_name.as_str()) {
                        locals.insert(parameter.node.name.node.name.clone());
                    }
                }
                self.collect_block_locals_for_type(&method.node.body, &type_name, &mut locals);

                for expression in HirQuery::from(&method.node.body.node).of::<HirExpressionNode>() {
                    match expression {
                        HirExpressionNode::MemberExpression(member) => {
                            let member_name = member.node.member.node.name.clone();
                            let Some(private_span) = private_fields.get(&member_name).copied() else {
                                continue;
                            };
                            let HirExpressionNode::PathExpression(target_path) = &member.node.target.node else {
                                continue;
                            };
                            let Some(target_name) = target_path.node.path.node.segments.first() else {
                                continue;
                            };
                            if target_path.node.path.node.segments.len() == 1
                                && locals.contains(&target_name.node.name.node.name)
                            {
                                ctx.emit_issue(
                                    member.node.member.span,
                                    SemanticIssueKind::ExtendTypePrivateMemberAccess {
                                        member_name,
                                        type_name: type_name.clone(),
                                        private_span,
                                    },
                                );
                            }
                        }
                        HirExpressionNode::PathExpression(path) => {
                            let segments = &path.node.path.node.segments;
                            if segments.len() != 2 {
                                continue;
                            }
                            let target_name = &segments[0].node.name.node.name;
                            let member_name = segments[1].node.name.node.name.clone();
                            let Some(private_span) = private_fields.get(&member_name).copied() else {
                                continue;
                            };
                            if locals.contains(target_name) {
                                ctx.emit_issue(
                                    segments[1].node.name.span,
                                    SemanticIssueKind::ExtendTypePrivateMemberAccess {
                                        member_name,
                                        type_name: type_name.clone(),
                                        private_span,
                                    },
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn check_unused_private_items(&self, ctx: &mut RuleContext, hir: &Spanned<HirProgram>) {
        let used_names = self.collect_used_value_names(hir);

        for item in &hir.node.items {
            // Tests are invoked by the `beskid test` harness, not by name references in source.
            if matches!(&item.node, HirItem::TestDefinition(_)) {
                continue;
            }

            let (name, visibility, span) = match &item.node {
                HirItem::FunctionDefinition(definition) => {
                    (definition.node.name.node.name.clone(), definition.node.visibility.node, definition.node.name.span)
                }
                HirItem::TypeDefinition(definition) => {
                    (definition.node.name.node.name.clone(), definition.node.visibility.node, definition.node.name.span)
                }
                HirItem::EnumDefinition(definition) => {
                    (definition.node.name.node.name.clone(), definition.node.visibility.node, definition.node.name.span)
                }
                HirItem::ContractDefinition(definition) => {
                    (definition.node.name.node.name.clone(), definition.node.visibility.node, definition.node.name.span)
                }
                HirItem::ModuleDeclaration(definition) => (
                    self.path_tail_stage5(&definition.node.path),
                    definition.node.visibility.node,
                    definition.node.path.span,
                ),
                _ => continue,
            };

            if visibility == HirVisibility::Public || name == "main" || used_names.contains(&name) {
                continue;
            }

            ctx.emit_issue(span, SemanticIssueKind::UnusedPrivateItem { name });
        }
    }

    fn collect_private_item_spans(&self, hir: &Spanned<HirProgram>) -> HashMap<String, crate::syntax::SpanInfo> {
        let mut items = HashMap::new();
        for item in &hir.node.items {
            match &item.node {
                HirItem::FunctionDefinition(definition)
                    if definition.node.visibility.node == HirVisibility::Private =>
                {
                    items.insert(definition.node.name.node.name.clone(), definition.node.name.span);
                }
                HirItem::TypeDefinition(definition) if definition.node.visibility.node == HirVisibility::Private => {
                    items.insert(definition.node.name.node.name.clone(), definition.node.name.span);
                }
                HirItem::EnumDefinition(definition) if definition.node.visibility.node == HirVisibility::Private => {
                    items.insert(definition.node.name.node.name.clone(), definition.node.name.span);
                }
                HirItem::ContractDefinition(definition)
                    if definition.node.visibility.node == HirVisibility::Private =>
                {
                    items.insert(definition.node.name.node.name.clone(), definition.node.name.span);
                }
                HirItem::TestDefinition(definition) if definition.node.visibility.node == HirVisibility::Private => {
                    items.insert(definition.node.name.node.name.clone(), definition.node.name.span);
                }
                _ => {}
            }
        }
        items
    }

    fn collect_used_value_names(&self, hir: &Spanned<HirProgram>) -> HashSet<String> {
        let mut used = HashSet::new();
        for item in &hir.node.items {
            match &item.node {
                HirItem::FunctionDefinition(definition) => {
                    for expression in HirQuery::from(&definition.node.body.node).of::<HirExpressionNode>() {
                        self.collect_used_from_expression(expression, &mut used);
                    }
                }
                HirItem::MethodDefinition(definition) => {
                    for expression in HirQuery::from(&definition.node.body.node).of::<HirExpressionNode>() {
                        self.collect_used_from_expression(expression, &mut used);
                    }
                }
                HirItem::ExtendTypeDefinition(definition) => {
                    for method in &definition.node.methods {
                        for expression in HirQuery::from(&method.node.body.node).of::<HirExpressionNode>() {
                            self.collect_used_from_expression(expression, &mut used);
                        }
                    }
                }
                HirItem::TestDefinition(definition) => {
                    for expression in HirQuery::from(&definition.node.body.node).of::<HirExpressionNode>() {
                        self.collect_used_from_expression(expression, &mut used);
                    }
                    if let Some(meta) = &definition.node.meta {
                        for expression in HirQuery::from(&meta.node).of::<HirExpressionNode>() {
                            self.collect_used_from_expression(expression, &mut used);
                        }
                    }
                    if let Some(skip) = &definition.node.skip {
                        for expression in HirQuery::from(&skip.node).of::<HirExpressionNode>() {
                            self.collect_used_from_expression(expression, &mut used);
                        }
                    }
                }
                _ => {}
            }
        }
        used
    }

    fn collect_used_from_expression(&self, expression: &HirExpressionNode, used: &mut HashSet<String>) {
        match expression {
            HirExpressionNode::PathExpression(path_expression) => {
                for segment in &path_expression.node.path.node.segments {
                    let name = segment.node.name.node.name.clone();
                    if !name.is_empty() {
                        used.insert(name);
                    }
                }
            }
            HirExpressionNode::MemberExpression(member_expression) => {
                used.insert(member_expression.node.member.node.name.clone());
            }
            HirExpressionNode::EnumConstructorExpression(constructor_expression) => {
                for segment in &constructor_expression.node.path.node.type_path.node.segments {
                    used.insert(segment.node.name.node.name.clone());
                }
                used.insert(constructor_expression.node.path.node.variant.node.name.clone());
            }
            _ => {}
        }
    }

    fn path_tail_stage5(&self, path: &Spanned<HirPath>) -> String {
        path.node.segments.last().map(|segment| segment.node.name.node.name.clone()).unwrap_or_default()
    }

    fn path_to_string_stage5(&self, path: &Spanned<HirPath>) -> String {
        path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>().join(".")
    }

    fn imported_name_stage5(&self, use_decl: &HirUseDeclaration) -> String {
        use_decl
            .alias
            .as_ref()
            .map(|alias| alias.node.name.clone())
            .unwrap_or_else(|| self.path_tail_stage5(&use_decl.path))
    }

    fn collect_type_field_visibility(
        &self,
        hir: &Spanned<HirProgram>,
    ) -> HashMap<String, HashMap<String, (HirVisibility, crate::syntax::SpanInfo)>> {
        let mut visibility = HashMap::new();
        for item in &hir.node.items {
            if let HirItem::TypeDefinition(definition) = &item.node {
                visibility.insert(
                    definition.node.name.node.name.clone(),
                    definition
                        .node
                        .fields
                        .iter()
                        .map(|field| (field.node.name.node.name.clone(), (field.node.visibility.node, field.span)))
                        .collect(),
                );
            }
        }
        visibility
    }

    fn collect_block_locals_for_type(&self, block: &Spanned<HirBlock>, type_name: &str, locals: &mut HashSet<String>) {
        for statement in &block.node.statements {
            match &statement.node {
                HirStatementNode::LetStatement(let_statement) => {
                    if let Some(type_annotation) = &let_statement.node.type_annotation
                        && self.type_name_stage5(type_annotation).as_deref() == Some(type_name)
                    {
                        locals.insert(let_statement.node.name.node.name.clone());
                    }
                }
                HirStatementNode::WhileStatement(while_statement) => {
                    self.collect_block_locals_for_type(&while_statement.node.body, type_name, locals);
                }
                HirStatementNode::ForStatement(for_statement) => {
                    self.collect_block_locals_for_type(&for_statement.node.body, type_name, locals);
                }
                HirStatementNode::IfStatement(if_statement) => {
                    self.collect_block_locals_for_type(&if_statement.node.then_block, type_name, locals);
                    if let Some(else_branch) = &if_statement.node.else_branch {
                        match &else_branch.node {
                            crate::hir::HirElseBranch::Block(block) => {
                                self.collect_block_locals_for_type(block, type_name, locals);
                            }
                            crate::hir::HirElseBranch::If(nested) => {
                                self.collect_block_locals_for_type(&nested.node.then_block, type_name, locals);
                                if let Some(nested_else) = &nested.node.else_branch {
                                    match &nested_else.node {
                                        crate::hir::HirElseBranch::Block(block) => {
                                            self.collect_block_locals_for_type(block, type_name, locals);
                                        }
                                        crate::hir::HirElseBranch::If(_) => {}
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn type_name_stage5(&self, ty: &Spanned<HirType>) -> Option<String> {
        match &ty.node {
            HirType::Complex(path) => Some(self.path_to_string_stage5(path)),
            HirType::Primitive(primitive) => Some(format!("{:?}", primitive.node)),
            _ => None,
        }
    }

    fn file_scoped_module_index(&self, hir: &Spanned<HirProgram>) -> Option<usize> {
        hir.node.items.iter().position(|item| match &item.node {
            HirItem::ModuleDeclaration(def) => {
                def.node.visibility.node == HirVisibility::Private && def.node.attributes.is_empty()
            }
            _ => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::analysis::{AnalysisOptions, Severity, builtin_rules, run_rules};
    use crate::parser::{BeskidParser, Rule};
    use crate::parsing::parsable::Parsable;
    use crate::syntax::Program;
    use pest::Parser;

    fn analyze(source: &str) -> crate::analysis::AnalysisResult {
        let pair =
            BeskidParser::parse(Rule::Program, source).expect("source should parse").next().expect("program pair");
        let program = Program::parse(pair).expect("source should build AST");
        run_rules(&program.node, "test.bd", source, &builtin_rules(), AnalysisOptions::default())
    }

    #[test]
    fn extend_type_cannot_access_private_target_members() {
        let source = r#"
            type Account { i64 balance }
            extend type Account {
                pub i64 Balance() {
                    Account account = Account { balance: 1 };
                    return account.balance;
                }
            }
        "#;

        let result = analyze(source);

        assert!(
            result.diagnostics.iter().any(|diag| diag.code.as_deref() == Some("E1511")),
            "expected extend-type private member access diagnostic, got: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn extend_type_allows_public_target_members() {
        let source = r#"
            pub type Account { pub i64 balance }
            extend type Account {
                pub i64 Balance() {
                    Account account = Account { balance: 1 };
                    return account.balance;
                }
            }
        "#;

        let result = analyze(source);

        assert!(
            result.diagnostics.iter().all(|diag| diag.severity != Severity::Error),
            "expected public member access to pass, got: {:?}",
            result.diagnostics
        );
    }
}
