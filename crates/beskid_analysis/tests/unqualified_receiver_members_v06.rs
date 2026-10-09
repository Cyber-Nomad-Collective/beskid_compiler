//! `beskid check` (legacy resolver + checker) agrees with the production semantic layer
//! (`beskid_queries::semantic_contract::calls::resolution::unqualified_enclosing_method_call`) on
//! bare sibling-method calls and bare receiver field reads inside type-body methods.

use beskid_analysis::resolve::{ResolveError, ResolvedValue};
use beskid_analysis::services::{SemanticFactsError, parse_program, resolve_and_type_program};
use beskid_analysis::syntax::{CallExpression, Expression};
use beskid_analysis::syntax_query::Query;

/// Bare `Name(...)` calls that the legacy resolver bound to the implicit receiver `this` in a
/// program the legacy checker accepts. The checker accepts such a callee only through its
/// sibling-method dispatch (`unqualified_sibling_method_call`): typed as an ordinary call, a
/// `this`-bound callee is not callable and a free-function binding would be an arity error.
///
/// Observed through resolution rather than `TypeResult::lowering.call_kinds`: that map is keyed
/// by `Spanned::id`, which `Program::parse` leaves at `AstNodeId::INVALID` (dense node ids live
/// only in `SyntaxIndex`), so the checker records no call kinds for parsed source.
fn method_dispatch_count(source: &str) -> usize {
    let program = parse_program(source).expect("source parses");
    let (program, resolution, _) = match resolve_and_type_program(&program) {
        Ok(result) => result,
        Err(SemanticFactsError::Resolve(errors)) => panic!("resolution failed: {errors:?}"),
        Err(SemanticFactsError::Type { errors, .. }) => panic!("type check failed: {errors:?}"),
    };
    Query::from(&program.node)
        .of::<CallExpression>()
        .filter(|call| {
            let Expression::Path(path_expr) = &call.callee.node else {
                return false;
            };
            let [segment] = path_expr.node.path.node.segments.as_slice() else {
                return false;
            };
            segment.node.name.node.name != "this"
                && matches!(
                    resolution.tables.resolved_value_at(path_expr.node.path.span, None),
                    Some(ResolvedValue::Local(local))
                        if resolution.tables.local_info(local).is_some_and(|info| info.name == "this")
                )
        })
        .count()
}

#[test]
fn v06_bare_sibling_method_beats_same_named_free_function() {
    // The free `Add` takes three arguments; binding the bare call to it would be an arity error.
    let source = r#"
pub type Counter {
    i64 value,

    pub i64 Add(i64 amount) { return value + amount; }

    pub i64 Twice(i64 amount) { return Add(Add(amount)); }
}

pub i64 Add(Counter counter, i64 amount, i64 extra) { return counter.value + amount + extra; }
"#;
    assert_eq!(method_dispatch_count(source), 2, "both bare `Add` calls must dispatch on the implicit receiver");
}

#[test]
fn v06_free_function_stays_callable_by_module_path_and_outside_methods() {
    let source = r#"
mod Tools {
    pub i64 Add(Counter counter, i64 amount, i64 extra) { return counter.value + amount + extra; }
}

pub type Counter {
    i64 value,

    pub i64 Add(i64 amount) { return value + amount; }

    pub i64 Both(i64 amount) { return Tools.Add(this, Add(amount), 1); }
}

pub i64 Add(Counter counter, i64 amount, i64 extra) { return counter.value + amount + extra; }

pub i64 UseFree() {
    Counter counter = Counter { value: 1 };
    return Add(counter, 2, 3);
}
"#;
    assert_eq!(method_dispatch_count(source), 1, "only the bare call inside the method is a sibling dispatch");
}

#[test]
fn v06_bare_sibling_method_in_generic_type_dispatches_on_receiver() {
    let source = r#"
pub type Holder<T> {
    T item,

    pub bool Has() { return true; }

    pub bool Check() { return Has(); }
}
"#;
    assert_eq!(method_dispatch_count(source), 1);
}

#[test]
fn v06_bare_receiver_field_reads_resolve_in_type_body_methods() {
    let source = r#"
pub type Port {
    u8 high,
    u8 low,

    pub i64 Value() { return i64(high) * 256_i64 + i64(low); }
}
"#;
    assert_eq!(method_dispatch_count(source), 0);
}

#[test]
fn v06_unknown_bare_call_inside_method_still_errors() {
    let source = r#"
pub type Counter {
    i64 value,

    pub i64 Get() { return Missing(); }
}
"#;
    let program = parse_program(source).expect("source parses");
    match resolve_and_type_program(&program) {
        Err(SemanticFactsError::Resolve(errors)) => assert!(
            errors.iter().any(|error| matches!(error, ResolveError::UnknownValue { name, .. } if name == "Missing")),
            "{errors:?}"
        ),
        Err(other) => panic!("expected an unknown-value resolve error, got {other:?}"),
        Ok(_) => panic!("an unknown bare call must not resolve"),
    }
}

#[test]
fn v06_fully_qualified_reference_marks_its_module_import_used() {
    use beskid_analysis::resolve::Resolver;
    use beskid_analysis::syntax::Node;
    use std::path::PathBuf;

    let output_path = PathBuf::from("/tmp/v06-import-usage/Core/Output.bd");
    let output = parse_program("pub unit WriteLine() { return; }").expect("module parses");
    let entry = parse_program("use Core.Output;\nunit Main() { Core.Output.WriteLine(); }").expect("entry parses");
    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(&output, &["Core".into(), "Output".into()], Some(&output_path));
    let resolution = resolver.resolve_program(&entry).expect("qualified reference resolves");
    let Node::UseDeclaration(use_declaration) = &entry.node.items[0].node else { panic!("use expected") };
    assert!(
        resolution.tables.used_import_spans.contains(&use_declaration.node.path.span),
        "`Core.Output.WriteLine` goes through the imported module, so `use Core.Output;` is used"
    );
}
