use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::rules::RuleContext;
use crate::syntax::{
    ContractDefinition, InlineModule, ModuleDeclaration, Node, Path, PrimitiveType, Program, Type, Visibility,
};
use crate::syntax::{SpanInfo, Spanned};
use crate::syntax_query::{AstNode, Query};
use std::collections::{HashMap, HashSet};

impl SemanticPipelineRule {
    pub(super) fn stage0_collect_definitions(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        self.check_duplicate_definition_names(ctx, program);
        self.check_file_scoped_module_structure(ctx, program);
        self.check_duplicate_non_type_item_names(ctx, program);
        self.check_test_metadata_schema(ctx, program);
        self.check_unknown_types_in_definitions(ctx, program);
        self.check_conflicting_embedded_contracts(ctx, program);

        for definition in Query::from(&program.node).of::<crate::syntax::EnumDefinition>() {
            self.check_duplicate_enum_variants(ctx, definition);
        }

        for definition in Query::from(&program.node).of::<crate::syntax::ContractDefinition>() {
            self.check_duplicate_contract_methods(ctx, definition);
        }
    }

    fn check_duplicate_non_type_item_names(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        let mut seen: HashMap<String, SpanInfo> = HashMap::new();

        self.check_duplicate_query_entries::<crate::syntax::FunctionDefinition>(
            ctx,
            program,
            &mut seen,
            DuplicateKind::ItemName,
            |definition| (definition.name.node.name.clone(), definition.name.span),
        );
        self.check_duplicate_query_entries::<crate::syntax::TestDefinition>(
            ctx,
            program,
            &mut seen,
            DuplicateKind::ItemName,
            |definition| (definition.name.node.name.clone(), definition.name.span),
        );
        self.check_duplicate_query_entries::<crate::syntax::ModuleDeclaration>(
            ctx,
            program,
            &mut seen,
            DuplicateKind::ItemName,
            |definition| {
                if self.is_file_scoped_module_declaration(program, definition) {
                    ("<file-scope>".to_string(), definition.path.span)
                } else {
                    (self.path_dotted(&definition.path), definition.path.span)
                }
            },
        );
        self.check_duplicate_query_entries::<crate::syntax::UseDeclaration>(
            ctx,
            program,
            &mut seen,
            DuplicateKind::ItemName,
            |definition| {
                let name = definition
                    .alias
                    .as_ref()
                    .map(|alias| alias.node.name.clone())
                    .unwrap_or_else(|| self.path_tail(&definition.path));
                let span = definition.alias.as_ref().map(|alias| alias.span).unwrap_or(definition.path.span);
                (name, span)
            },
        );
    }

    fn check_file_scoped_module_structure(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        let Some((file_scope_index, file_scope_def)) = self.file_scoped_module_declaration(program) else {
            return;
        };
        let file_scope_path = self.path_to_string(&file_scope_def.node.path);
        if file_scope_index != 0 {
            ctx.emit_issue(
                file_scope_def.node.path.span,
                SemanticIssueKind::FileScopedModuleNotFirstItem { module_path: file_scope_path.clone() },
            );
        }

        for (index, item) in program.node.items.iter().enumerate() {
            if index == file_scope_index {
                continue;
            }
            match &item.node {
                Node::ModuleDeclaration(module_decl) => {
                    ctx.emit_issue(
                        module_decl.node.path.span,
                        SemanticIssueKind::DuplicateFileScopedModule {
                            module_path: self.path_to_string(&module_decl.node.path),
                        },
                    );
                }
                Node::InlineModule(inline_module) => {
                    ctx.emit_issue(
                        inline_module.node.name.span,
                        SemanticIssueKind::ModuleDeclarationForbiddenInFileScopedModule,
                    );
                    Self::emit_nested_module_errors(ctx, inline_module);
                }
                _ => {}
            }
        }
    }

    fn check_test_metadata_schema(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        for test in Query::from(&program.node).of::<crate::syntax::TestDefinition>() {
            if let Some(meta) = &test.meta {
                for entry in &meta.node.entries {
                    let key = entry.node.name.node.name.as_str();
                    if key != "tags" && key != "group" {
                        ctx.emit_issue(
                            entry.node.name.span,
                            SemanticIssueKind::InvalidSyntaxSpan {
                                context: format!(
                                    "test `{}` meta key `{}` is invalid (allowed: tags, group)",
                                    test.name.node.name, key
                                ),
                            },
                        );
                    }
                }
            }

            if let Some(skip) = &test.skip {
                for entry in &skip.node.entries {
                    let key = entry.node.name.node.name.as_str();
                    if key != "condition" && key != "reason" {
                        ctx.emit_issue(
                            entry.node.name.span,
                            SemanticIssueKind::InvalidSyntaxSpan {
                                context: format!(
                                    "test `{}` skip key `{}` is invalid (allowed: condition, reason)",
                                    test.name.node.name, key
                                ),
                            },
                        );
                    }
                    if key == "condition" {
                        let is_const_bool = matches!(
                            entry.node.value.node,
                            crate::syntax::Expression::Literal(ref literal)
                                if matches!(literal.node.literal.node, crate::syntax::Literal::Bool(_))
                        );
                        if !is_const_bool {
                            ctx.emit_issue(
                                entry.node.value.span,
                                SemanticIssueKind::InvalidSyntaxSpan {
                                    context: format!(
                                        "test `{}` skip.condition must be a boolean literal",
                                        test.name.node.name
                                    ),
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    fn emit_nested_module_errors(ctx: &mut RuleContext, inline_module: &Spanned<InlineModule>) {
        for nested in &inline_module.node.items {
            match &nested.node {
                Node::ModuleDeclaration(module_decl) => {
                    ctx.emit_issue(
                        module_decl.node.path.span,
                        SemanticIssueKind::ModuleDeclarationForbiddenInFileScopedModule,
                    );
                }
                Node::InlineModule(nested_inline) => {
                    ctx.emit_issue(
                        nested_inline.node.name.span,
                        SemanticIssueKind::ModuleDeclarationForbiddenInFileScopedModule,
                    );
                    Self::emit_nested_module_errors(ctx, nested_inline);
                }
                _ => {}
            }
        }
    }

    fn check_unknown_types_in_definitions(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        let known_types = self.collect_known_type_names(ctx, program);

        for definition in Query::from(&program.node).of::<crate::syntax::TypeDefinition>() {
            let generic_names = self.collect_generic_names(&definition.generics);
            for field in &definition.fields {
                Self::validate_type_reference(ctx, &field.node.ty, &known_types, &generic_names);
            }
        }

        for definition in Query::from(&program.node).of::<crate::syntax::EnumDefinition>() {
            let generic_names = self.collect_generic_names(&definition.generics);
            for variant in &definition.variants {
                for field in &variant.node.fields {
                    Self::validate_type_reference(ctx, &field.node.ty, &known_types, &generic_names);
                }
            }
        }

        for definition in Query::from(&program.node).of::<crate::syntax::FunctionDefinition>() {
            let generic_names = self.collect_generic_names(&definition.generics);
            for parameter in &definition.parameters {
                Self::validate_type_reference(ctx, &parameter.node.ty, &known_types, &generic_names);
            }
            if let Some(return_type) = &definition.return_type {
                Self::validate_type_reference(ctx, return_type, &known_types, &generic_names);
            }
        }

        // A method declares no generic parameters of its own: it sees those of the type or impl
        // that owns it; an `extend type` or free method sees its receiver's parameters.
        let mut owner_generics: HashMap<SpanInfo, HashSet<String>> = HashMap::new();
        for definition in Query::from(&program.node).of::<crate::syntax::TypeDefinition>() {
            let generic_names = self.collect_generic_names(&definition.generics);
            for method in &definition.methods {
                owner_generics.insert(method.node.name.span, generic_names.clone());
            }
        }
        for block in Query::from(&program.node).of::<crate::syntax::ImplBlock>() {
            let mut generic_names = self.collect_generic_names(&block.generics);
            generic_names.extend(Self::receiver_generic_names(&block.receiver_type, &known_types));
            for method in &block.methods {
                owner_generics.insert(method.node.name.span, generic_names.clone());
            }
        }

        for definition in Query::from(&program.node).of::<crate::syntax::MethodDefinition>() {
            let generic_names = owner_generics
                .get(&definition.name.span)
                .cloned()
                .unwrap_or_else(|| Self::receiver_generic_names(&definition.receiver_type, &known_types));
            Self::validate_type_reference(ctx, &definition.receiver_type, &known_types, &generic_names);
            for parameter in &definition.parameters {
                Self::validate_type_reference(ctx, &parameter.node.ty, &known_types, &generic_names);
            }
            if let Some(return_type) = &definition.return_type {
                Self::validate_type_reference(ctx, return_type, &known_types, &generic_names);
            }
        }

        for definition in Query::from(&program.node).of::<crate::syntax::ContractDefinition>() {
            let generic_names = self.collect_generic_names(&definition.generics);
            for signature in Query::from(definition).of::<crate::syntax::ContractMethodSignature>() {
                for parameter in &signature.parameters {
                    Self::validate_type_reference(ctx, &parameter.node.ty, &known_types, &generic_names);
                }
                if let Some(return_type) = &signature.return_type {
                    Self::validate_type_reference(ctx, return_type, &known_types, &generic_names);
                }
            }
        }
    }

    fn check_conflicting_embedded_contracts(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        let contracts = self.collect_contract_definitions(program);

        for definition in contracts.values() {
            let mut known_signatures = self.contract_methods(&definition.node);

            for embedding in Query::from(&definition.node).of::<crate::syntax::ContractEmbedding>() {
                let embedded_name = embedding.name.node.name.clone();
                let Some(embedded_contract) = contracts.get(&embedded_name) else {
                    continue;
                };

                for (method_name, signature) in self.contract_methods(&embedded_contract.node) {
                    let Some(previous) = known_signatures.insert(method_name.clone(), signature.clone()) else {
                        continue;
                    };
                    if previous == signature {
                        continue;
                    }

                    ctx.emit_issue(
                        embedding.name.span,
                        SemanticIssueKind::ConflictingEmbeddedContractMethod {
                            contract_name: embedded_name.clone(),
                            method_name,
                        },
                    );
                }
            }
        }
    }

    fn collect_contract_definitions<'a>(
        &self,
        program: &'a Spanned<Program>,
    ) -> HashMap<String, &'a Spanned<ContractDefinition>> {
        let mut contracts = HashMap::new();
        for definition in program.node.items.iter().filter_map(|item| match &item.node {
            Node::ContractDefinition(definition) => Some(definition),
            _ => None,
        }) {
            contracts.insert(definition.node.name.node.name.clone(), definition);
        }
        contracts
    }

    fn contract_methods(&self, definition: &ContractDefinition) -> HashMap<String, String> {
        let mut methods = HashMap::new();
        for signature in Query::from(definition).of::<crate::syntax::ContractMethodSignature>() {
            let name = signature.name.node.name.clone();
            let signature_string = self.contract_signature_string(signature);
            methods.insert(name, signature_string);
        }
        methods
    }

    fn contract_signature_string(&self, signature: &crate::syntax::ContractMethodSignature) -> String {
        let params = signature
            .parameters
            .iter()
            .map(|parameter| self.type_to_string(&parameter.node.ty))
            .collect::<Vec<_>>()
            .join(",");
        let return_type =
            signature.return_type.as_ref().map(|ty| self.type_to_string(ty)).unwrap_or_else(|| "unit".to_string());
        format!("{return_type}({params})")
    }

    fn file_scoped_module_declaration<'a>(
        &self,
        program: &'a Spanned<Program>,
    ) -> Option<(usize, &'a Spanned<ModuleDeclaration>)> {
        program.node.items.iter().enumerate().find_map(|(index, item)| match &item.node {
            Node::ModuleDeclaration(def)
                if def.node.visibility.node == Visibility::Private && def.node.attributes.is_empty() =>
            {
                Some((index, def))
            }
            _ => None,
        })
    }

    fn is_file_scoped_module_declaration(&self, program: &Spanned<Program>, definition: &ModuleDeclaration) -> bool {
        self.file_scoped_module_declaration(program)
            .map(|(_, file_scope)| file_scope.span == definition.path.span)
            .unwrap_or(false)
    }

    fn path_to_string(&self, path: &Spanned<Path>) -> String {
        path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>().join(".")
    }

    fn type_to_string(&self, ty: &Spanned<Type>) -> String {
        match &ty.node {
            Type::Primitive(primitive) => match primitive.node {
                PrimitiveType::Bool => "bool".to_string(),
                PrimitiveType::I32 => "i32".to_string(),
                PrimitiveType::I8 => "i8".to_string(),
                PrimitiveType::I16 => "i16".to_string(),
                PrimitiveType::U16 => "u16".to_string(),
                PrimitiveType::U64 => "u64".to_string(),
                PrimitiveType::I64 => "i64".to_string(),
                PrimitiveType::U32 => "u32".to_string(),
                PrimitiveType::U8 => "u8".to_string(),
                PrimitiveType::Pointer => "pointer".to_string(),
                PrimitiveType::Word => "word".to_string(),
                PrimitiveType::F64 => "f64".to_string(),
                PrimitiveType::F32 => "f32".to_string(),
                PrimitiveType::Char => "char".to_string(),
                PrimitiveType::String => "string".to_string(),
                PrimitiveType::Unit => "unit".to_string(),
                PrimitiveType::Never => "never".to_string(),
            },
            Type::Complex(path) => path
                .node
                .segments
                .iter()
                .map(|segment| segment.node.name.node.name.clone())
                .collect::<Vec<_>>()
                .join("."),
            Type::Associated { contract, name } => format!("{}::{}", self.path_to_string(contract), name.node.name),
            Type::This => "This".to_string(),
            Type::Array(inner) => format!("{}[]", self.type_to_string(inner)),
            Type::Function { return_type, parameters } => {
                let params =
                    parameters.iter().map(|parameter| self.type_to_string(parameter)).collect::<Vec<_>>().join(", ");
                format!("{}({})", self.type_to_string(return_type), params)
            }
        }
    }

    fn collect_known_type_names(&self, ctx: &RuleContext, program: &Spanned<Program>) -> HashSet<String> {
        let mut known = HashSet::new();

        for primitive in [
            "bool", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "char", "string", "unit",
            "pointer", "word", "never",
        ] {
            known.insert(primitive.to_string());
        }

        self.extend_known_type_names::<crate::syntax::TypeDefinition>(program, &mut known, |definition| {
            definition.name.node.name.clone()
        });
        self.extend_known_type_names::<crate::syntax::EnumDefinition>(program, &mut known, |definition| {
            definition.name.node.name.clone()
        });
        self.extend_known_type_names::<crate::syntax::ContractDefinition>(program, &mut known, |definition| {
            definition.name.node.name.clone()
        });

        for unit_program in self.assembly_programs_excluding_entry(ctx) {
            self.extend_known_type_names::<crate::syntax::TypeDefinition>(unit_program, &mut known, |definition| {
                definition.name.node.name.clone()
            });
            self.extend_known_type_names::<crate::syntax::EnumDefinition>(unit_program, &mut known, |definition| {
                definition.name.node.name.clone()
            });
            self.extend_known_type_names::<crate::syntax::ContractDefinition>(unit_program, &mut known, |definition| {
                definition.name.node.name.clone()
            });
        }

        known
    }

    fn collect_generic_names(&self, generics: &[Spanned<crate::syntax::Identifier>]) -> HashSet<String> {
        generics.iter().map(|identifier| identifier.node.name.clone()).collect()
    }

    /// Type parameters a receiver such as `List<T>` introduces: its bare, argument-less type
    /// arguments that name no known type.
    fn receiver_generic_names(receiver: &Spanned<Type>, known_types: &HashSet<String>) -> HashSet<String> {
        let Type::Complex(path) = &receiver.node else {
            return HashSet::new();
        };
        path.node
            .segments
            .iter()
            .flat_map(|segment| segment.node.type_args.iter())
            .filter_map(|argument| match &argument.node {
                Type::Complex(argument_path) => match argument_path.node.segments.as_slice() {
                    [segment] if segment.node.type_args.is_empty() => Some(segment.node.name.node.name.clone()),
                    _ => None,
                },
                _ => None,
            })
            .filter(|name| !known_types.contains(name))
            .collect()
    }

    fn validate_type_reference(
        ctx: &mut RuleContext,
        ty: &Spanned<Type>,
        known_types: &HashSet<String>,
        generic_names: &HashSet<String>,
    ) {
        match &ty.node {
            // `This` is a reserved keyword, never an unresolved user identifier; its validity
            // in a given position is a typechecker concern (`generic_params["This"]`), not a
            // "known type name" lookup concern.
            Type::Primitive(_) | Type::This => {}
            Type::Complex(path) => {
                if path.node.segments.len() > 1 {
                    return;
                }
                let Some(last_segment) = path.node.segments.last() else {
                    return;
                };
                let type_name = &last_segment.node.name.node.name;
                if known_types.contains(type_name) || generic_names.contains(type_name) {
                    return;
                }

                ctx.emit_issue(path.span, SemanticIssueKind::UnknownTypeInDefinition { type_name: type_name.clone() });
            }
            Type::Associated { contract, .. } => {
                let contract_type = Spanned::new(Type::Complex(contract.clone()), contract.span);
                Self::validate_type_reference(ctx, &contract_type, known_types, generic_names);
            }
            Type::Array(inner) => {
                Self::validate_type_reference(ctx, inner, known_types, generic_names);
            }
            Type::Function { return_type, parameters } => {
                Self::validate_type_reference(ctx, return_type, known_types, generic_names);
                for parameter in parameters {
                    Self::validate_type_reference(ctx, parameter, known_types, generic_names);
                }
            }
        }
    }

    fn path_tail(&self, path: &Spanned<Path>) -> String {
        path.node.segments.last().map(|segment| segment.node.name.node.name.clone()).unwrap_or_default()
    }

    fn path_dotted(&self, path: &Spanned<Path>) -> String {
        path.node.segments.iter().map(|segment| segment.node.name.node.name.as_str()).collect::<Vec<_>>().join(".")
    }

    fn check_duplicate_definition_names(&self, ctx: &mut RuleContext, program: &Spanned<Program>) {
        self.check_duplicate_definition_names_in_scope(ctx, &program.node.items);
    }

    fn check_duplicate_definition_names_in_scope(&self, ctx: &mut RuleContext, items: &[Spanned<Node>]) {
        let mut seen: HashMap<String, SpanInfo> = HashMap::new();
        for item in items {
            let name = match &item.node {
                Node::TypeDefinition(definition) => Some(&definition.node.name),
                Node::EnumDefinition(definition) => Some(&definition.node.name),
                Node::ContractDefinition(definition) => Some(&definition.node.name),
                Node::InlineModule(module) => {
                    self.check_duplicate_definition_names_in_scope(ctx, &module.node.items);
                    None
                }
                _ => None,
            };
            if let Some(name) = name {
                self.emit_duplicate_if_any(
                    ctx,
                    &mut seen,
                    name.node.name.clone(),
                    name.span,
                    DuplicateKind::DefinitionName,
                );
            }
        }
    }

    fn check_duplicate_enum_variants(&self, ctx: &mut RuleContext, definition: &crate::syntax::EnumDefinition) {
        let mut seen: HashMap<String, SpanInfo> = HashMap::new();
        for variant in Query::from(definition).of::<crate::syntax::EnumVariant>() {
            self.emit_duplicate_if_any(
                ctx,
                &mut seen,
                variant.name.node.name.clone(),
                variant.name.span,
                DuplicateKind::EnumVariant,
            );
        }
    }

    fn check_duplicate_contract_methods(&self, ctx: &mut RuleContext, definition: &crate::syntax::ContractDefinition) {
        let mut seen: HashMap<String, SpanInfo> = HashMap::new();
        for signature in Query::from(definition).of::<crate::syntax::ContractMethodSignature>() {
            self.emit_duplicate_if_any(
                ctx,
                &mut seen,
                signature.name.node.name.clone(),
                signature.name.span,
                DuplicateKind::ContractMethod,
            );
        }
    }

    fn check_duplicate_query_entries<T: AstNode + 'static>(
        &self,
        ctx: &mut RuleContext,
        program: &Spanned<Program>,
        seen: &mut HashMap<String, SpanInfo>,
        kind: DuplicateKind,
        name_and_span: impl Fn(&T) -> (String, SpanInfo),
    ) {
        for node in Query::from(&program.node).of::<T>() {
            let (name, span) = name_and_span(node);
            self.emit_duplicate_if_any(ctx, seen, name, span, kind);
        }
    }

    fn emit_duplicate_if_any(
        &self,
        ctx: &mut RuleContext,
        seen: &mut HashMap<String, SpanInfo>,
        name: String,
        span: SpanInfo,
        kind: DuplicateKind,
    ) {
        let Some(previous_span) = seen.insert(name.clone(), span) else {
            return;
        };

        let issue = match kind {
            DuplicateKind::DefinitionName => {
                SemanticIssueKind::DuplicateDefinitionName { name, previous: previous_span }
            }
            DuplicateKind::EnumVariant => SemanticIssueKind::DuplicateEnumVariant { name, previous: previous_span },
            DuplicateKind::ContractMethod => {
                SemanticIssueKind::DuplicateContractMethod { name, previous: previous_span }
            }
            DuplicateKind::ItemName => SemanticIssueKind::DuplicateItemName { name, previous: previous_span },
        };
        ctx.emit_issue(span, issue);
    }

    fn extend_known_type_names<T: AstNode + 'static>(
        &self,
        program: &Spanned<Program>,
        known: &mut HashSet<String>,
        name_of: impl Fn(&T) -> String,
    ) {
        for node in Query::from(&program.node).of::<T>() {
            known.insert(name_of(node));
        }
    }
}

#[derive(Clone, Copy)]
enum DuplicateKind {
    DefinitionName,
    EnumVariant,
    ContractMethod,
    ItemName,
}

#[cfg(test)]
mod tests {
    use crate::analysis::{AnalysisOptions, builtin_rules, run_rules};
    use crate::parser::{BeskidParser, Rule};
    use crate::parsing::parsable::Parsable;
    use crate::syntax::Program;
    use pest::Parser;

    fn unknown_definition_types(source: &str) -> Vec<String> {
        let pair =
            BeskidParser::parse(Rule::Program, source).expect("source should parse").next().expect("program pair");
        let program = Program::parse(pair).expect("source should build AST");
        let result = run_rules(&program.node, "test.bd", source, &builtin_rules(), AnalysisOptions::default());
        result
            .diagnostics
            .iter()
            .filter(|diag| diag.code.as_deref() == Some("E1005"))
            .map(|diag| diag.message.clone())
            .collect()
    }

    #[test]
    fn inline_methods_see_their_generic_type_parameters() {
        let source = r#"
            pub type Pair<TKey, TValue> {
                TKey key,
                TValue value,

                pub TKey Key() { return this.key; }
                pub bool Matches(TKey other, TValue payload) { return true; }
            }
        "#;
        assert!(unknown_definition_types(source).is_empty(), "{:?}", unknown_definition_types(source));
    }

    #[test]
    fn impl_methods_see_their_impl_generic_parameters() {
        let source = r#"
            pub type Box<T> { T value }
            impl<T> Box<T> {
                pub T Get() { return this.value; }
            }
        "#;
        assert!(unknown_definition_types(source).is_empty(), "{:?}", unknown_definition_types(source));
    }

    #[test]
    fn inline_methods_still_reject_undeclared_types() {
        let source = r#"
            pub type Holder<T> {
                T value,

                pub Missing Get(T other) { return this.value; }
            }
        "#;
        let unknown = unknown_definition_types(source);
        assert_eq!(unknown.len(), 1, "{unknown:?}");
        assert!(unknown[0].contains("Missing"), "{unknown:?}");
    }
}
