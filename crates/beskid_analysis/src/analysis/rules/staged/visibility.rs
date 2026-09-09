use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::rules::RuleContext;
use crate::syntax::Spanned;
use crate::syntax::{Block, Expression, Node, Path, Program, Statement, Type, UseDeclaration, Visibility};
use crate::syntax_query::Query;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

impl SemanticPipelineRule {
    pub(super) fn stage5_modules_and_visibility(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        self.check_module_not_found(ctx, program);
        self.check_visibility_violations(ctx, program);
        self.check_extend_type_private_member_access(ctx, program);
        self.check_unused_imports(ctx, program);
        self.check_unused_private_items(ctx, program);
    }

    fn check_module_not_found(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        if self.file_scoped_module_index(program).is_some() {
            return;
        }

        let source = PathBuf::from(ctx.source_name());
        let Some(parent) = source.parent() else {
            return;
        };

        for item in &program.node.items {
            let Node::ModuleDeclaration(module) = &item.node else {
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

    fn check_visibility_violations(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        let private_items = self.collect_private_item_spans(program);

        for item in &program.node.items {
            let Node::UseDeclaration(use_decl) = &item.node else {
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

    fn check_unused_imports(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        let used_names = self.collect_used_value_names(program);

        for item in &program.node.items {
            let Node::UseDeclaration(use_decl) = &item.node else {
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

    fn check_extend_type_private_member_access(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        let field_visibility = self.collect_type_field_visibility(program);

        for item in &program.node.items {
            let Node::ExtendTypeDefinition(extension) = &item.node else {
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
                    (*visibility == Visibility::Private).then_some((field_name, *span))
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

                for expression in Query::from(&method.node.body.node).of::<Expression>() {
                    match expression {
                        Expression::Member(member) => {
                            let member_name = member.node.member.node.name.clone();
                            let Some(private_span) = private_fields.get(&member_name).copied() else {
                                continue;
                            };
                            let Expression::Path(target_path) = &member.node.target.node else {
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
                        Expression::Path(path) => {
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

    fn check_unused_private_items(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        let used_names = self.collect_used_value_names(program);

        for item in &program.node.items {
            // Tests are invoked by the `beskid test` harness, not by name references in source.
            if matches!(&item.node, Node::TestDefinition(_)) {
                continue;
            }

            let (name, visibility, span) = match &item.node {
                Node::Function(definition) => {
                    (definition.node.name.node.name.clone(), definition.node.visibility.node, definition.node.name.span)
                }
                Node::TypeDefinition(definition) => {
                    (definition.node.name.node.name.clone(), definition.node.visibility.node, definition.node.name.span)
                }
                Node::EnumDefinition(definition) => {
                    (definition.node.name.node.name.clone(), definition.node.visibility.node, definition.node.name.span)
                }
                Node::ContractDefinition(definition) => {
                    (definition.node.name.node.name.clone(), definition.node.visibility.node, definition.node.name.span)
                }
                Node::ModuleDeclaration(definition) => (
                    self.path_tail_stage5(&definition.node.path),
                    definition.node.visibility.node,
                    definition.node.path.span,
                ),
                _ => continue,
            };

            if visibility == Visibility::Public || name == "main" || used_names.contains(&name) {
                continue;
            }

            ctx.emit_issue(span, SemanticIssueKind::UnusedPrivateItem { name });
        }
    }

    fn collect_private_item_spans(&self, program: &Spanned<Program>) -> HashMap<String, crate::syntax::SpanInfo> {
        let mut items = HashMap::new();
        for item in &program.node.items {
            match &item.node {
                Node::Function(definition) if definition.node.visibility.node == Visibility::Private => {
                    items.insert(definition.node.name.node.name.clone(), definition.node.name.span);
                }
                Node::TypeDefinition(definition) if definition.node.visibility.node == Visibility::Private => {
                    items.insert(definition.node.name.node.name.clone(), definition.node.name.span);
                }
                Node::EnumDefinition(definition) if definition.node.visibility.node == Visibility::Private => {
                    items.insert(definition.node.name.node.name.clone(), definition.node.name.span);
                }
                Node::ContractDefinition(definition) if definition.node.visibility.node == Visibility::Private => {
                    items.insert(definition.node.name.node.name.clone(), definition.node.name.span);
                }
                Node::TestDefinition(definition) if definition.node.visibility.node == Visibility::Private => {
                    items.insert(definition.node.name.node.name.clone(), definition.node.name.span);
                }
                _ => {}
            }
        }
        items
    }

    fn collect_used_value_names(&self, program: &Spanned<Program>) -> HashSet<String> {
        let mut used = HashSet::new();
        self.collect_used_from_items(&program.node.items, &mut used);
        used
    }

    fn collect_used_from_items(&self, items: &[Spanned<Node>], used: &mut HashSet<String>) {
        for item in items {
            if let Node::InlineModule(module) = &item.node {
                self.collect_used_from_items(&module.node.items, used);
                continue;
            }
            let generic_parameters: &[_] = match &item.node {
                Node::Function(definition) => &definition.node.generics,
                Node::TypeDefinition(definition) => &definition.node.generics,
                Node::EnumDefinition(definition) => &definition.node.generics,
                _ => &[],
            };
            let shadowed =
                generic_parameters.iter().map(|parameter| parameter.node.name.clone()).collect::<HashSet<_>>();
            for expression in Query::from(&item.node).of::<Expression>() {
                self.collect_used_from_expression(expression, &shadowed, used);
            }
            for ty in Query::from(&item.node).of::<Type>() {
                self.collect_used_from_type(ty, &shadowed, used);
            }
        }
    }

    fn collect_used_from_expression(
        &self,
        expression: &Expression,
        shadowed: &HashSet<String>,
        used: &mut HashSet<String>,
    ) {
        match expression {
            Expression::Path(path_expression) => {
                self.collect_used_from_path(&path_expression.node.path, shadowed, used);
            }
            Expression::Member(member_expression) => {
                used.insert(member_expression.node.member.node.name.clone());
            }
            Expression::StructLiteral(literal) => {
                self.collect_used_from_path(&literal.node.path, shadowed, used);
            }
            Expression::EnumConstructor(constructor_expression) => {
                self.collect_used_from_path(&constructor_expression.node.path.node.type_path, shadowed, used);
                used.insert(constructor_expression.node.path.node.variant.node.name.clone());
            }
            _ => {}
        }
    }

    fn collect_used_from_type(&self, ty: &Type, shadowed: &HashSet<String>, used: &mut HashSet<String>) {
        match ty {
            Type::Complex(path) => self.collect_used_from_path(path, shadowed, used),
            Type::Associated { contract, .. } => self.collect_used_from_path(contract, shadowed, used),
            Type::Array(inner) => self.collect_used_from_type(&inner.node, shadowed, used),
            Type::Function { return_type, parameters } => {
                self.collect_used_from_type(&return_type.node, shadowed, used);
                for parameter in parameters {
                    self.collect_used_from_type(&parameter.node, shadowed, used);
                }
            }
            Type::Primitive(_) => {}
        }
    }

    fn collect_used_from_path(&self, path: &Spanned<Path>, shadowed: &HashSet<String>, used: &mut HashSet<String>) {
        if let [segment] = path.node.segments.as_slice()
            && segment.node.type_args.is_empty()
            && shadowed.contains(&segment.node.name.node.name)
        {
            return;
        }
        for segment in &path.node.segments {
            let name = segment.node.name.node.name.clone();
            if !name.is_empty() {
                used.insert(name);
            }
            for type_arg in &segment.node.type_args {
                self.collect_used_from_type(&type_arg.node, shadowed, used);
            }
        }
    }

    fn path_tail_stage5(&self, path: &Spanned<Path>) -> String {
        path.node.segments.last().map(|segment| segment.node.name.node.name.clone()).unwrap_or_default()
    }

    fn path_to_string_stage5(&self, path: &Spanned<Path>) -> String {
        path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>().join(".")
    }

    fn imported_name_stage5(&self, use_decl: &UseDeclaration) -> String {
        use_decl
            .alias
            .as_ref()
            .map(|alias| alias.node.name.clone())
            .unwrap_or_else(|| self.path_tail_stage5(&use_decl.path))
    }

    fn collect_type_field_visibility(
        &self,
        program: &Spanned<Program>,
    ) -> HashMap<String, HashMap<String, (Visibility, crate::syntax::SpanInfo)>> {
        let mut visibility = HashMap::new();
        for item in &program.node.items {
            if let Node::TypeDefinition(definition) = &item.node {
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

    fn collect_block_locals_for_type(&self, block: &Spanned<Block>, type_name: &str, locals: &mut HashSet<String>) {
        for statement in &block.node.statements {
            match &statement.node {
                Statement::Let(let_statement) => {
                    if let Some(type_annotation) = &let_statement.node.type_annotation
                        && self.type_name_stage5(type_annotation).as_deref() == Some(type_name)
                    {
                        locals.insert(let_statement.node.name.node.name.clone());
                    }
                }
                Statement::While(while_statement) => {
                    self.collect_block_locals_for_type(&while_statement.node.body, type_name, locals);
                }
                Statement::For(for_statement) => {
                    self.collect_block_locals_for_type(&for_statement.node.body, type_name, locals);
                }
                Statement::If(if_statement) => {
                    self.collect_block_locals_for_type(&if_statement.node.then_block, type_name, locals);
                    if let Some(else_branch) = &if_statement.node.else_branch {
                        match &else_branch.node {
                            crate::syntax::ElseBranch::Block(block) => {
                                self.collect_block_locals_for_type(block, type_name, locals);
                            }
                            crate::syntax::ElseBranch::If(nested) => {
                                self.collect_block_locals_for_type(&nested.node.then_block, type_name, locals);
                                if let Some(nested_else) = &nested.node.else_branch {
                                    match &nested_else.node {
                                        crate::syntax::ElseBranch::Block(block) => {
                                            self.collect_block_locals_for_type(block, type_name, locals);
                                        }
                                        crate::syntax::ElseBranch::If(_) => {}
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

    fn type_name_stage5(&self, ty: &Spanned<Type>) -> Option<String> {
        match &ty.node {
            Type::Complex(path) => Some(self.path_to_string_stage5(path)),
            Type::Primitive(primitive) => Some(format!("{:?}", primitive.node)),
            _ => None,
        }
    }

    fn file_scoped_module_index(&self, program: &Spanned<Program>) -> Option<usize> {
        program.node.items.iter().position(|item| match &item.node {
            Node::ModuleDeclaration(def) => {
                def.node.visibility.node == Visibility::Private && def.node.attributes.is_empty()
            }
            _ => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::SemanticPipelineRule;
    use crate::analysis::{AnalysisOptions, RuleContext, Severity, builtin_rules, run_rules};
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

    #[test]
    fn imported_types_used_in_annotations_constructors_and_generic_arguments_are_not_unused() {
        let source = r#"
            use Sdk.Payload;
            use Sdk.Constructor;
            use Collections.Array;

            test imported_types_are_used {
                Payload[] values = Array.Empty<Payload>();
                Constructor item = Constructor { value: 0 };
            }
        "#;
        let pair =
            BeskidParser::parse(Rule::Program, source).expect("source should parse").next().expect("program pair");
        let program = Program::parse(pair).expect("source should build AST");

        let used = SemanticPipelineRule.collect_used_value_names(&program);

        for expected in ["Payload", "Constructor", "Array"] {
            assert!(used.contains(expected), "{expected} must count as used, got: {used:?}");
        }
    }

    #[test]
    fn generic_parameter_shadow_does_not_count_as_an_import_use() {
        let source = r#"
            use Sdk.Payload;

            unit Consume<Payload>(Payload value) {
                return;
            }
        "#;

        let pair =
            BeskidParser::parse(Rule::Program, source).expect("source should parse").next().expect("program pair");
        let program = Program::parse(pair).expect("source should build AST");
        let mut context = RuleContext::new("test.bd", source, AnalysisOptions::default());

        SemanticPipelineRule.check_unused_imports(&mut context, &program);

        assert!(
            context.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("W1503")),
            "the generic parameter owns `Payload`; the shadowed import must remain unused: {:?}",
            context.diagnostics
        );
    }

    #[test]
    fn generic_parameter_shadow_inside_inline_module_does_not_count_as_an_import_use() {
        let source = r#"
            use Sdk.Payload;

            mod Nested {
                unit Consume<Payload>(Payload value) {
                    return;
                }
            }
        "#;

        let pair =
            BeskidParser::parse(Rule::Program, source).expect("source should parse").next().expect("program pair");
        let program = Program::parse(pair).expect("source should build AST");
        let mut context = RuleContext::new("test.bd", source, AnalysisOptions::default());

        SemanticPipelineRule.check_unused_imports(&mut context, &program);

        assert!(
            context.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("W1503")),
            "the nested generic parameter owns `Payload`; the shadowed import must remain unused: {:?}",
            context.diagnostics
        );
    }
}
