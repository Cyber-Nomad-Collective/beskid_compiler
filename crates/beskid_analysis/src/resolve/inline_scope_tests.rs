//! Inline modules are lexical scopes: constants, imports and duplicate names follow the scope
//! chain (file scope, then each enclosing inline module), and a module never hides a type.

use crate::services::parse_program;

use super::{Resolution, ResolveError, ResolveResult, Resolver};

/// Resolve `entry` with each `(module path, source)` dependency collected first.
fn resolve(dependencies: &[(&[&str], &str)], entry: &str) -> ResolveResult<Resolution> {
    let mut resolver = Resolver::new();
    for (module, source) in dependencies {
        let program = parse_program(source).expect("parse dependency");
        let module = module.iter().map(|segment| segment.to_string()).collect::<Vec<_>>();
        resolver.collect_program_in_module(&program, &module, None);
    }
    let entry = parse_program(entry).expect("parse entry");
    resolver.resolve_program(&entry)
}

const RANDOM: &[&str] = &["Core", "Random"];
const RANDOM_SOURCE: &str = "pub type Random { i64 state; } pub i64 New() { return 1; }";

#[test]
fn a_file_constant_is_visible_in_nested_inline_modules() {
    let entry = "const Limit = 7; mod T { i64 F() { return Limit; } mod Deeper { i64 G() { return Limit; } } }";
    let resolution = resolve(&[], entry).expect("file constant visible in inline modules");
    assert_eq!(resolution.tables.integer_constants.len(), 2);
}

#[test]
fn an_inline_module_constant_shadows_the_file_constant_inside_that_module_only() {
    let entry = "const Limit = 7; mod T { const Limit = 9; i64 F() { return Limit; } } i64 G() { return Limit; }";
    let resolution = resolve(&[], entry).expect("both constants resolve");
    let mut values = resolution
        .tables
        .integer_constants
        .iter()
        .map(|((_, span), literal)| (span.start, format!("{:?}", literal.node)))
        .collect::<Vec<_>>();
    values.sort();
    assert_eq!(values.len(), 2, "{values:?}");
    assert!(values[0].1.contains('9'), "the use inside `T` reads `T.Limit`: {values:?}");
    assert!(values[1].1.contains('7'), "the use outside `T` reads the file constant: {values:?}");
}

#[test]
fn an_inline_module_constant_is_not_visible_outside_the_module() {
    let errors = resolve(&[], "mod T { const Limit = 9; } i64 G() { return Limit; }").expect_err("not visible");
    assert!(
        errors.iter().any(|error| matches!(error, ResolveError::UnknownValue { name, .. } if name == "Limit")),
        "{errors:?}"
    );
}

/// `use Core.Random;` imports the type `Random`; `mod Random` binds the same name for values
/// and qualifiers, but a type position sees only types.
#[test]
fn a_module_named_like_an_imported_type_does_not_hide_the_type() {
    let entry = "use Core.Random;\n\
        i64 Size(Random value) { return 1; }\n\
        mod Random { i64 Probe(Random value) { return 2; } }";
    let resolution = resolve(&[(RANDOM, RANDOM_SOURCE)], entry).expect("the type resolves in every type position");
    assert_eq!(resolution.tables.used_import_spans.len(), 1, "the `use` that supplied the type is used");
}

#[test]
fn a_function_named_like_an_imported_type_does_not_hide_the_type() {
    let entry = "use Core.Random; i64 Random() { return 1; } i64 Size(Random value) { return Random(); }";
    resolve(&[(RANDOM, RANDOM_SOURCE)], entry).expect("type position sees the imported type");
}

/// `Random.New()` with `mod Random { pub i64 New() }` and `use Core.Random;` (which has `New`)
/// names two functions: E1104, and neither is chosen.
#[test]
fn a_qualifier_naming_an_inline_module_and_an_import_is_ambiguous_when_both_reach_the_member() {
    let entry = "use Core.Random;\n\
        mod Random { pub i64 New() { return 2; } }\n\
        i64 Main() { return Random.New(); }";
    let errors = resolve(&[(RANDOM, RANDOM_SOURCE)], entry).expect_err("ambiguous qualifier");
    assert!(
        errors.iter().any(|error| matches!(error,
            ResolveError::AmbiguousModuleQualifier { name, member, import_path, .. }
                if name == "Random" && member == "New" && import_path == "Core.Random")),
        "{errors:?}"
    );
}

#[test]
fn a_qualifier_naming_an_inline_module_and_an_import_binds_the_one_that_reaches_the_member() {
    let entry = "use Core.Random;\n\
        mod Random { pub i64 Other() { return 2; } }\n\
        i64 FromImport() { return Random.New(); }\n\
        i64 FromModule() { return Random.Other(); }";
    resolve(&[(RANDOM, RANDOM_SOURCE)], entry).expect("each path has one reading");
}

#[test]
fn a_nested_inline_module_qualifies_paths_from_its_enclosing_module() {
    let entry = "mod A { mod B { pub i64 F() { return 1; } } i64 G() { return B.F(); } }";
    resolve(&[], entry).expect("`B.F` names `A.B.F` inside `A`");
}

/// A `use` inside an inline module binds in that module (and the modules nested in it) only.
#[test]
fn a_use_inside_an_inline_module_is_visible_in_that_module_only() {
    let inside = "mod A { use Core.Random; i64 F(Random value) { return Random.New(); } mod B { i64 G() { return Random.New(); } } }";
    resolve(&[(RANDOM, RANDOM_SOURCE)], inside).expect("visible in `A` and `A.B`");

    let outside = "mod A { use Core.Random; } i64 G() { return Random.New(); }";
    let errors = resolve(&[(RANDOM, RANDOM_SOURCE)], outside).expect_err("not visible at the file scope");
    assert!(
        errors.iter().any(|error| matches!(error, ResolveError::UnknownValue { name, .. } if name == "Random")),
        "{errors:?}"
    );
}

#[test]
fn the_same_use_in_two_scopes_is_no_duplicate() {
    let entry =
        "use Core.Random; mod A { use Core.Random; i64 F() { return Random.New(); } } i64 G() { return Random.New(); }";
    let resolution = resolve(&[(RANDOM, RANDOM_SOURCE)], entry).expect("one `use` per scope");
    assert_eq!(resolution.tables.used_import_spans.len(), 2, "each scope's `use` is used by its own scope");
}

#[test]
fn the_same_use_twice_in_one_inline_module_is_a_duplicate() {
    let errors =
        resolve(&[(RANDOM, RANDOM_SOURCE)], "mod A { use Core.Random; use Core.Random; }").expect_err("duplicate");
    assert!(
        errors.iter().any(|error| matches!(error, ResolveError::DuplicateItem { name, .. } if name == "Random")),
        "{errors:?}"
    );
}

#[test]
fn functions_of_one_name_in_different_scopes_are_no_duplicate() {
    let entry = "i64 Helper() { return 1; } mod A { i64 Helper() { return 2; } i64 F() { return Helper(); } mod B { i64 G() { return Helper(); } } }";
    resolve(&[], entry).expect("one `Helper` per scope");
}
