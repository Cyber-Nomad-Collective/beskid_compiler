//! Growth of an array without a proven owner (`move` only at isolate crossings).
//!
//! `Array.Append` lowers to the rooted in-place growth operation only when its first argument
//! owns the array: a `let` local (mutable or not), a `mut` parameter, or an aggregate field. Any
//! other array (a parameter that is not `mut`, a loop or pattern variable, a call result) is an
//! ordinary call of the Corelib `Append`, which copies into a fresh array. Neither shape is an
//! internal error (E2103).

use super::support::{
    SyntaxGenerationId, SyntaxModuleItem, canonical_corelib_project_fixture, find_function_definitions, item_name,
    lower_syntax_program,
};

const GROW: &str = "beskid_rt_v5_array_grow_rooted";

/// Lower every function of `source` against the canonical Corelib and return each emitted
/// function's name with its CLIF text.
fn lower(source: &str, generation: u64) -> Vec<(String, String)> {
    let (input, isa, root) =
        canonical_corelib_project_fixture(&[("App/Growth.bd", source)], SyntaxGenerationId(generation));
    let functions = find_function_definitions(input.database(), root);
    if let Err(findings) = beskid_queries::check_items(input.database(), &functions) {
        panic!("the repro checks clean: {findings:?}");
    }
    let items = functions
        .iter()
        .map(|key| SyntaxModuleItem {
            key: *key,
            symbol: item_name(input.database(), *key).expect("item name query").expect("item name").to_string(),
        })
        .collect::<Vec<_>>();
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .unwrap_or_else(|error| panic!("growth lowers without an internal error: {error}"));
    artifact.functions.iter().map(|function| (function.name.clone(), function.function.display().to_string())).collect()
}

fn clif<'a>(functions: &'a [(String, String)], name: &str) -> &'a str {
    &functions.iter().find(|(function, _)| function == name).unwrap_or_else(|| panic!("{name}")).1
}

#[test]
fn append_to_a_parameter_that_is_not_mut_copies_into_a_fresh_array() {
    let functions = lower(
        "use Core.Collections.Array; string[] Add(string[] xs, string n) { return Array.Append<string>(xs, n); }",
        6810,
    );
    let names = functions.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>();
    let add = clif(&functions, "Add");
    assert!(!add.contains(GROW), "a parameter that is not mut is not grown in place: {add}");
    assert!(add.contains(" call "), "Add calls the copying Corelib Append: {add}");
    assert!(
        names.iter().any(|name| name.starts_with("Append#") && name.contains("#generic_")),
        "the ordinary Corelib Append copies into a fresh array: {names:?}"
    );
}

#[test]
fn append_to_a_fresh_local_in_a_loop_grows_the_local_in_place() {
    for (offset, binding) in ["string[] out", "mut string[] out"].iter().enumerate() {
        let source = format!(
            "use Core.Collections.Array; string[] Gather(string[] xs) {{ {binding} = Array.Empty<string>(); \
             for x in xs {{ Array.Append<string>(out, x); }} return out; }}"
        );
        let functions = lower(&source, 6811 + offset as u64);
        let gather = clif(&functions, "Gather");
        assert!(gather.contains(GROW), "a let local owns the array it grows ({binding}): {gather}");
    }
}

const FINISH: &str = "beskid_rt_v5_array_construction_finish";

/// Line index of every `call` in `clif` whose callee is declared with `symbol`.
fn call_lines(clif: &str, symbol: &str) -> Vec<usize> {
    let references = clif
        .lines()
        .filter(|line| line.contains(symbol) && line.contains(" = "))
        .filter_map(|line| line.trim().split(" = ").next().map(str::to_owned))
        .filter(|reference| reference.starts_with("fn"))
        .collect::<Vec<_>>();
    assert!(!references.is_empty(), "{symbol} is declared: {clif}");
    clif.lines()
        .enumerate()
        .filter(|(_, line)| references.iter().any(|reference| line.contains(&format!("call {reference}("))))
        .map(|(index, _)| index)
        .collect()
}

/// An in-place append grows the rooted array, stores the element into the grown storage, and only
/// then finishes construction, exactly once, with no array write barrier in the emitted code.
#[test]
fn append_to_an_owned_local_grows_then_stores_then_finishes_once() {
    let functions = lower(
        "use Core.Collections.Array; i64[] Push(i64 value) { mut i64[] out = Array.Empty<i64>(); \
         Array.Append<i64>(out, value); return out; }",
        6820,
    );
    let push = clif(&functions, "Push");
    let grow = call_lines(push, GROW);
    let finish = call_lines(push, FINISH);
    assert_eq!(grow.len(), 1, "one rooted growth: {push}");
    assert_eq!(finish.len(), 1, "exactly one construction finish: {push}");
    let element_store = push
        .lines()
        .enumerate()
        .skip(grow[0] + 1)
        .take_while(|(index, _)| *index < finish[0])
        .any(|(_, line)| line.trim_start().split(' ').any(|token| token.starts_with("store")));
    assert!(element_store, "the element is stored between the growth and the finish: {push}");
    assert!(!push.contains("beskid_rt_v5_array_write_barrier"), "no array write barrier in an append: {push}");
}

/// Parameters pass by value: growth of a `mut T[]` parameter never writes the header the caller
/// shares. While the parameter holds the caller's handle, growth requests `capacity + 1`, so the
/// runtime copies; every later growth of the callee's copy requests only `length + 1`. Every
/// growth also reads the array's sharing mark (a marked array copies too); a local owner has no
/// incoming handle to compare.
#[test]
fn growth_of_a_mut_parameter_detaches_from_the_callers_handle() {
    let functions = lower(
        "use Core.Collections.Array; \
         i64[] Param(mut i64[] xs) { Array.Append<i64>(xs, 9); return xs; } \
         bool Checked(mut i64[] xs) { bool ok = Array.TryAppend<i64>(xs, 9); return ok && Array.Len<i64>(xs) > 0; } \
         i64[] Local() { mut i64[] xs = Array.Empty<i64>(); Array.Append<i64>(xs, 9); return xs; }",
        6820,
    );
    for name in ["Param", "Checked"] {
        let body = clif(&functions, name);
        assert!(body.contains("grow_rooted"), "{name} grows the parameter through the runtime: {body}");
        assert!(
            body.lines().any(|line| line.contains("icmp") && line.contains(" eq ")),
            "{name} compares the slot with the incoming handle: {body}"
        );
        assert!(body.contains("select"), "{name} selects the detaching minimum capacity: {body}");
    }
    let local = clif(&functions, "Local");
    assert!(local.contains(GROW), "a local grows in place: {local}");
    assert!(
        !local.lines().any(|line| line.contains("icmp") && line.contains(" eq ")),
        "a local owner has no incoming handle to compare: {local}"
    );
    assert!(local.contains(" band "), "every growth reads the sharing mark: {local}");
    assert!(!local.contains(" bor "), "returning an owned local makes no second handle: {local}");
}

/// Growing one handle never changes what another handle sees. A read of an array place that makes
/// a second handle (a binding, a field store, a construction, a returned field) sets the array's
/// sharing mark (`bor` of bit 0 into the hidden element-descriptor word); the owner of a growth,
/// `Array.Len`, `Array.Capacity`, an index read and `xs = Array.Append(xs, v)` back into the same local do not, so a
/// loop of appends to an unaliased local keeps growing in place.
#[test]
fn a_second_handle_marks_the_array_shared_and_an_unaliased_append_loop_does_not() {
    let functions = lower(
        "use Core.Collections.Array; \
         type Holder { i64[] items; } \
         i64 Alias() { mut i64[] xs = Array.Empty<i64>(); Array.Append<i64>(xs, 1); i64[] ys = xs; \
           Array.Append<i64>(xs, 2); return Array.Len<i64>(ys); } \
         Holder Store() { mut i64[] xs = Array.Empty<i64>(); Array.Append<i64>(xs, 1); \
           Holder h = Holder { items: xs }; Array.Append<i64>(xs, 2); return h; } \
         i64[] Field(Holder h) { return h.items; } \
         i64[] Loop(i64 n) { mut i64[] xs = Array.Empty<i64>(); mut i64 i = 0; \
           while (i < n) { xs = Array.Append<i64>(xs, i); Array.Append<i64>(xs, xs[0]); \
           i = i + Array.Len<i64>(xs) + Array.Capacity<i64>(xs); } return xs; }",
        6830,
    );
    for name in ["Alias", "Store", "Field"] {
        let body = clif(&functions, name);
        assert!(body.contains(" bor "), "{name} marks the array it hands to a second name: {body}");
    }
    let looped = clif(&functions, "Loop");
    assert!(looped.contains(GROW), "the loop grows the local through the runtime: {looped}");
    assert!(!looped.contains(" bor "), "an unaliased append loop never marks its array: {looped}");
    // `Array.Len` runs its body (`__array_len(values)`): an intrinsic argument keeps no handle, so
    // reading the length between appends never forces a copy.
    let len = functions.iter().filter(|(name, _)| name.starts_with("Len#")).collect::<Vec<_>>();
    assert!(
        !len.is_empty(),
        "the loop calls the Corelib Len: {:?}",
        functions.iter().map(|f| &f.0).collect::<Vec<_>>()
    );
    for (name, body) in len {
        assert!(!body.contains(" bor "), "{name} never marks the array it measures: {body}");
    }
}

/// Shrinking one handle never changes what another handle sees. `Array.RemoveLast` reads the
/// sharing mark of an owned array and branches: a shared array is copied through the runtime
/// growth (`capacity + 1`) before the copy is shortened and written back; an unshared array is
/// shortened in place with no runtime call on that path. An array with no owner (a plain
/// parameter) always copies. A loop of shrinks of an unaliased local never marks its array.
#[test]
fn remove_last_copies_a_shared_array_and_shrinks_an_unaliased_one_in_place() {
    let functions = lower(
        "use Core.Collections.Array; \
         i64[] Loop(i64 n) { mut i64[] xs = Array.Zeroed<i64>(n); \
           while (Array.Len<i64>(xs) > 1) { Array.RemoveLast<i64>(xs); xs = Array.RemoveLast<i64>(xs); } \
           return xs; } \
         i64[] Plain(i64[] xs) { return Array.RemoveLast<i64>(xs); } \
         i64[] Param(mut i64[] xs) { Array.RemoveLast<i64>(xs); return xs; }",
        6840,
    );
    let looped = clif(&functions, "Loop");
    assert!(!looped.contains(" bor "), "an unaliased shrink loop never marks its array: {looped}");
    assert!(looped.contains(" band "), "every shrink of an owned array reads the sharing mark: {looped}");
    assert!(looped.contains("brif"), "the shrink branches on the mark: {looped}");
    assert_eq!(call_lines(looped, GROW).len(), 2, "each shrink copies only on its shared branch: {looped}");
    let plain = clif(&functions, "Plain");
    assert!(plain.contains(GROW), "a parameter that is not mut is copied before it is shortened: {plain}");
    assert!(!plain.contains(" band "), "an array without an owner always copies: {plain}");
    let param = clif(&functions, "Param");
    assert!(
        param.lines().any(|line| line.contains("icmp") && line.contains(" eq ")),
        "a mut parameter compares the slot with the incoming handle: {param}"
    );
}

/// A field lent to a plain parameter of a free function is marked when another argument can lead
/// the callee to its holder, including an array of objects (`Holder[]`), but not when the other
/// arguments are primitives, strings or arrays of them.
#[test]
fn a_field_lent_beside_an_array_of_holders_is_marked() {
    let functions = lower(
        "use Core.Collections.Array; \
         type Holder { i64[] items; } \
         i64 Count(i64[] lent, Holder[] holders) { return Array.Len<i64>(lent); } \
         i64 Sum(i64[] lent, i64[] values, i64[][] nested, string name) { return Array.Len<i64>(lent); } \
         i64 Reach(Holder h, Holder[] hs) { return Count(h.items, hs); } \
         i64 NoReach(Holder h, i64[] vs, i64[][] ns) { return Sum(h.items, vs, ns, \"n\"); }",
        6841,
    );
    let reach = clif(&functions, "Reach");
    assert!(reach.contains(" bor "), "a holder reachable through an array element marks the lent field: {reach}");
    let no_reach = clif(&functions, "NoReach");
    assert!(!no_reach.contains(" bor "), "arrays of values cannot reach the holder: {no_reach}");
}
