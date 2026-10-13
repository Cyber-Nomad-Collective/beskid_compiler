//! Specialized generic free-function bodies: methods on locals typed by the function's own type
//! parameters, and type-qualified calls to functions of the declaring unit.

use super::super::support::{
    SyntaxModuleItem, find_function_definitions, find_nodes_of_kind, item_fixture_with_root, item_name,
    lower_syntax_program,
};

const BAG: &str = "type Bag<T> { T[] values, i64 count, Bag<T> Put(T next) { return Bag<T> { values: values, count: count + 1 }; } i64 Size() { return count; } } T[] NoValues<T>() { return []; } Bag<T> Empty<T>() { return Bag<T> { values: NoValues<T>(), count: 0 }; }";

/// Lower every method and function of one fixture and return the emitted function names.
fn lower_all(source: &str) -> Vec<String> {
    let (input, isa, root) = item_fixture_with_root(source);
    let mut keys = find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition);
    keys.extend(find_function_definitions(input.database(), root));
    let items = keys
        .into_iter()
        .map(|key| SyntaxModuleItem {
            key,
            symbol: item_name(input.database(), key).expect("item name query").expect("item name").to_string(),
        })
        .collect::<Vec<_>>();
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .unwrap_or_else(|error| panic!("specialized generic bodies lower: {error:?}"));
    beskid_codegen::validate_artifact(&artifact).expect("specialized generic artifact resolves its imports");
    artifact.functions.iter().map(|function| function.name.to_string()).collect()
}

#[test]
fn generic_function_calls_a_method_on_a_local_typed_by_its_own_parameter() {
    let names = lower_all(&format!(
        "{BAG} Bag<T> Single<T>(T value) {{ Bag<T> bag = Empty<T>(); return bag.Put(value); }} i64 Main() {{ Bag<i64> bag = Single<i64>(7_i64); Bag<string> text = Single<string>(\"s\"); return bag.Size() + text.Size(); }}"
    ));
    assert_eq!(
        names.iter().filter(|name| name.starts_with("Single#generic_")).count(),
        2,
        "one Single specialization each for i64 and string: {names:?}"
    );
    assert_eq!(
        names.iter().filter(|name| name.starts_with("Put#generic_")).count(),
        2,
        "the receiver's T is the enclosing binding, so Put specializes for i64 and string: {names:?}"
    );
}

#[test]
fn generic_function_reassigns_a_mutable_local_through_its_own_method() {
    let names = lower_all(&format!(
        "{BAG} Bag<T> Twice<T>(T value) {{ mut Bag<T> result = Empty<T>(); result = result.Put(value); result = result.Put(value); return result; }} i64 Main() {{ Bag<i64> bag = Twice<i64>(3_i64); return bag.Size(); }}"
    ));
    assert!(names.iter().any(|name| name.starts_with("Twice#generic_")), "{names:?}");
    assert!(names.iter().any(|name| name.starts_with("Put#generic_")), "{names:?}");
}

#[test]
fn generic_function_passes_a_generic_call_result_to_a_method_on_its_local() {
    let names = lower_all(&format!(
        "{BAG} T First<T>(T[] values) {{ return values[0]; }} Bag<T> Head<T>(T[] values) {{ Bag<T> bag = Empty<T>(); return bag.Put(First<T>(values)); }} i64 Main() {{ i64[] values = [5_i64, 6_i64]; Bag<i64> bag = Head<i64>(values); return bag.Size(); }}"
    ));
    assert!(names.iter().any(|name| name.starts_with("Head#generic_")), "{names:?}");
    assert!(names.iter().any(|name| name.starts_with("First#generic_")), "{names:?}");
}

#[test]
fn type_qualified_call_to_a_function_of_the_declaring_unit_specializes() {
    let names = lower_all(&format!(
        "{BAG} Bag<T> One<T>(T value) {{ Bag<T> bag = Bag.Empty<T>(); return bag.Put(value); }} Bag<T> Wrap<T>(T value) {{ return Bag.One<T>(value); }} i64 Main() {{ Bag<i64> bag = Wrap<i64>(4_i64); Bag<string> text = Wrap<string>(\"s\"); return bag.Size() + text.Size(); }}"
    ));
    assert_eq!(
        names.iter().filter(|name| name.starts_with("One#generic_")).count(),
        2,
        "`Bag.One<T>` inside a specialized body names this unit's `One`: {names:?}"
    );
    assert_eq!(names.iter().filter(|name| name.starts_with("Empty#generic_")).count(), 2, "{names:?}");
}

/// `?` on a generic call inside a generic specialization: an operand whose payload names the
/// specialization's own parameters (`ReadOne<T, R>(reader, value)?` as a method argument) and
/// an operand whose call carries the parameter only as an explicit type argument
/// (`Admit<T>(limit)?`), and a `T` local initialized by a propagated generic call
/// (`T loaded = ReadOne<T, R>(reader, value)?`) for a scalar and a managed record `T`. All read
/// their operand and return type in the specialization. A propagated call whose argument is a
/// field of a generic record (`CountAll(value.history)?` in `Measure<T>`) infers from the field's
/// identity in the specialization.
pub(crate) const PROPAGATING_GENERICS: &str = r#"
enum Error { Failed() }
enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }
contract Source { Result<i64, Error> Begin(i64 limit); }
type Reader : Source {
    i64 n,
    pub Result<i64, Error> Begin(i64 limit) { return Result::Ok(limit + n); }
}
type Bag<T> { T[] values, i64 count, Bag<T> Put(T next) { return Bag<T> { values: values, count: count + 1 }; } i64 Size() { return count; } }
T[] NoValues<T>() { return []; }
Bag<T> Empty<T>() { return Bag<T> { values: NoValues<T>(), count: 0 }; }
Result<T, Error> ReadOne<T, R>(R reader, T value) where R: Source { i64 begun = reader.Begin(1_i64)?; return Result::Ok(value); }
Result<Bag<T>, Error> Collect<T, R>(R reader, T value) where R: Source {
    Bag<T> bag = Empty<T>();
    return Result::Ok(bag.Put(ReadOne<T, R>(reader, value)?));
}
Result<unit, Error> Admit<T>(i64 limit) { if (limit < 0_i64) { return Result::Error(Error::Failed()); } return Result::Ok(()); }
Result<T, Error> Load<T>(T value, i64 limit) { Admit<T>(limit)?; return Result::Ok(value); }
type Point { i64 x }
type Boxed<T> { T item, T[] history }
Result<i64, Error> CountAll<T>(T[] values) { return Result::Ok(1_i64); }
Result<i64, Error> Measure<T>(Boxed<T> value) { i64 counted = CountAll(value.history)?; return Result::Ok(counted); }
Result<T, Error> Complete<T, R>(R reader, T value) where R: Source {
    reader.Begin(1_i64)?;
    T loaded = ReadOne<T, R>(reader, value)?;
    reader.Begin(2_i64)?;
    return Result::Ok(loaded);
}
i64 Main() {
    Reader reader = Reader { n: 1_i64 };
    i64 a = match Collect<i64, Reader>(reader, 5_i64) { Result::Ok(_) => 1_i64, Result::Error(_) => -1_i64, };
    i64 b = match Load<i64>(40_i64, 1_i64) { Result::Ok(v) => v, Result::Error(_) => -100_i64, };
    i64 c = match Load<i64>(40_i64, -1_i64) { Result::Ok(_) => -1000_i64, Result::Error(_) => 1_i64, };
    i64 d = match Complete<i64, Reader>(reader, 2_i64) { Result::Ok(v) => v, Result::Error(_) => -1_i64, };
    i64 f = match Measure<i64>(Boxed<i64> { item: 1_i64, history: [1_i64] }) { Result::Ok(v) => v, Result::Error(_) => -1_i64, };
    i64 e = match Complete<Point, Reader>(reader, Point { x: -3_i64 }) { Result::Ok(p) => p.x, Result::Error(_) => -1_i64, };
    return a + b + c + d + e + f;
}
"#;

/// Every node of `kind` under `key`, `key` included.
fn descendants(
    db: &dyn beskid_queries::Db,
    key: beskid_queries::AstNodeKey,
    kind: beskid_queries::IndexedNodeKind,
) -> Vec<beskid_queries::AstNodeKey> {
    let mut found = Vec::new();
    let mut pending = vec![key];
    while let Some(next) = pending.pop() {
        if beskid_queries::node_kind(db, next).ok().flatten() == Some(kind) {
            found.push(next);
        }
        pending.extend(beskid_queries::child_nodes(db, next).ok().flatten().unwrap_or_default().iter().copied());
    }
    found
}

#[test]
fn generic_calls_propagated_with_try_inside_a_specialization_have_specialized_facts_and_lower() {
    let (input, _, root) = item_fixture_with_root(PROPAGATING_GENERICS);
    let db = input.database();
    let function = |name: &str| {
        find_function_definitions(db, root)
            .into_iter()
            .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some(name))
            .unwrap_or_else(|| panic!("function {name}"))
    };
    // The instance of `name` that Main's call specializes.
    let instance_of = |name: &str| {
        let declaration = function(name);
        descendants(db, function("Main"), beskid_queries::IndexedNodeKind::CallExpression)
            .into_iter()
            .filter_map(|call| beskid_queries::generic_call_specialization(db, call).ok().flatten())
            .filter_map(|specialization| {
                beskid_queries::generic_call_specialization_instance(db, specialization).ok().flatten()
            })
            .find(|instance| instance.declaration == declaration)
            .unwrap_or_else(|| panic!("Main specializes {name}"))
    };
    let tries = |name: &str| descendants(db, function(name), beskid_queries::IndexedNodeKind::TryExpression);

    // `Admit<T>(limit)?` in Load<i64>: the unit payload and the Result<i64, Error> return.
    let load = instance_of("Load");
    let [admit] = tries("Load")[..] else { panic!("one try in Load") };
    let fact = beskid_queries::specialized_try_expression_fact(db, admit, &load)
        .expect("specialized try fact of Admit<T>(limit)?")
        .expect("the try fact is proven in Load<i64>");
    assert_eq!(fact.payload_type, beskid_queries::SemanticTypeId::UNIT);

    // `ReadOne<T, R>(reader, value)?` as the argument of `bag.Put` in Collect<i64, Reader>.
    let collect = instance_of("Collect");
    let [read] = tries("Collect")[..] else { panic!("one try in Collect") };
    let fact = beskid_queries::specialized_try_expression_fact(db, read, &collect)
        .expect("specialized try fact of ReadOne<T, R>(reader, value)?")
        .expect("the try fact is proven in Collect<i64, Reader>");
    assert_eq!(fact.payload_type, beskid_queries::SemanticTypeId::I64);
    let put = descendants(db, function("Collect"), beskid_queries::IndexedNodeKind::CallExpression)
        .into_iter()
        .filter_map(|call| beskid_queries::generic_call_specialization_in_environment(db, call, &collect).ok().flatten())
        .find(|instance| item_name(db, instance.declaration).ok().flatten().as_deref() == Some("Put"))
        .expect("bag.Put specializes in Collect<i64, Reader> with the try payload as its argument");
    assert_eq!(put.signature.parameters.last(), Some(&beskid_queries::SemanticTypeId::I64));

    let names = lower_all(PROPAGATING_GENERICS);
    for name in ["Collect#generic_", "ReadOne#generic_", "Load#generic_", "Admit#generic_", "Put#generic_", "Complete#generic_", "Measure#generic_", "CountAll#generic_"] {
        assert!(names.iter().any(|emitted| emitted.starts_with(name)), "{name}: {names:?}");
    }
}
