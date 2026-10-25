//! Copy on assignment (v0.6.7, `language-meta--memory-model--memory-and-references` and
//! `core-library--foundation-and-primitives--core-collections`).
//!
//! A type marked `[CopyOnAssign]` (Corelib `List<T>` only) is a value: wherever a value of that
//! type gets a new name, the compiler calls the type's `This Copy()` method, so a write through
//! one name is never visible through another. `Copy` is copy-on-write: it returns a new value
//! that shares the storage and marks the storage shared, and the first mutation through either
//! name copies the storage. Read-only use costs one small allocation per new name and no
//! element copy.
//!
//! A new name is made by a read of a place (a local, a parameter, a field, an array element, or
//! the payload `r?` takes out of a `Result`/`Option`) in a copy position: a `let` value, an
//! assignment value, an argument of a parameter that is not `mut`, a struct field, array
//! element or enum payload of a construction, and a returned place that the callable does not
//! own (a field, an element, `this`, a `mut` parameter or a pattern binding). The element an
//! array `for` loop binds, every `match` pattern binding and every closure capture are new names
//! too. A method receiver, a
//! `mut` parameter argument, a `move` operand, an operator operand and a match scrutinee read
//! the place without a copy. Values of a type parameter are decided per specialization: inside
//! `Get<T>` instantiated with `List<i64>`, `return values[index]` copies.

use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

use super::contracts::concrete_declaration;
use super::*;

type Program = beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>;

/// The position that makes a place read store a copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ValueCopyPosition {
    /// The value of a `let`.
    Binding,
    /// The value of an assignment to a local, field or element.
    Assignment,
    /// An argument of a parameter that is not `mut`.
    Argument,
    /// A struct field, array element or enum payload of a construction.
    Construction,
    /// A returned place that the callable does not own.
    Return,
}

/// The copy position of the place expression `key`, or `None` when the read stores no copy.
/// Only the place node itself (a path, member or index expression) has a position; its grouping
/// and `Expression` wrappers have none, so a read is copied once.
pub fn value_copy_position(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ValueCopyPosition> {
    with_registered_syntax(db, key, value_copy_position_tracked)
}

#[salsa::tracked(persist)]
fn value_copy_position_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ValueCopyPosition> {
    let position = with_node(db, syntax, key, |program, index, _node| copy_position(db, key, program, index))?;
    Ok(position)
}

/// The exact `Copy` instance a place read at `key` calls in `enclosing` (the specialization of
/// the item being lowered, or `None` for a concrete item). `Ok(None)` when the read is in no copy
/// position or its type is not `[CopyOnAssign]`. A `[CopyOnAssign]` type without a valid
/// `This Copy()` method is an error, so the read never silently shares storage.
pub fn value_copy_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    if value_copy_position(db, key)?.is_none() {
        return Ok(None);
    }
    // A read whose type is not proven is not a `[CopyOnAssign]` value: every nominal place type
    // is proven by the same authority that types its method calls and field reads, which report
    // their own failures.
    let Ok(identity) = place_identity(db, key, enclosing) else {
        return Ok(None);
    };
    hook_instance(db, key, &identity)
}

/// The `Copy` instance the element read of an array `for` loop calls before it binds the loop
/// variable, so the variable is a copy of the element like any other binding.
pub fn for_element_copy_specialization(
    db: &dyn Db,
    statement: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let Some(syntax) = db.syntax_unit(statement.unit).filter(|syntax| syntax.accepts_key(db, statement)) else {
        return Ok(None);
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let Some(for_statement) =
        index.node_at(program, statement.node).and_then(|node| node.of::<beskid_analysis::syntax::ForStatement>())
    else {
        return Ok(None);
    };
    let Some(iterable) = index.direct_child_id(program, statement.node, DynNodeRef::from(&for_statement.iterable))
    else {
        return Ok(None);
    };
    let Ok(GenericSourceTypeIdentity::Array(element)) =
        specialized_source_expression_identity(db, AstNodeKey { node: iterable, ..statement }, enclosing)
    else {
        return Ok(None);
    };
    hook_instance(db, statement, &element)
}

/// The `Copy` instance a closure calls when it captures the local `declaration` (its declaring
/// identifier), so the closure holds a copy like any other binding.
pub fn capture_copy_specialization(
    db: &dyn Db,
    declaration: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let identity = match enclosing {
        Some(enclosing) => match super::contracts::specialized_local_declaration_identity(db, declaration, enclosing) {
            Some(Ok(identity)) => Some(identity),
            _ => None,
        },
        None => None,
    };
    let identity = match identity {
        Some(identity) => identity,
        None => {
            let Some(syntax) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))
            else {
                return Ok(None);
            };
            let Ok(identity) = generic_source_local_identity(
                db,
                syntax.expanded_program(db),
                syntax.syntax_index(db),
                declaration,
                declaration.node,
            ) else {
                return Ok(None);
            };
            identity
        }
    };
    hook_instance(db, declaration, &identity)
}

/// The `Copy` instance a `match` arm calls when it binds the pattern variable `declaration` (its
/// declaring identifier), so a write through the binding never reaches the matched value.
/// `Ok(None)` for an identifier that is not a pattern binding.
pub fn pattern_binding_copy_specialization(
    db: &dyn Db,
    declaration: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let Some(syntax) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)) else {
        return Ok(None);
    };
    let index = syntax.syntax_index(db);
    if index.kind(declaration.node) != Some(NodeKind::Identifier)
        || parent_node(index, declaration.node).and_then(|parent| index.kind(parent)) != Some(NodeKind::Pattern)
    {
        return Ok(None);
    }
    capture_copy_specialization(db, declaration, enclosing)
}

/// The exact `Copy` instance of a value of `identity`, or `None` when its type is not
/// `[CopyOnAssign]`.
fn hook_instance(
    db: &dyn Db,
    site: AstNodeKey,
    identity: &GenericSourceTypeIdentity,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let GenericSourceTypeIdentity::Nominal { arguments, .. } = identity else {
        return Ok(None);
    };
    let Some(declaration) = concrete_declaration(db, site, identity) else {
        return Ok(None);
    };
    let Some((hook, generics)) = copy_hook(db, declaration)? else {
        return Ok(None);
    };
    if generics.len() != arguments.len() {
        return Err(SemanticError::unavailable_at("value_copy_specialization", site));
    }
    let declaration_identity = stable_declaration_identity(db, hook)
        .ok_or_else(|| SemanticError::unavailable_at("value_copy_specialization", site))?;
    let substitutions = generics
        .iter()
        .zip(arguments.iter())
        .map(|(name, argument)| GenericSubstitution::from_source(name.clone(), argument.abi_type(), argument.clone()))
        .collect::<Vec<_>>();
    Ok(Some(GenericSpecializationInstance {
        declaration: hook,
        declaration_identity,
        signature: ItemSignature { parameters: Arc::from([SemanticTypeId::POINTER]), result: SemanticTypeId::POINTER },
        substitutions: substitutions.into(),
        contract_witnesses: Arc::from([]),
    }))
}

/// The source identity of the place `key` in `enclosing`. An element read takes the element of
/// its array target's identity in the same environment, so `values[index]` in `Get<T>` is
/// `List<i64>` inside `Get<List<i64>>`.
fn place_identity(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    if enclosing.is_some()
        && let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key))
    {
        let index = syntax.syntax_index(db);
        let program = syntax.expanded_program(db);
        if let Some(indexed) =
            index.node_at(program, key.node).and_then(|node| node.of::<beskid_analysis::syntax::IndexExpression>())
            && let Some(target) = index.direct_child_id(program, key.node, DynNodeRef::from(indexed.target.as_ref()))
        {
            return match specialized_source_expression_identity(db, AstNodeKey { node: target, ..key }, enclosing)? {
                GenericSourceTypeIdentity::Array(element) => Ok(*element),
                _ => Err(SemanticError::unavailable_at("value_copy_specialization", key)),
            };
        }
    }
    specialized_source_expression_identity(db, key, enclosing)
}

/// The `Copy` method and the owner's generic parameter names of a `[CopyOnAssign]` type, or
/// `None` for any other type.
fn copy_hook(db: &dyn Db, declaration: AstNodeKey) -> Result<Option<(AstNodeKey, Vec<Arc<str>>)>, SemanticError> {
    let Some(syntax) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)) else {
        return Ok(None);
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let Some(definition) =
        index.node_at(program, declaration.node).and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
    else {
        return Ok(None);
    };
    if !has_recognized_attribute(db, declaration, &definition.attributes, "CopyOnAssign") {
        return Ok(None);
    }
    let type_name = definition.name.node.name.as_str();
    let hook = definition.methods.iter().find(|method| {
        let method = &method.node;
        method.name.node.name == "Copy"
            && method.parameters.is_empty()
            && method.generics.is_empty()
            && method.receiver.as_ref().is_none_or(|receiver| !receiver.node.mutable && !receiver.node.movable)
            && method.return_type.as_ref().is_some_and(|result| returns_owner(&result.node, type_name))
    });
    let Some(hook) = hook else {
        return Err(SemanticError::new(format!(
            "`[CopyOnAssign]` type `{type_name}` needs a method `{type_name} Copy()` with a borrowed receiver and no \
             parameters"
        )));
    };
    let hook = index
        .direct_child_id(program, declaration.node, DynNodeRef::from(hook))
        .map(|node| AstNodeKey { node, ..declaration })
        .ok_or_else(|| SemanticError::unavailable_at("value_copy_specialization", declaration))?;
    let generics = definition.generics.iter().map(|generic| Arc::<str>::from(generic.node.name.as_str())).collect();
    Ok(Some((hook, generics)))
}

/// Whether a `Copy` result type names its owner (`This`, `List<T>`).
fn returns_owner(result: &beskid_analysis::syntax::Type, type_name: &str) -> bool {
    match result {
        beskid_analysis::syntax::Type::Complex(path) => {
            path.node.segments.last().is_some_and(|segment| segment.node.name.node.name == type_name)
        }
        beskid_analysis::syntax::Type::This => true,
        _ => false,
    }
}

fn copy_position(db: &dyn Db, key: AstNodeKey, program: &Program, index: &SyntaxIndex) -> Option<ValueCopyPosition> {
    let place = key.node;
    if !matches!(
        index.kind(place)?,
        NodeKind::PathExpression | NodeKind::MemberExpression | NodeKind::IndexExpression | NodeKind::TryExpression
    ) {
        return None;
    }
    let child_of = |parent, target: DynNodeRef<'_>| index.direct_child_id(program, parent, target);
    let mut child = place;
    loop {
        let parent = parent_node(index, child)?;
        let syntax = index.node_at(program, parent)?;
        match index.kind(parent)? {
            NodeKind::Expression | NodeKind::GroupedExpression => {}
            NodeKind::LetStatement => return Some(ValueCopyPosition::Binding),
            NodeKind::AssignExpression => {
                let assignment = syntax.of::<beskid_analysis::syntax::AssignExpression>()?;
                if child_of(parent, DynNodeRef::from(assignment.target.as_ref())) == Some(child) {
                    return None;
                }
                return Some(ValueCopyPosition::Assignment);
            }
            NodeKind::CallExpression => {
                let call = syntax.of::<beskid_analysis::syntax::CallExpression>()?;
                let position = call
                    .args
                    .iter()
                    .position(|argument| child_of(parent, DynNodeRef::from(argument)) == Some(child))?;
                let call_key = AstNodeKey { node: parent, ..key };
                // `exchange(place, move value)` writes its first argument in place.
                if position == 0 && exchange_call_for_node(program, index, call_key).is_some() {
                    return None;
                }
                let shared = direct_callee(db, call_key)
                    .and_then(|declaration| declared_parameters(db, declaration))
                    .and_then(|(parameters, _)| parameters.get(position).map(|parameter| parameter.node.mutable))
                    .unwrap_or(false);
                return (!shared).then_some(ValueCopyPosition::Argument);
            }
            NodeKind::StructLiteralField | NodeKind::ArrayLiteralExpression | NodeKind::EnumConstructorExpression => {
                return Some(ValueCopyPosition::Construction);
            }
            NodeKind::ReturnStatement | NodeKind::LambdaExpression => {
                return return_copies(program, index, place).then_some(ValueCopyPosition::Return);
            }
            // A match arm's value flows to wherever the match expression's value goes.
            NodeKind::MatchArm => {
                let arm = syntax.of::<beskid_analysis::syntax::MatchArm>()?;
                if child_of(parent, DynNodeRef::from(&arm.value)) != Some(child) {
                    return None;
                }
            }
            NodeKind::MatchExpression => {
                if index.kind(child)? != NodeKind::MatchArm {
                    return None;
                }
            }
            _ => return None,
        }
        child = parent;
    }
}

/// A returned place copies unless the callable owns it: a `let` local, or a parameter that is
/// not `mut` (its argument was already copied at the call).
fn return_copies(program: &Program, index: &SyntaxIndex, place: beskid_analysis::syntax::AstNodeId) -> bool {
    let Some(path) =
        index.node_at(program, place).and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
    else {
        return true;
    };
    let [segment] = path.path.node.segments.as_slice() else {
        return true;
    };
    let Some(declaration) = resolve_lexical_declaration(program, index, place, &segment.node.name.node.name) else {
        return true;
    };
    match parent_node(index, declaration).and_then(|declaring| index.kind(declaring)) {
        Some(NodeKind::LetStatement) => false,
        Some(NodeKind::Parameter) => local_declaration_is_mutable(program, index, declaration),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BeskidDatabase, ProjectSession, build_typed_program};
    use beskid_analysis::projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
    };
    use beskid_analysis::services::parse_program_with_source_name;

    fn assembly(source: &str, generation: u64) -> (BeskidDatabase, AstNodeKey) {
        let generation = SyntaxGenerationId(generation);
        let mut db = BeskidDatabase::default();
        let directory = tempfile::tempdir().expect("project").keep();
        let source_path = directory.join("Main.bd");
        std::fs::write(&source_path, source).expect("source");
        let program =
            parse_program_with_source_name(source_path.to_str().expect("path"), source).expect("parse source");
        let entry = SourceUnitId::new(&db, source_path.clone());
        let project = ProjectSession::new(&db, directory.clone(), source_path.clone(), "App".into(), "lock".into());
        let assembly = Arc::new(ProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry { dependency_name: None, source_root: directory },
                dependencies: Vec::new(),
            },
            Arc::new(vec![SourceUnit {
                logical_name: "Main".into(),
                origin_path: source_path.clone(),
                path: source_path,
                source: source.into(),
                program,
            }]),
            0,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false,
            generation,
        ));
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
        (db, AstNodeKey { unit: entry, generation, node: beskid_analysis::syntax::AstNodeId(0) })
    }

    /// The place expression whose source text is exactly `text` (its first occurrence).
    fn place(db: &dyn Db, source: &str, key: AstNodeKey, text: &str) -> Option<AstNodeKey> {
        let kind = node_kind(db, key).expect("node kind");
        if matches!(
            kind,
            Some(
                IndexedNodeKind::PathExpression | IndexedNodeKind::MemberExpression | IndexedNodeKind::IndexExpression
            )
        ) && (node_span(db, key)
            .ok()
            .flatten()
            .is_some_and(|span| source.get(span.start..span.end).map(str::trim) == Some(text))
            || path_spelling(db, key).as_deref() == Some(text))
        {
            return Some(key);
        }
        child_nodes(db, key).expect("child nodes")?.iter().find_map(|child| place(db, source, *child, text))
    }

    /// `a.b` for a path expression with segments `a` and `b`.
    fn path_spelling(db: &dyn Db, key: AstNodeKey) -> Option<String> {
        let syntax = db.syntax_unit(key.unit)?;
        let path = syntax
            .syntax_index(db)
            .node_at(syntax.expanded_program(db), key.node)?
            .of::<beskid_analysis::syntax::PathExpression>()?;
        Some(
            path.path
                .node
                .segments
                .iter()
                .map(|segment| segment.node.name.node.name.as_str())
                .collect::<Vec<_>>()
                .join("."),
        )
    }

    const BAG: &str = "[CopyOnAssign] pub type Bag<T> { T[] items; i64 count; \
        pub Bag<T> Copy() { return Bag<T> { items: this.items, count: this.count }; } \
        pub unit Put(mut this, T value) { this.count = this.count + 1_i64; return; } } \
        pub type Holder { Bag<i64> bag; } \
        unit Keep(Bag<i64> kept) { return; } \
        unit Share(mut Bag<i64> shared) { return; } \
        Bag<i64> Own(Bag<i64> owned) { return owned; } \
        Bag<i64> Lend(mut Bag<i64> lent) { return lent; } \
        Bag<i64> Field(Holder holder) { return holder.bag; } ";

    const MAIN: &str = "i64 Main(Bag<i64> bound, Bag<i64> assigned, Bag<i64> passed, Bag<i64> sharedArg, \
        Bag<i64> receiver, Bag<i64> built, Bag<i64> counted, Bag<i64> moved) { \
        Bag<i64> b = bound; \
        mut Bag<i64> c = b; \
        c = assigned; \
        Keep(passed); \
        Share(sharedArg); \
        receiver.Put(1_i64); \
        Holder made = Holder { bag: built }; \
        Keep(move moved); \
        i64 n = counted.count; \
        return n; }";

    #[test]
    fn copy_positions_follow_the_destination_of_a_place_read() {
        let source = format!("{BAG}{MAIN}");
        let (db, root) = assembly(&source, 6700);
        let position = |text: &str| {
            let key = place(&db, &source, root, text).unwrap_or_else(|| panic!("place `{text}`"));
            value_copy_position(&db, key).expect("copy position")
        };
        assert_eq!(position("bound"), Some(ValueCopyPosition::Binding));
        assert_eq!(position("assigned"), Some(ValueCopyPosition::Assignment));
        assert_eq!(position("passed"), Some(ValueCopyPosition::Argument));
        assert_eq!(position("built"), Some(ValueCopyPosition::Construction));
        assert_eq!(position("sharedArg"), None, "a `mut` parameter shares the caller's value");
        assert_eq!(position("moved"), None, "a `move` operand hands the value over");
        assert_eq!(position("owned"), None, "a parameter that is not `mut` is owned by the callee");
        assert_eq!(position("lent"), Some(ValueCopyPosition::Return), "a returned `mut` parameter is shared");
        assert_eq!(position("holder.bag"), Some(ValueCopyPosition::Return), "a returned field is shared");
        assert_eq!(position("counted.count"), Some(ValueCopyPosition::Binding));
        let receiver = place(&db, &source, root, "receiver.Put").or_else(|| place(&db, &source, root, "receiver"));
        assert_eq!(value_copy_position(&db, receiver.expect("receiver")).expect("copy position"), None);
    }

    #[test]
    fn a_copy_position_calls_the_copy_hook_only_for_a_copy_on_assign_type() {
        let source = format!("{BAG}{MAIN}");
        let (db, root) = assembly(&source, 6701);
        for text in ["bound", "assigned", "passed", "built", "holder.bag"] {
            let key = place(&db, &source, root, text).expect("place");
            let instance = value_copy_specialization(&db, key, None)
                .expect("copy specialization")
                .unwrap_or_else(|| panic!("`{text}` calls the hook"));
            assert_eq!(item_name(&db, instance.declaration).expect("hook name").as_deref(), Some("Copy"), "{text}");
            let substitutions = instance
                .substitutions
                .iter()
                .map(|binding| format!("{}={}", binding.parameter, binding.argument.display_name()))
                .collect::<Vec<_>>();
            assert_eq!(substitutions, ["T=i64"], "{text}");
            assert_eq!(instance.signature.parameters.as_ref(), [SemanticTypeId::POINTER], "{text}");
        }
        let count = place(&db, &source, root, "counted.count").expect("member read");
        assert_eq!(value_copy_specialization(&db, count, None).expect("scalar read"), None, "an i64 is not copied");
    }

    #[test]
    fn a_copy_on_assign_type_without_a_copy_method_is_rejected() {
        let source = "[CopyOnAssign] pub type Raw { i64 x; } i64 Main(Raw r) { Raw s = r; return s.x; }";
        let (db, root) = assembly(source, 6702);
        let key = place(&db, source, root, "r").expect("place");
        let error = value_copy_specialization(&db, key, None).expect_err("missing hook");
        assert!(!error.is_unavailable(), "{error}");
        assert!(error.to_string().contains("Raw Copy()"), "{error}");
    }

    #[test]
    fn a_type_without_the_attribute_is_shared() {
        let source = "pub type Plain { i64 x; pub Plain Copy() { return Plain { x: this.x }; } } \
            i64 Main(Plain p) { Plain q = p; return q.x; }";
        let (db, root) = assembly(source, 6703);
        let key = place(&db, source, root, "p").expect("place");
        assert_eq!(value_copy_position(&db, key).expect("position"), Some(ValueCopyPosition::Binding));
        assert_eq!(value_copy_specialization(&db, key, None).expect("specialization"), None);
    }

    fn first_of_kind(db: &dyn Db, key: AstNodeKey, kind: IndexedNodeKind) -> Option<AstNodeKey> {
        if node_kind(db, key).expect("node kind") == Some(kind) {
            return Some(key);
        }
        child_nodes(db, key).expect("child nodes")?.iter().find_map(|child| first_of_kind(db, *child, kind))
    }

    #[test]
    fn a_for_element_a_capture_and_a_try_payload_are_new_names() {
        let source = format!(
            "{BAG}i64 Loop(Bag<i64>[] bags) {{ for b in bags {{ b.Put(1_i64); }} return 0_i64; }} \
             i64 Capture(Bag<i64> captured) {{ let f = () => captured.count; return f(); }} \
             i64 Main() {{ return 0_i64; }}"
        );
        let (db, root) = assembly(&source, 6704);
        let for_statement = first_of_kind(&db, root, IndexedNodeKind::ForStatement).expect("for statement");
        let element = for_element_copy_specialization(&db, for_statement, None)
            .expect("for element copy")
            .expect("the loop variable is a copy of the element");
        assert_eq!(item_name(&db, element.declaration).expect("hook name").as_deref(), Some("Copy"));
        let lambda = first_of_kind(&db, root, IndexedNodeKind::LambdaExpression).expect("lambda");
        let environment = closure_environment(&db, lambda).expect("closure environment").expect("captures");
        let capture = environment.captures.first().expect("one capture");
        let copied = capture_copy_specialization(&db, capture.declaration, None)
            .expect("capture copy")
            .expect("a captured list is a copy");
        assert_eq!(item_name(&db, copied.declaration).expect("hook name").as_deref(), Some("Copy"));
    }

    #[test]
    fn a_try_payload_in_a_binding_is_a_copy_position() {
        let source = format!(
            "{BAG}Core.Results.Result<Bag<i64>, i64> Make(Bag<i64> b) {{ return Core.Results.Result::Ok(b); }} \
             Core.Results.Result<i64, i64> Main(Bag<i64> a) {{ Bag<i64> taken = Make(a)?; return Core.Results.Result::Ok(taken.count); }}"
        );
        let (db, root) = assembly(&source, 6705);
        let try_expression = first_of_kind(&db, root, IndexedNodeKind::TryExpression).expect("try expression");
        assert_eq!(value_copy_position(&db, try_expression).expect("position"), Some(ValueCopyPosition::Binding));
    }

    #[test]
    fn a_match_pattern_binding_is_a_new_name() {
        let source = format!(
            "{BAG}pub enum Slot {{ Full(Bag<i64> bag), Empty }} \
             i64 Main(Slot slot) {{ return match slot {{ Slot::Full(held) => held.count, Slot::Empty => 0_i64, }}; }}"
        );
        let (db, root) = assembly(&source, 6706);
        fn identifiers(db: &dyn Db, key: AstNodeKey, found: &mut Vec<AstNodeKey>) {
            if node_kind(db, key).expect("node kind") == Some(IndexedNodeKind::Identifier) {
                found.push(key);
            }
            for child in child_nodes(db, key).expect("child nodes").iter().flat_map(|children| children.iter()) {
                identifiers(db, *child, found);
            }
        }
        let matched = first_of_kind(&db, root, IndexedNodeKind::MatchExpression).expect("match");
        let mut names = Vec::new();
        identifiers(&db, matched, &mut names);
        let copies = names
            .iter()
            .filter_map(|name| pattern_binding_copy_specialization(&db, *name, None).expect("pattern binding copy"))
            .collect::<Vec<_>>();
        assert_eq!(copies.len(), 1, "only the binding `held` is a new name");
        let copied = &copies[0];
        assert_eq!(item_name(&db, copied.declaration).expect("hook name").as_deref(), Some("Copy"));
        let not_a_binding = first_of_kind(&db, root, IndexedNodeKind::LetStatement)
            .or_else(|| first_of_kind(&db, root, IndexedNodeKind::FunctionDefinition))
            .expect("other node");
        assert_eq!(pattern_binding_copy_specialization(&db, not_a_binding, None).expect("not a binding"), None);
    }
}
