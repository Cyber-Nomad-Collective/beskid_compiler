//! Legacy checker false positives found by `beskid check` on Corelib (v0.6 rc17). Production
//! semantics (`beskid_queries`) accept every program below; the legacy resolver and checker must
//! agree.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::resolve::span_index::span_index_from_tables;
use crate::resolve::{ItemKind, ResolvedType, Resolver};
use crate::services::{SemanticFactsError, parse_program, resolve_and_type_program};
use crate::syntax::{Node, SpanInfo};
use crate::types::build_unit_type_surface;
use crate::types::checker::TypeChecker;
use crate::types::result::{CallLoweringKind, TypeError};
use crate::types::{TypeInfo, UnitTypeSurface};

fn type_errors(source: &str) -> Vec<TypeError> {
    let program = parse_program(source).expect("source parses");
    match resolve_and_type_program(&program) {
        Ok(_) => Vec::new(),
        Err(SemanticFactsError::Type { errors, .. }) => errors,
        Err(other) => panic!("resolution must succeed: {other:?}"),
    }
}

/// `HttpServer.Accept()` calling `tcpListener.Accept(deadline)`: a receiver-qualified call binds
/// to the local's own type even when a dependency type definition covers the enclosing method's
/// byte range in the merged, source-less span index (which once registered the enclosing method
/// under that foreign receiver).
#[test]
fn v06_receiver_qualified_call_is_not_bound_to_the_enclosing_same_named_method() {
    let source = r#"
pub type Listener {
    i64 handle,

    pub i64 Accept(i64 deadline) { return handle + deadline; }
}

pub type Server {
    Listener listener,

    pub i64 Accept() {
        Listener current = this.listener;
        return current.Accept(1_i64);
    }
}
"#;
    let entry_path = PathBuf::from("/tmp/v06-rc17-receiver/src/Http/Server.bd");
    let mut program = parse_program(source).expect("source parses");
    let mut resolver = Resolver::new();
    resolver.set_current_source_path(Some(entry_path.clone()));
    let mut resolution = resolver.resolve_program(&program).expect("source resolves");

    let listener = resolution
        .items
        .iter()
        .find(|item| item.kind == ItemKind::Type && item.name == "Listener")
        .expect("Listener item")
        .id;
    let Node::TypeDefinition(server) = &program.node.items[1].node else { panic!("Server type expected") };
    let accept = server.node.methods[0].span;
    // Another unit's `Listener` definition at exactly the same byte range as `Server.Accept`.
    let foreign = SpanInfo { start: accept.start, end: accept.end, ..SpanInfo::default() };
    resolution.tables.scoped_resolved_types.insert(
        PathBuf::from("/tmp/v06-rc17-receiver/dep/Network/Tcp/Listener.bd"),
        HashMap::from([(foreign, ResolvedType::Item(listener))]),
    );
    resolution.span_index = span_index_from_tables(&resolution.tables);

    let (_, errors) =
        TypeChecker::check_entry(&mut program, &resolution, &[], None, Some(entry_path), false, None, None, None, None);
    assert!(errors.is_empty(), "`current.Accept(1_i64)` must use `Listener.Accept(i64)`: {errors:#?}");
}

#[test]
fn v06_bare_sibling_rule_never_takes_a_receiver_qualified_call() {
    let source = r#"
pub type Listener {
    i64 handle,

    pub i64 Accept(i64 deadline) { return handle + deadline; }
}

pub type Server {
    Listener listener,

    pub i64 Accept() {
        Listener current = this.listener;
        return current.Accept(1_i64) + this.listener.Accept(2_i64);
    }
}
"#;
    let errors = type_errors(source);
    assert!(errors.is_empty(), "qualified calls keep their receiver: {errors:#?}");
}

/// `return Result::Ok(Option::Some(0_i64));` in a method returning `Result<Option<i64>, E>`.
#[test]
fn v06_nested_generic_variant_is_instantiated_from_its_expected_type_and_payload() {
    let source = r#"
pub enum Option<T> {
    Some(T value),
    None,
}

pub enum Result<TValue, TError> {
    Ok(TValue value),
    Error(TError error),
}

pub enum Fault {
    Failed,
}

pub Result<Option<i64>, Fault> Ready() {
    return Result::Ok(Option::Some(0_i64));
}

pub Result<Option<i64>, Fault> Pending() {
    return Result::Ok(Option::None);
}

pub Result<Option<i64>, Fault> Failed() {
    return Result::Error(Fault::Failed);
}

pub unit Unanchored() {
    Option::Some(Option::Some(1_i64));
    return;
}
"#;
    let errors = type_errors(source);
    assert!(errors.is_empty(), "nested generic variants must instantiate their own generics: {errors:#?}");
}

#[test]
fn v06_nested_generic_variant_payload_mismatch_is_still_reported() {
    let source = r#"
pub enum Option<T> {
    Some(T value),
    None,
}

pub enum Result<TValue, TError> {
    Ok(TValue value),
    Error(TError error),
}

pub enum Fault {
    Failed,
}

pub Result<Option<i64>, Fault> Wrong() {
    return Result::Ok(Option::Some(true));
}
"#;
    let errors = type_errors(source);
    assert!(
        errors.iter().any(|error| matches!(error, TypeError::TypeMismatch { .. })),
        "a bool payload for `Option<i64>` must mismatch: {errors:#?}"
    );
}

/// `pub unit Walk<V>(NodeRef root, V visitor) where V: SyntaxVisitor { visitor.Enter(root); }`.
#[test]
fn v06_where_bounded_generic_value_dispatches_through_its_contract() {
    let source = r#"
pub type Node {
    i64 id,
}

pub contract Visitor {
    unit Enter(Node node);
    unit Exit(Node node);
}

pub unit Walk<V>(Node root, V visitor) where V: Visitor {
    visitor.Enter(root);
    visitor.Exit(root);
    return;
}
"#;
    let program = parse_program(source).expect("source parses");
    let (_, _, typed) = match resolve_and_type_program(&program) {
        Ok(result) => result,
        Err(error) => panic!("bounded contract calls must type-check: {error:?}"),
    };
    let dispatches = typed
        .lowering
        .call_kinds
        .values()
        .filter(|kind| matches!(kind, CallLoweringKind::ContractDispatch { .. }))
        .count();
    assert_eq!(dispatches, 2, "both bounded calls dispatch through `Visitor`");
}

#[test]
fn v06_unbounded_generic_value_has_no_contract_members() {
    let source = r#"
pub type Node {
    i64 id,
}

pub contract Visitor {
    unit Enter(Node node);
}

pub unit Walk<V>(Node root, V visitor) {
    visitor.Enter(root);
    return;
}
"#;
    let errors = type_errors(source);
    assert!(!errors.is_empty(), "a generic parameter without a bound exposes no contract method");
}

#[test]
fn v06_bounded_contract_method_arity_is_checked() {
    let source = r#"
pub type Node {
    i64 id,
}

pub contract Visitor {
    unit Enter(Node node);
}

pub unit Walk<V>(Node root, V visitor) where V: Visitor {
    visitor.Enter(root, root);
    return;
}
"#;
    let errors = type_errors(source);
    assert!(
        errors.iter().any(|error| matches!(error, TypeError::CallArityMismatch { expected: 1, actual: 2, .. })),
        "{errors:#?}"
    );
}

/// `Platform/Terminal.bd` declares `pub Console.ConsoleSize QuerySize()` while `Console.bd` is the
/// entry: the entry's declarations stay reachable by its logical module path.
#[test]
fn v06_sibling_unit_signature_names_the_entry_by_its_module_path() {
    let terminal_path = PathBuf::from("/tmp/v06-rc17-console/src/Platform/Terminal.bd");
    let entry_path = PathBuf::from("/tmp/v06-rc17-console/src/Console.bd");
    let terminal = parse_program(
        "pub Console.ConsoleSize QuerySize() { return Console.ConsoleSize { columns: 80_i32, rows: 24_i32 }; }\n\
         pub unit PollResize(Console.ConsoleSize lastSize) { return; }",
    )
    .expect("terminal parses");
    let mut entry = parse_program(
        "use Platform.Terminal;\n\
         pub type ConsoleSize { pub i32 columns, pub i32 rows, }\n\
         pub ConsoleSize QuerySize() { return Platform.Terminal.QuerySize(); }\n\
         pub unit RunTick(ConsoleSize lastSize) { Platform.Terminal.PollResize(lastSize); return; }",
    )
    .expect("entry parses");

    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(&terminal, &["Platform".into(), "Terminal".into()], Some(&terminal_path));
    resolver.set_current_source_path(Some(entry_path.clone()));
    let resolution =
        resolver.resolve_entry_program_in_module(&entry, Some(&["Console".to_string()])).expect("entry resolves");

    let console_size = resolution
        .items
        .iter()
        .find(|item| item.kind == ItemKind::Type && item.name == "ConsoleSize")
        .expect("ConsoleSize item")
        .id;
    let surface: UnitTypeSurface = build_unit_type_surface(&terminal, &resolution, &terminal_path);
    // `Platform.Terminal` is collected before the entry, so its `QuerySize` has the lower id.
    let query_size = resolution
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Function && item.name == "QuerySize")
        .map(|item| item.id)
        .min_by_key(|id| id.0)
        .expect("Terminal.QuerySize item");
    let signature = surface.function_signatures.get(&query_size).expect("QuerySize signature");
    assert!(
        matches!(surface.types.get(signature.return_type), Some(TypeInfo::Named(id)) if *id == console_size),
        "`Console.ConsoleSize` must name the entry's type, not a placeholder"
    );

    let dependency_paths = [terminal_path];
    let (_, errors) = TypeChecker::check_entry(
        &mut entry,
        &resolution,
        &[&terminal],
        Some(&dependency_paths),
        Some(entry_path),
        false,
        None,
        None,
        None,
        None,
    );
    assert!(errors.is_empty(), "qualified sibling calls keep their real signatures: {errors:#?}");
}

/// `use Beskid.Compiler.Workspace;` with the type `Beskid.Compiler.Workspace workspace`: the
/// qualified type path names the imported module's homonymous item, so the import is used.
#[test]
fn v06_qualified_type_path_naming_the_imported_module_item_marks_the_import_used() {
    let workspace_path = PathBuf::from("/tmp/v06-rc17-import/Beskid/Compiler/Workspace.bd");
    let workspace = parse_program("pub type Workspace { u64 generation, }").expect("module parses");
    let entry =
        parse_program("use Beskid.Compiler.Workspace;\npub type Request { Beskid.Compiler.Workspace workspace, }")
            .expect("entry parses");
    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(
        &workspace,
        &["Beskid".into(), "Compiler".into(), "Workspace".into()],
        Some(&workspace_path),
    );
    let resolution = resolver.resolve_program(&entry).expect("qualified type resolves");
    let Node::UseDeclaration(use_declaration) = &entry.node.items[0].node else { panic!("use expected") };
    assert!(
        resolution.tables.used_import_spans.contains(&use_declaration.node.path.span),
        "`Beskid.Compiler.Workspace` goes through the imported module, so its `use` is used"
    );
}

/// A qualified path that only shares a prefix spelling with an imported module, without naming its
/// homonymous item, does not credit that import.
#[test]
fn v06_unrelated_qualified_type_path_leaves_the_import_unused() {
    let workspace_path = PathBuf::from("/tmp/v06-rc17-import-unused/Beskid/Compiler/Workspace.bd");
    let other_path = PathBuf::from("/tmp/v06-rc17-import-unused/Beskid/Compiler/Other.bd");
    let workspace = parse_program("pub type Workspace { u64 generation, }").expect("module parses");
    let other = parse_program("pub type Other { u64 generation, }").expect("module parses");
    let entry = parse_program("use Beskid.Compiler.Workspace;\npub type Request { Beskid.Compiler.Other other, }")
        .expect("entry parses");
    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(
        &workspace,
        &["Beskid".into(), "Compiler".into(), "Workspace".into()],
        Some(&workspace_path),
    );
    resolver.collect_program_in_module(
        &other,
        &["Beskid".into(), "Compiler".into(), "Other".into()],
        Some(&other_path),
    );
    let resolution = resolver.resolve_program(&entry).expect("qualified type resolves");
    let Node::UseDeclaration(use_declaration) = &entry.node.items[0].node else { panic!("use expected") };
    assert!(!resolution.tables.used_import_spans.contains(&use_declaration.node.path.span));
}

/// `serialization_mod` `Syntax.bd` shape: `use Beskid.Compiler.Semantic;` (exports `pub type
/// Field`) and `use Beskid.Syntax.Nodes;` (declares only the module `pub mod
/// Beskid.Syntax.Nodes.Field;`, whose unit exports the syntax `Field`) are both imported. In type
/// position a module is never a type: the bare `Field` resolves to a type item, never the module
/// item, and the qualified `Beskid.Syntax.Nodes.Field` resolves to the module's homonymous type,
/// as `beskid_queries::resolve_type_declaration` does. The entry is resolved with its logical
/// module path, exactly as `ModuleIndex::resolve_entry_program` does.
#[test]
fn v06_type_positions_beside_a_same_named_imported_module_resolve_to_types() {
    let root = PathBuf::from("/tmp/v06-rc17-field");
    let semantic_path = root.join("sdk/Beskid/Compiler/Semantic.bd");
    let field_path = root.join("sdk/Beskid/Syntax/Nodes/Field.bd");
    let nodes_path = root.join("sdk/Beskid/Syntax/Nodes.bd");
    let entry_path = root.join("mod/Src/SerializationMod/Syntax.bd");
    let semantic = parse_program("pub type Field { pub u64 token, }").expect("semantic parses");
    let field = parse_program("pub type Field { pub u64 id, }").expect("syntax field parses");
    let nodes = parse_program("pub mod Beskid.Syntax.Nodes.Field;").expect("nodes parses");
    let mut entry = parse_program(
        "use Beskid.Compiler.Semantic;\nuse Beskid.Syntax.Nodes;\n\
         pub u64 Token(Field field) { Field value = field; return value.token; }\n\
         pub u64 Id(Beskid.Syntax.Nodes.Field node) { Beskid.Syntax.Nodes.Field value = node; return value.id; }",
    )
    .expect("entry parses");

    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(
        &semantic,
        &["Beskid".into(), "Compiler".into(), "Semantic".into()],
        Some(&semantic_path),
    );
    resolver.collect_program_in_module(
        &field,
        &["Beskid".into(), "Syntax".into(), "Nodes".into(), "Field".into()],
        Some(&field_path),
    );
    resolver.collect_program_in_module(&nodes, &["Beskid".into(), "Syntax".into(), "Nodes".into()], Some(&nodes_path));
    resolver.set_current_source_path(Some(entry_path.clone()));
    let resolution = resolver
        .resolve_entry_program_in_module(&entry, Some(&["SerializationMod".to_string(), "Syntax".to_string()]))
        .unwrap_or_else(|errors| panic!("type positions must resolve to types: {errors:?}"));

    // `Semantic.bd` is collected before `Field.bd`.
    let mut field_types = resolution
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Type && item.name == "Field")
        .map(|item| item.id)
        .collect::<Vec<_>>();
    field_types.sort_by_key(|id| id.0);
    let [_, syntax_field] = field_types.as_slice() else { panic!("two `Field` types expected: {field_types:?}") };
    let parameter_type = |index: usize| {
        let Node::Function(function) = &entry.node.items[index].node else { panic!("function expected") };
        let crate::syntax::Type::Complex(path) = &function.node.parameters[0].node.ty.node else {
            panic!("nominal parameter type expected")
        };
        resolution.tables.resolved_types.get(&path.span).cloned()
    };
    let Some(ResolvedType::Item(bare)) = parameter_type(2) else { panic!("bare `Field` must resolve") };
    assert!(
        resolution.items.get(bare.0).is_some_and(|item| item.kind == ItemKind::Type),
        "the module `Beskid.Syntax.Nodes.Field` is not a type and must not take the bare name"
    );
    assert_eq!(parameter_type(3), Some(ResolvedType::Item(*syntax_field)));

    let dependency_paths = [semantic_path, field_path, nodes_path];
    let (_, errors) = TypeChecker::check_entry(
        &mut entry,
        &resolution,
        &[&semantic, &field, &nodes],
        Some(&dependency_paths),
        Some(entry_path),
        false,
        None,
        None,
        None,
        None,
    );
    assert!(errors.is_empty(), "{errors:#?}");
}
