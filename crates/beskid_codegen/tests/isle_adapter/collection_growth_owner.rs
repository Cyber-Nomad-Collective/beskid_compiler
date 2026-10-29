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

/// Parameters pass by value: growth of a `mut T[]` parameter never writes the header the caller
/// shares. While the parameter holds the caller's handle, growth requests `capacity + 1`, so the
/// runtime copies; every later growth of the callee's copy and every local owner requests only
/// `length + 1`.
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
    assert!(!local.contains("select"), "a local owner never detaches: {local}");
}
