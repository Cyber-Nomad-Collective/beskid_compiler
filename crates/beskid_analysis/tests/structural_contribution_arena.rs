//! Serialization's structural Mod boundary: static AST, owner/generation/kind and bounds.
use beskid_analysis::mod_host::{StructuralContributionArena, StructuralContributionBounds, StructuralProvenance};
use beskid_analysis::mod_host::{StructuralContributionItem, StructuralContributionTag};
use beskid_analysis::syntax::{
    Block, Expression, Field, FieldKind, FunctionDefinition, GroupedExpression, Identifier, Literal, LiteralExpression,
    Node, Path, PathSegment, PrimitiveType, ReturnStatement, SpanInfo, Spanned, Statement, Type, TypeDefinition,
    Visibility,
};

fn span() -> SpanInfo {
    SpanInfo { start: 11, end: 29, line_col_start: (2, 1), line_col_end: (2, 19) }
}
fn node<T>(value: T) -> Spanned<T> {
    Spanned::new(value, span())
}
fn name(value: &str) -> Spanned<Identifier> {
    node(Identifier { name: value.to_owned() })
}
fn provenance() -> StructuralProvenance {
    StructuralProvenance {
        source_name: "Configuration.bd".to_owned(),
        origin_generation: 17,
        generator_type_id: "Serialization.AdapterGenerator".to_owned(),
        origin_span: span(),
    }
}
fn function() -> Spanned<Node> {
    node(Node::Function(node(FunctionDefinition {
        attributes: vec![],
        visibility: node(Visibility::Public),
        name: name("EncodeConfiguration"),
        generics: vec![],
        where_bounds: vec![],
        parameters: vec![],
        parameter_docs: vec![],
        return_type: Some(node(Type::Primitive(node(PrimitiveType::Unit)))),
        body: node(Block { statements: vec![] }),
    })))
}
fn generic_record() -> Spanned<Node> {
    node(Node::TypeDefinition(node(TypeDefinition {
        attributes: vec![],
        visibility: node(Visibility::Public),
        name: name("SerializedRecord"),
        generics: vec![name("T")],
        conformances: vec![],
        fields: vec![node(Field {
            attributes: vec![],
            visibility: node(Visibility::Public),
            kind: FieldKind::Value,
            event_capacity: None,
            inject_qualifier: None,
            name: name("Value"),
            ty: node(Type::Complex(node(Path {
                segments: vec![node(PathSegment { name: name("T"), type_args: vec![] })],
            }))),
        })],
        field_docs: vec![None],
        methods: vec![],
        method_docs: vec![],
        associated_type_bindings: vec![],
    })))
}
fn provenance_at(generation: u64) -> StructuralProvenance {
    StructuralProvenance { origin_generation: generation, ..provenance() }
}
fn arena(generation: u64) -> StructuralContributionArena {
    StructuralContributionArena::new(
        "Serialization.AdapterGenerator".to_owned(),
        generation,
        StructuralContributionBounds { max_items: 4, max_nodes: 128, max_depth: 32 },
    )
    .expect("valid structural arena")
}

#[test]
fn static_function_and_generic_record_materialize_without_source() {
    let arena = arena(17);
    let generated_function = function();
    let generated_record = generic_record();
    let function_handle = arena.insert(generated_function.clone(), provenance()).expect("function handle");
    let record_handle = arena.insert(generated_record.clone(), provenance()).expect("record handle");
    let items = arena
        .materialize(&[function_handle, record_handle], "Serialization.AdapterGenerator", 17)
        .expect("static structural materialization");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].item, generated_function);
    assert_eq!(items[1].item, generated_record);
    assert_eq!(items[0].provenance, provenance());
    assert_eq!(items[1].provenance.origin_span, span());
}

#[test]
fn structural_arena_enforces_aggregate_node_budget() {
    let arena = StructuralContributionArena::new(
        "Serialization.AdapterGenerator".to_owned(),
        17,
        StructuralContributionBounds { max_items: 2048, max_nodes: 4096, max_depth: 32 },
    )
    .unwrap();
    // Each fixture contains more than four real AST nodes. All 1024 individually
    // fit, but the arena cannot publish them all under a 4096-node total budget.
    let mut reached_budget = false;
    for _ in 0..1024 {
        if arena.insert(function(), provenance()).is_err() {
            reached_budget = true;
            break;
        }
    }
    assert!(
        reached_budget,
        "arena node limit must bound all published contributions together, not each item independently"
    );
}

fn fixture_node_count() -> usize {
    let item = function();
    let mut pending = vec![beskid_analysis::syntax_query::DynNodeRef::from(&item)];
    let mut count = 0;
    while let Some(node) = pending.pop() {
        count += 1;
        node.children(|child| pending.push(child));
    }
    count
}

#[test]
fn structural_arena_aggregate_boundary_is_atomic_and_preserves_published_handles() {
    let arena = StructuralContributionArena::new(
        "Serialization.AdapterGenerator".into(),
        17,
        StructuralContributionBounds { max_items: 4, max_nodes: fixture_node_count() * 2, max_depth: 32 },
    )
    .unwrap();
    let first = arena.insert(function(), provenance()).unwrap();
    let second = arena.insert(function(), provenance()).unwrap();
    assert!(arena.insert(function(), provenance()).is_err(), "third item exceeds invocation-wide node budget");
    assert_eq!(arena.materialize(&[first, second], "Serialization.AdapterGenerator", 17).unwrap().len(), 2);
}

#[test]
fn structural_arena_concurrent_insertions_share_one_aggregate_budget() {
    let arena = std::sync::Arc::new(
        StructuralContributionArena::new(
            "Serialization.AdapterGenerator".into(),
            17,
            StructuralContributionBounds { max_items: 4, max_nodes: fixture_node_count(), max_depth: 32 },
        )
        .unwrap(),
    );
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let threads = (0..4)
        .map(|_| {
            let arena = arena.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                arena.insert(function(), provenance()).is_ok()
            })
        })
        .collect::<Vec<_>>();
    let accepted = threads.into_iter().map(|thread| thread.join().unwrap()).filter(|accepted| *accepted).count();
    assert_eq!(accepted, 1, "all insertions must reserve the same locked node budget");
}

#[test]
fn structural_materialization_rejects_foreign_owner_generation_and_kind() {
    let arena = arena(17);
    let handle = arena.insert(function(), provenance()).expect("handle");
    assert!(arena.materialize(&[handle], "Other.Generator", 17).is_err());
    assert!(arena.materialize(&[handle], "Serialization.AdapterGenerator", 18).is_err());
    let wrong_kind = StructuralContributionItem { tag: StructuralContributionTag::TypeDefinition, node: handle.node };
    assert!(arena.materialize(&[wrong_kind], "Serialization.AdapterGenerator", 17).is_err());
    let foreign_arena = arena_from_owner("Other.Generator", 17);
    assert!(foreign_arena.materialize(&[handle], "Other.Generator", 17).is_err());
}

fn arena_from_owner(owner: &str, generation: u64) -> StructuralContributionArena {
    StructuralContributionArena::new(
        owner.to_owned(),
        generation,
        StructuralContributionBounds { max_items: 4, max_nodes: 128, max_depth: 32 },
    )
    .expect("valid arena")
}

#[test]
fn structural_arena_enforces_limits_before_publishing_handle() {
    let arena = StructuralContributionArena::new(
        "Serialization.AdapterGenerator".to_owned(),
        17,
        StructuralContributionBounds { max_items: 1, max_nodes: 128, max_depth: 32 },
    )
    .unwrap();
    let first = arena.insert(function(), provenance()).unwrap();
    assert!(arena.insert(generic_record(), provenance()).is_err());
    assert_eq!(arena.materialize(&[first], "Serialization.AdapterGenerator", 17).unwrap().len(), 1);
    assert!(arena.materialize(&[first, first], "Serialization.AdapterGenerator", 17).is_err());
    let shallow = StructuralContributionArena::new(
        "Serialization.AdapterGenerator".to_owned(),
        17,
        StructuralContributionBounds { max_items: 4, max_nodes: 1, max_depth: 1 },
    )
    .unwrap();
    assert!(shallow.insert(generic_record(), provenance()).is_err());
}

#[test]
fn closed_structural_arena_rejects_existing_and_new_handles() {
    let arena = arena(17);
    let handle = arena.insert(function(), provenance()).unwrap();
    arena.close().expect("close arena");
    assert!(arena.materialize(&[handle], "Serialization.AdapterGenerator", 17).is_err());
    assert!(arena.insert(function(), provenance()).is_err());
    arena.close().expect("repeated close is harmless");
}

/// A function whose body holds `statements` return statements.
fn function_with_statements(statements: usize) -> Spanned<Node> {
    let mut item = function();
    let Node::Function(definition) = &mut item.node else { unreachable!() };
    definition.node.body.node.statements =
        (0..statements).map(|_| node(Statement::Return(node(ReturnStatement { value: None })))).collect();
    item
}

/// A function returning `depth` nested grouped expressions around a literal.
fn function_with_nesting(depth: usize) -> Spanned<Node> {
    let mut expression = node(Expression::Literal(node(LiteralExpression { literal: node(Literal::Integer("1".into())) })));
    for _ in 0..depth {
        expression = node(Expression::Grouped(node(GroupedExpression { expr: Box::new(expression) })));
    }
    let mut item = function();
    let Node::Function(definition) = &mut item.node else { unreachable!() };
    definition.node.body.node.statements =
        vec![node(Statement::Return(node(ReturnStatement { value: Some(expression) })))];
    item
}

fn walk(item: &Spanned<Node>) -> (usize, usize) {
    let mut pending = vec![(beskid_analysis::syntax_query::DynNodeRef::from(item), 1usize)];
    let (mut nodes, mut depth) = (0, 0);
    while let Some((node, level)) = pending.pop() {
        nodes += 1;
        depth = depth.max(level);
        node.children(|child| pending.push((child, level + 1)));
    }
    (nodes, depth)
}

fn production_arena() -> StructuralContributionArena {
    StructuralContributionArena::new(
        "Serialization.AdapterGenerator".into(),
        17,
        StructuralContributionBounds::default(),
    )
    .expect("production bounds are valid")
}

#[test]
fn production_bounds_are_the_normative_arena_limits() {
    let bounds = StructuralContributionBounds::default();
    assert_eq!((bounds.max_items, bounds.max_nodes, bounds.max_depth), (4096, 65536, 128));
    for over in [
        StructuralContributionBounds { max_items: 4097, ..bounds },
        StructuralContributionBounds { max_nodes: 65537, ..bounds },
        StructuralContributionBounds { max_depth: 129, ..bounds },
        StructuralContributionBounds { max_items: 0, ..bounds },
    ] {
        assert!(StructuralContributionArena::new("Owner".into(), 17, over).is_err(), "{over:?}");
    }
    assert!(StructuralContributionArena::new("Owner".into(), 0, bounds).is_err(), "generation zero is not authority");
    assert!(StructuralContributionArena::new(String::new(), 17, bounds).is_err(), "an arena needs an owner");
}

#[test]
fn production_arena_admits_4096_items_and_rejects_the_4097th() {
    let arena = production_arena();
    let handles: Vec<_> = (0..4096).map(|_| arena.insert(function(), provenance()).expect("item within budget")).collect();
    assert!(arena.insert(function(), provenance()).is_err(), "item 4097 exceeds the invocation budget");
    assert_eq!(arena.materialize(&handles, "Serialization.AdapterGenerator", 17).unwrap().len(), 4096);
}

#[test]
fn production_arena_node_budget_is_exact_at_65536() {
    let (base, _) = walk(&function_with_statements(0));
    let (per_statement, _) = {
        let (with_one, depth) = walk(&function_with_statements(1));
        (with_one - base, depth)
    };
    assert!(per_statement > 0);
    // Largest body whose tree fits the budget, then grow it past the budget.
    let statements = (65536 - base) / per_statement;
    let fitting = function_with_statements(statements);
    assert!(walk(&fitting).0 <= 65536);
    let arena = production_arena();
    arena.insert(fitting, provenance()).expect("a tree within 65536 nodes is admitted");
    let over = function_with_statements(statements + 1);
    assert!(walk(&over).0 > 65536);
    assert!(production_arena().insert(over, provenance()).is_err(), "a tree past 65536 nodes is rejected");
    // The budget is per invocation: two halves that fit alone cannot both publish.
    let half = function_with_statements(statements / 2 + 1);
    let shared = production_arena();
    shared.insert(half.clone(), provenance()).expect("first half fits");
    assert!(shared.insert(half, provenance()).is_err(), "the aggregate passes 65536 nodes");
}

#[test]
fn production_arena_depth_budget_is_exact_at_128() {
    let mut nesting = 0;
    while walk(&function_with_nesting(nesting + 1)).1 <= 128 {
        nesting += 1;
    }
    let deepest = function_with_nesting(nesting);
    assert!(walk(&deepest).1 <= 128);
    production_arena().insert(deepest, provenance()).expect("depth within 128 is admitted");
    let deeper = function_with_nesting(nesting + 1);
    assert!(walk(&deeper).1 > 128);
    assert!(production_arena().insert(deeper, provenance()).is_err(), "depth past 128 is rejected");
}

#[test]
fn structural_lineage_must_name_the_arena_generation() {
    let arena = arena(17);
    assert!(arena.insert(function(), provenance_at(18)).is_err(), "lineage from another generation");
    let handle = arena.insert(function(), provenance_at(17)).expect("current lineage");
    assert_eq!(arena.materialize(&[handle], "Serialization.AdapterGenerator", 17).unwrap()[0].provenance.origin_generation, 17);
}
