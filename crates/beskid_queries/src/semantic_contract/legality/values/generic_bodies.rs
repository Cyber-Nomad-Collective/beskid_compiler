//! E1206 value obligations of generic callables, judged against their own type parameters
//! (beskid-lings milestone 3): members of a generic `type` (and of an `impl` block of one), and
//! generic functions and methods.
//!
//! The rest of [`super::value_obligations`] never judges a position whose type names a generic
//! parameter: such a type has no concrete identity until a specialization supplies one. A body
//! that confuses two parameters (`pub A Second() { return this.second; }` in `Pair<A, B>`,
//! `Pair<B, A> { first: this.first, .. }`, or `B Pick<A, B>(A a, B b) { return a; }`) was
//! therefore accepted, and the error surfaced only at a use site, as a conflicting generic
//! binding.
//!
//! Here each parameter of the callable's owner and of the callable itself is a rigid identity: an
//! opaque nominal named by the parameter, equal only to itself. The body is read in that
//! environment: fields of `this`, parameters, locals and struct literals from their written
//! types, everything else by the same specialized-identity authority lowering uses. So
//! `this.second` is `B` and a `return` declared `A` rejects it. Only a judgement in which one side names a rigid
//! parameter is reported; everything else stays with the target-neutral obligations, so nothing
//! is reported twice. Positions whose identity the rigid environment cannot prove are not judged.

use super::*;

/// The rigid environment of a generic callable: the callable as its own specialization, with
/// every owner and own parameter bound to a rigid identity. `None` for a callable that declares
/// no parameters and whose owner declares none.
fn rigid_member_environment(db: &dyn Db, member: AstNodeKey) -> Option<(GenericSpecializationInstance, Vec<String>)> {
    let (names, _) = generic_callable_parameters(db, member)?;
    if names.is_empty() {
        return None;
    }
    let names = names.into_iter().map(str::to_owned).collect::<Vec<_>>();
    let substitutions = names
        .iter()
        .map(|name| GenericSubstitution::from_source(name.as_str(), SemanticTypeId::POINTER, rigid(name)))
        .collect::<Vec<_>>();
    let instance = GenericSpecializationInstance {
        declaration: member,
        declaration_identity: Arc::from("<rigid generic body>"),
        signature: ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::UNIT },
        substitutions: substitutions.into(),
        contract_witnesses: Arc::from([]),
    };
    Some((instance, names))
}

fn rigid(name: &str) -> Identity {
    Identity::Nominal { qualified_name: Arc::from(name), arguments: Arc::from([]) }
}

fn mentions_rigid(identity: &Identity, names: &[String]) -> bool {
    match identity {
        Identity::Abi(_) => false,
        Identity::Nominal { qualified_name, arguments } => {
            (arguments.is_empty() && names.iter().any(|name| name.as_str() == qualified_name.as_ref()))
                || arguments.iter().any(|argument| mentions_rigid(argument, names))
        }
        Identity::Array(element) => mentions_rigid(element, names),
        Identity::Function { parameters, result } => {
            parameters.iter().any(|parameter| mentions_rigid(parameter, names)) || mentions_rigid(result, names)
        }
        Identity::Contract { arguments, .. } => arguments.iter().any(|argument| mentions_rigid(argument, names)),
    }
}

/// The destination a declared type denotes in the rigid environment. A type that names none of
/// the parameters is left to the target-neutral obligations.
fn rigid_destination(
    db: &dyn Db,
    scope: AstNodeKey,
    names: &[String],
    environment: &HashMap<&str, &Identity>,
    ty: &Type,
) -> Option<Identity> {
    if !names.iter().any(|name| type_syntax_mentions_generic_parameter(ty, name)) {
        return None;
    }
    generic_source_type_identity_with_substitutions(db, scope, ty, environment).ok()
}

struct RigidJudge<'a> {
    db: &'a dyn Db,
    program: &'a Program,
    index: &'a SyntaxIndex,
    key: AstNodeKey,
    instance: &'a GenericSpecializationInstance,
    names: &'a [String],
    environment: &'a HashMap<&'a str, &'a Identity>,
}

impl RigidJudge<'_> {
    /// The identity of a value in the rigid environment. A rigid parameter has no declaration and
    /// so no layout, so the shapes that read one are proven here from their written types: a
    /// field of `this`, a parameter or typed local, an unannotated local through its initializer,
    /// and a struct literal. Every other value is read by the specialized-identity authority.
    fn value(&self, value: AstNodeId) -> Option<Identity> {
        let value = normalized_expression_node(self.index, value);
        let node = self.index.node_at(self.program, value)?;
        if let Some(literal) = node.of::<StructLiteralExpression>() {
            return generic_source_type_identity_with_substitutions(
                self.db,
                AstNodeKey { node: value, ..self.key },
                &Type::Complex(literal.path.clone()),
                self.environment,
            )
            .ok();
        }
        if let Some(path) = node.of::<PathExpression>() {
            let segments = path.path.node.segments.as_slice();
            if segments.iter().all(|segment| segment.node.type_args.is_empty()) {
                match segments {
                    [receiver, field] if receiver.node.name.node.name == "this" => {
                        return self.this_field(field.node.name.node.name.as_str());
                    }
                    [local] => {
                        if let Some(identity) = self.local(value, local.node.name.node.name.as_str()) {
                            return Some(identity);
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(member) = node.of::<beskid_analysis::syntax::MemberExpression>()
            && let Expression::Path(target) = &member.target.node
            && let [receiver] = target.node.path.node.segments.as_slice()
            && receiver.node.name.node.name == "this"
        {
            return self.this_field(member.member.node.name.as_str());
        }
        let identity = specialized_source_expression_identity(
            self.db,
            AstNodeKey { node: value, ..self.key },
            Some(self.instance),
        )
        .ok()?;
        // A call's result is trusted only when it was read through the rigid parameters. A call
        // the rigid environment cannot specialize (an extern contract bridge such as
        // `DynamicUnpackingBridgeV1<T>.beskid_dynamic_v1_unpack_box(payload)` in Corelib) falls
        // back to a target-neutral ABI that does not describe its value.
        if node.of::<CallExpression>().is_some() && !mentions_rigid(&identity, self.names) {
            return None;
        }
        Some(identity)
    }

    /// The declared type of the field `name` of the member's owner type, in the rigid environment.
    fn this_field(&self, name: &str) -> Option<Identity> {
        let owner = super::super::super::locals::method_owner_node(self.program, self.index, self.key.node)?;
        let definition = self.index.node_at(self.program, owner)?.of::<TypeDefinition>()?;
        if !definition.generics.iter().all(|generic| self.names.iter().any(|name| name == generic.node.name.as_str())) {
            return None;
        }
        let field = definition
            .fields
            .iter()
            .find(|field| field.node.kind == FieldKind::Value && field.node.name.node.name == name)?;
        generic_source_type_identity_with_substitutions(
            self.db,
            AstNodeKey { node: owner, ..self.key },
            &field.node.ty.node,
            self.environment,
        )
        .ok()
    }

    /// The identity of the local `name` read at `site`: a parameter or typed local by its written
    /// type, an unannotated local by its initializer, a `for` variable by its array iterable.
    fn local(&self, site: AstNodeId, name: &str) -> Option<Identity> {
        let declaration = resolve_lexical_declaration(self.program, self.index, site, name)?;
        let parent = parent_node(self.index, declaration)?;
        let scope = AstNodeKey { node: parent, ..self.key };
        let node = self.index.node_at(self.program, parent)?;
        if let Some(parameter) = node.of::<beskid_analysis::syntax::Parameter>() {
            return generic_source_type_identity_with_substitutions(
                self.db,
                scope,
                &parameter.ty.node,
                self.environment,
            )
            .ok();
        }
        // A `for` variable is an element of its iterable's array identity.
        if let Some(statement) = node.of::<beskid_analysis::syntax::ForStatement>() {
            if range_iterable_bounds(&statement.iterable.node).is_some() {
                return None;
            }
            let iterable = self.index.direct_child_id(self.program, parent, DynNodeRef::from(&statement.iterable))?;
            return match self.value(iterable)? {
                Identity::Array(element) => Some(*element),
                _ => None,
            };
        }
        let statement = node.of::<LetStatement>()?;
        match statement.type_annotation.as_ref() {
            Some(annotation) => {
                generic_source_type_identity_with_substitutions(self.db, scope, &annotation.node, self.environment).ok()
            }
            None => {
                let initializer =
                    self.index.direct_child_id(self.program, parent, DynNodeRef::from(&statement.value))?;
                self.value(initializer)
            }
        }
    }

    /// Judge `value` against `expected` at `site` when either side names a rigid parameter.
    fn judge(&self, expected: &Identity, value: AstNodeId, site: AstNodeId, findings: &mut Vec<ValueObligation>) {
        let Some(actual) = self.value(value) else { return };
        if !mentions_rigid(expected, self.names) && !mentions_rigid(&actual, self.names) {
            return;
        }
        judge(
            self.db,
            findings,
            &Destination::Identity(expected.clone()),
            &actual,
            AstNodeKey { node: site, ..self.key },
        );
    }

    /// Judge a value whose destination is a declared type that need not name a parameter
    /// (`i64 n = this.first;`): the declared identity when it is concrete, else the rigid one.
    fn judge_declared(
        &self,
        scope: AstNodeKey,
        ty: &Type,
        value: AstNodeId,
        site: AstNodeId,
        findings: &mut Vec<ValueObligation>,
    ) {
        let expected = match rigid_destination(self.db, scope, self.names, self.environment, ty) {
            Some(expected) => expected,
            // A contract destination is judged by conformance, which a rigid parameter has no
            // declaration for; only a concrete declared identity is compared.
            None if matches!(ty, Type::Complex(path) if resolve_contract(self.db, scope, &path.node).is_some()) => {
                return;
            }
            None => match generic_source_type_identity(self.db, scope, ty) {
                Ok(expected) => expected,
                Err(_) => return,
            },
        };
        self.judge(&expected, value, site, findings);
    }
}

/// Judge the `return` values, typed `let` initializers and struct literal fields of a generic
/// function or method, or of a member of a generic type, against its rigid parameters.
pub(super) fn collect_generic_member_values(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    item: DynNodeRef<'_>,
    findings: &mut Vec<ValueObligation>,
) {
    let return_type = if let Some(function) = item.of::<FunctionDefinition>() {
        function.return_type.as_ref()
    } else if let Some(method) = item.of::<MethodDefinition>() {
        method.return_type.as_ref()
    } else {
        return;
    };
    let Some((instance, names)) = rigid_member_environment(db, key) else { return };
    let rigid_names = names.iter().map(|name| (name.as_str(), rigid(name))).collect::<Vec<_>>();
    let environment = rigid_names.iter().map(|(name, identity)| (*name, identity)).collect::<HashMap<_, _>>();
    let rigid_judge =
        RigidJudge { db, program, index, key, instance: &instance, names: &names, environment: &environment };

    if let Some(declared) = return_type {
        for statement in nodes_of_kind(index, key.node, NodeKind::ReturnStatement) {
            if enclosing_callable(index, statement) != Some(key.node) {
                continue;
            }
            let Some(value) = index
                .node_at(program, statement)
                .and_then(|node| node.of::<ReturnStatement>())
                .and_then(|statement| statement.value.as_ref())
            else {
                continue;
            };
            let Some((_, subject)) = expression_nodes(program, index, statement, value) else { continue };
            rigid_judge.judge_declared(key, &declared.node, subject, statement, findings);
        }
    }

    for statement in nodes_of_kind(index, key.node, NodeKind::LetStatement) {
        let Some(let_statement) = index.node_at(program, statement).and_then(|node| node.of::<LetStatement>()) else {
            continue;
        };
        let Some(annotation) = let_statement.type_annotation.as_ref() else { continue };
        let Some((_, subject)) = expression_nodes(program, index, statement, &let_statement.value) else { continue };
        let Some(name) = index.direct_child_id(program, statement, DynNodeRef::from(&let_statement.name)) else {
            continue;
        };
        rigid_judge.judge_declared(AstNodeKey { node: statement, ..key }, &annotation.node, subject, name, findings);
    }

    for node in nodes_of_kind(index, key.node, NodeKind::StructLiteralExpression) {
        let Some(literal) = index.node_at(program, node).and_then(|node| node.of::<StructLiteralExpression>()) else {
            continue;
        };
        // The literal's applied type in the rigid environment (`Pair<B, A>`), and its declaration.
        let Some(Identity::Nominal { arguments, .. }) = rigid_judge.value(node) else { continue };
        let Some(declaration) = resolve_type_declaration(db, AstNodeKey { node, ..key }, &literal.path.node) else {
            continue;
        };
        let Some(definition) = type_definition(db, declaration) else { continue };
        if definition.generics.len() != arguments.len() {
            continue;
        }
        let generics = definition.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>();
        let applied = generics.iter().copied().zip(arguments.iter()).collect::<HashMap<_, _>>();
        for field in &literal.fields {
            let Some(declared) =
                definition.fields.iter().find(|declared| declared.node.name.node.name == field.node.name.node.name)
            else {
                continue;
            };
            if declared.node.kind != FieldKind::Value
                || !generics
                    .iter()
                    .any(|generic| type_syntax_mentions_generic_parameter(&declared.node.ty.node, generic))
            {
                continue;
            }
            let Ok(expected) =
                generic_source_type_identity_with_substitutions(db, declaration, &declared.node.ty.node, &applied)
            else {
                continue;
            };
            let Some(field_node) = index.direct_child_id(program, node, DynNodeRef::from(field)) else { continue };
            let Some((site, value)) = expression_nodes(program, index, field_node, &field.node.value) else { continue };
            rigid_judge.judge(&expected, value, site, findings);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic_contract::legality::tests::one_unit_assembly;

    fn methods(db: &dyn Db, key: AstNodeKey) -> Vec<AstNodeKey> {
        let mut found = Vec::new();
        if node_kind(db, key).expect("node kind") == Some(IndexedNodeKind::MethodDefinition) {
            found.push(key);
        }
        if let Some(children) = child_nodes(db, key).expect("child nodes") {
            for child in children.iter().copied() {
                found.extend(methods(db, child));
            }
        }
        found
    }

    fn findings(source: &str, generation: u64) -> Vec<(String, String)> {
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(generation));
        methods(&db, root)
            .into_iter()
            .flat_map(|method| value_obligations(&db, method).expect("value obligations").map(|found| found.to_vec()))
            .flatten()
            .map(|finding| (finding.expected.to_string(), finding.actual.to_string()))
            .collect()
    }

    fn function_findings(source: &str, generation: u64) -> Vec<(String, String)> {
        let (db, root) = one_unit_assembly(source, SyntaxGenerationId(generation));
        crate::semantic_contract::legality::tests::function_definitions(&db, root)
            .into_iter()
            .flat_map(|function| {
                value_obligations(&db, function).expect("value obligations").map(|found| found.to_vec())
            })
            .flatten()
            .map(|finding| (finding.expected.to_string(), finding.actual.to_string()))
            .collect()
    }

    #[test]
    fn a_generic_function_returning_another_parameter_is_reported_in_the_body() {
        let found = function_findings("B Pick<A, B>(A first, B second) { return first; }", 9_675);
        assert_eq!(found, vec![("B".to_owned(), "A".to_owned())]);
    }

    #[test]
    fn a_generic_function_parameter_into_a_concrete_or_other_parameter_local_is_reported() {
        let found = function_findings(
            "i64 Read<T, U>(T value, U other) { i64 copy = value; U swapped = value; return copy; }",
            9_676,
        );
        assert_eq!(found, vec![("i64".to_owned(), "T".to_owned()), ("U".to_owned(), "T".to_owned())]);
    }

    #[test]
    fn a_generic_function_building_a_generic_type_with_swapped_arguments_is_reported() {
        let found = function_findings(
            "pub type Pair<A, B> { A first; B second; }\n\
             Pair<A, B> Make<A, B>(A first, B second) { return Pair<A, B> { first: second, second: first }; }",
            9_677,
        );
        assert_eq!(found, vec![("A".to_owned(), "B".to_owned()), ("B".to_owned(), "A".to_owned())]);
    }

    /// Corelib `Core.Dynamic.Unpack<T>`: the result of an extern contract bridge call is not
    /// proven in the rigid environment, so it is not judged.
    #[test]
    fn an_extern_contract_bridge_result_is_not_judged() {
        let found = function_findings(
            "pub enum Result<T, E> { Ok(T value), Error(E error) }\n\
             pub enum Failure { Invalid }\n\
             [Extern(Abi:\"C\", Library:\"beskid_runtime\")]\n\
             contract Bridge<T> { Result<T, Failure> bridge_unpack(u64 payload); }\n\
             pub Result<T, Failure> Unpack<T>(u64 payload) { return Bridge<T>.bridge_unpack(payload); }",
            9_679,
        );
        assert_eq!(found, Vec::<(String, String)>::new());
    }

    #[test]
    fn a_generic_function_consistent_with_its_parameters_has_no_findings() {
        let found = function_findings(
            "pub type Pair<A, B> { A first; B second; }\n\
             T Same<T>(T value) { T copy = value; let other = copy; return other; }\n\
             Pair<B, A> Swap<A, B>(A first, B second) { return Pair<B, A> { first: second, second: first }; }\n\
             T First<T>(T[] values, T fallback) { for value in values { return value; } return fallback; }\n\
             i64 Count<T>(T[] values) { return 0_i64; }",
            9_678,
        );
        assert_eq!(found, Vec::<(String, String)>::new());
    }

    /// beskid-lings milestone 3: the body is judged against the type's own parameters.
    #[test]
    fn returning_one_parameter_where_another_is_declared_is_reported_in_the_body() {
        let found =
            findings("pub type Pair<A, B> { A first; B second; pub A Second() { return this.second; } }", 9_671);
        assert_eq!(found, vec![("A".to_owned(), "B".to_owned())]);
    }

    #[test]
    fn swapped_struct_literal_fields_are_reported_in_the_body() {
        let found = findings(
            "pub type Pair<A, B> { A first; B second; \
             pub Pair<B, A> Swap() { return Pair<B, A> { first: this.first, second: this.second }; } }",
            9_672,
        );
        assert_eq!(found, vec![("B".to_owned(), "A".to_owned()), ("A".to_owned(), "B".to_owned())]);
    }

    #[test]
    fn a_parameter_into_a_concrete_declared_type_is_reported_in_the_body() {
        let found =
            findings("pub type Box<T> { T value; pub i64 Read() { i64 copy = this.value; return copy; } }", 9_673);
        assert_eq!(found, vec![("i64".to_owned(), "T".to_owned())]);
    }

    #[test]
    fn a_body_consistent_with_its_parameters_has_no_findings() {
        let found = findings(
            "pub type Pair<A, B> { A first; B second; \
             pub A First() { return this.first; } \
             pub B Second() { B copy = this.second; return copy; } \
             pub Pair<B, A> Swap() { return Pair<B, A> { first: this.second, second: this.first }; } \
             pub i64 Count() { return 2_i64; } }",
            9_674,
        );
        assert_eq!(found, Vec::<(String, String)>::new());
    }
}
