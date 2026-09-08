use std::path::PathBuf;
use std::sync::Arc;

use crate::resolve::{ItemId, ItemKind, Resolver};
use crate::services::parse_program;
use crate::syntax::PrimitiveType;
use crate::types::checker::TypeChecker;
use crate::types::result::FunctionSignature;
use crate::types::{TypeId, TypeInfo, UnitTypeSurface, build_unit_type_surface, merge_unit_surfaces};

#[test]
fn merge_prefers_entry_surface_on_conflict() {
    let item = ItemId(1);
    let i32 = TypeId(0);
    let i64 = TypeId(1);

    let mut dep = UnitTypeSurface::default();
    dep.function_signatures.insert(item, FunctionSignature { params: vec![i32], return_type: i32 });

    let mut entry = UnitTypeSurface::default();
    entry.function_signatures.insert(item, FunctionSignature { params: vec![i64], return_type: i64 });

    let merged = merge_unit_surfaces(std::iter::once((PathBuf::from("dep.bd"), Arc::new(dep))), Arc::new(entry));
    assert_eq!(merged.function_signatures.get(&item), Some(&FunctionSignature { params: vec![i64], return_type: i64 }));
}

#[test]
fn imported_generic_type_surface_exports_owned_receiver_methods_with_generic_signatures() {
    let dependency_path = PathBuf::from("/tmp/surface-receiver/Collections/Bucket.bd");
    let entry_path = PathBuf::from("/tmp/surface-receiver/Main.bd");
    let dependency = parse_program(
        r#"
pub type Bucket<T> {
    T value,

    pub Bucket<T> Push(T value) {
        return Bucket<T> { value: value };
    }

    pub i64 Count() {
        return 1;
    }
}

pub Bucket<T> New<T>() {
    return Bucket<T> { value: 0 };
}
"#,
    )
    .expect("parse generic dependency");
    let entry = parse_program(
        r#"
use Collections.Bucket;

unit Main() {
    Bucket<i64> bucket = Bucket.New<i64>();
    bucket.Push(1);
}
"#,
    )
    .expect("parse importing entry");

    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(
        &dependency,
        &["Collections".to_owned(), "Bucket".to_owned()],
        Some(&dependency_path),
    );
    resolver.set_current_source_path(Some(entry_path));
    let resolution = resolver.resolve_program(&entry).expect("resolve importing entry");
    let surface = build_unit_type_surface(&dependency, &resolution, &dependency_path);

    let bucket = resolution
        .items
        .iter()
        .find(|item| item.kind == ItemKind::Type && item.name == "Bucket")
        .map(|item| item.id)
        .expect("Bucket item");
    let push =
        surface.methods_by_receiver.get(&(bucket, "Push".to_owned())).copied().expect("owned Push receiver method");
    assert!(surface.methods_by_receiver.contains_key(&(bucket, "Count".to_owned())));

    let signature = surface.method_function_signatures.get(&push).expect("Push signature");
    let [parameter] = signature.params.as_slice() else { panic!("Push must retain its one generic parameter") };
    assert!(matches!(surface.types.get(*parameter), Some(TypeInfo::GenericParam(name)) if name == "T"));
    let Some(TypeInfo::Applied { base, args }) = surface.types.get(signature.return_type) else {
        panic!("Push must return Bucket<T>")
    };
    assert_eq!(*base, bucket);
    let [argument] = args.as_slice() else { panic!("Bucket<T> must retain its one generic argument") };
    assert!(matches!(surface.types.get(*argument), Some(TypeInfo::GenericParam(name)) if name == "T"));
}

#[test]
fn entry_only_cross_unit_typecheck_resolves_imported_generic_owned_receiver_methods() {
    let dependency_path = PathBuf::from("/tmp/entry-only-receiver/Collections/Bucket.bd");
    let entry_path = PathBuf::from("/tmp/entry-only-receiver/Main.bd");
    let dependency = parse_program(
        r#"
pub type Bucket<T> {
    T value,

    pub Bucket<T> Push(T value) {
        return Bucket<T> { value: value };
    }

    pub i64 Count() {
        return 1;
    }
}

pub Bucket<T> New<T>() {
    return Bucket<T> { value: 0 };
}
"#,
    )
    .expect("parse generic dependency");
    let mut entry = parse_program(
        r#"
use Collections.Bucket;

unit Main() {
    Bucket<i64> bucket = Bucket.New<i64>();
    bucket.Push(1);
    bucket.Count();
}
"#,
    )
    .expect("parse importing entry");

    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(
        &dependency,
        &["Collections".to_owned(), "Bucket".to_owned()],
        Some(&dependency_path),
    );
    resolver.set_current_source_path(Some(entry_path.clone()));
    let resolution = resolver.resolve_program(&entry).expect("resolve importing entry");
    let dependency_paths = [dependency_path];
    let (_, errors) = TypeChecker::check_entry(
        &mut entry,
        &resolution,
        &[&dependency],
        Some(&dependency_paths),
        Some(entry_path),
        false,
        None,
        None,
        None,
        None,
    );

    assert!(errors.is_empty(), "entry-only imported receiver calls must typecheck: {errors:#?}");
}

#[test]
fn entry_only_cross_unit_typecheck_keeps_source_scoped_generic_static_return_identity() {
    let omega_path = PathBuf::from("/tmp/entry-only-static/Collections/Omega.bd");
    let alpha_path = PathBuf::from("/tmp/entry-only-static/Collections/Alpha.bd");
    let entry_path = PathBuf::from("/tmp/entry-only-static/Main.bd");
    let omega = parse_program(
        r#"
pub type Omega<T> {
    T value,
}
"#,
    )
    .expect("parse Omega dependency");
    let alpha = parse_program(
        r#"
use Collections.Alpha;
use Collections.Omega;

pub Alpha<T> New<T>() {
    return Alpha<T> { value: 0 };
}

pub type Alpha<T> {
    T value,
}
"#,
    )
    .expect("parse Alpha dependency");
    let entry = parse_program(
        r#"
use Collections.Alpha;
use Collections.Omega;

pub Omega<T> Wrong<T>(Omega<T> value) {
    return value;
}

unit Main() {
    Alpha<i64> value = Alpha.New<i64>();
}
"#,
    )
    .expect("parse importing entry");

    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(&omega, &["Collections".to_owned(), "Omega".to_owned()], Some(&omega_path));
    resolver.collect_program_in_module(&alpha, &["Collections".to_owned(), "Alpha".to_owned()], Some(&alpha_path));
    resolver.set_current_source_path(Some(entry_path.clone()));
    let resolution = resolver.resolve_program(&entry).expect("resolve importing entry");
    let dependency_paths = [omega_path, alpha_path];
    for type_dependency_bodies in [false, true] {
        let mut checked_entry = entry.clone();
        let (_, errors) = TypeChecker::check_entry(
            &mut checked_entry,
            &resolution,
            &[&omega, &alpha],
            Some(&dependency_paths),
            Some(entry_path.clone()),
            type_dependency_bodies,
            None,
            None,
            None,
            None,
        );

        assert!(
            errors.is_empty(),
            "imported Alpha.New<T> must retain Alpha<T> with dependency bodies={type_dependency_bodies}: {errors:#?}"
        );
    }
}

#[test]
fn explicit_generic_call_resolves_the_imported_type_argument_by_module() {
    let selected_path = PathBuf::from("/tmp/explicit-generic-import/Beskid/Compiler/ModPackage.bd");
    let homonym_path = PathBuf::from("/tmp/explicit-generic-import/Other/ModPackage.bd");
    let array_path = PathBuf::from("/tmp/explicit-generic-import/Core/Collections/Array.bd");
    let entry_path = PathBuf::from("/tmp/explicit-generic-import/Main.bd");
    let selected = parse_program(
        r#"
pub type ModPackage {
    string name,
}
"#,
    )
    .expect("parse selected ModPackage dependency");
    let homonym = parse_program(
        r#"
pub type ModPackage {
    i64 id,
}
"#,
    )
    .expect("parse homonymous ModPackage dependency");
    let array = parse_program(
        r#"
pub T[] Empty<T>() {
    return [];
}

pub i64 Len<T>(T[] values) {
    return 0;
}
"#,
    )
    .expect("parse generic Array dependency");
    let mut entry = parse_program(
        r#"
use Beskid.Compiler.ModPackage;
use Core.Collections.Array;

unit Main() {
    ModPackage[] packages = Array.Empty<ModPackage>();
    i64 count = Array.Len<ModPackage>(packages);
}
"#,
    )
    .expect("parse importing entry");

    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(
        &selected,
        &["Beskid".to_owned(), "Compiler".to_owned(), "ModPackage".to_owned()],
        Some(&selected_path),
    );
    resolver.collect_program_in_module(&homonym, &["Other".to_owned(), "ModPackage".to_owned()], Some(&homonym_path));
    resolver.collect_program_in_module(
        &array,
        &["Core".to_owned(), "Collections".to_owned(), "Array".to_owned()],
        Some(&array_path),
    );
    resolver.set_current_source_path(Some(entry_path.clone()));
    let resolution = resolver.resolve_program(&entry).expect("resolve importing entry");
    let dependency_paths = [selected_path, homonym_path, array_path];
    let (_, errors) = TypeChecker::check_entry(
        &mut entry,
        &resolution,
        &[&selected, &homonym, &array],
        Some(&dependency_paths),
        Some(entry_path),
        false,
        None,
        None,
        None,
        None,
    );

    assert!(errors.is_empty(), "explicit generic arguments must preserve imported type identity: {errors:#?}");
}

#[test]
fn qualified_generic_call_keeps_the_imported_module_function_identity() {
    let array_path = PathBuf::from("/tmp/qualified-generic-call/Core/Collections/Array.bd");
    let iterator_path = PathBuf::from("/tmp/qualified-generic-call/Query/ArrayIterator.bd");
    let entry_path = PathBuf::from("/tmp/qualified-generic-call/Main.bd");
    let array = parse_program(
        r#"
pub i64 Current<T>(T[] values, i64 index) {
    return index;
}
"#,
    )
    .expect("parse Array dependency");
    let iterator = parse_program(
        r#"
use Core.Collections.Array;

pub type ArrayIterator<T> {
    T[] source,
}

pub ArrayIterator<T> Over<T>(T[] source) {
    return ArrayIterator<T> { source: source };
}

pub ArrayIterator<T> Current<T>(ArrayIterator<T> iterator) {
    return iterator;
}
"#,
    )
    .expect("parse ArrayIterator dependency");
    let mut entry = parse_program(
        r#"
use Core.Collections.Array;
use Query.ArrayIterator;

unit Main() {
    i64[] values = [];
    ArrayIterator<i64> iterator = ArrayIterator.Over<i64>(values);
    ArrayIterator<i64> current = ArrayIterator.Current<i64>(iterator);
}
"#,
    )
    .expect("parse importing entry");

    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(
        &array,
        &["Core".to_owned(), "Collections".to_owned(), "Array".to_owned()],
        Some(&array_path),
    );
    resolver.collect_program_in_module(
        &iterator,
        &["Query".to_owned(), "ArrayIterator".to_owned()],
        Some(&iterator_path),
    );
    resolver.set_current_source_path(Some(entry_path.clone()));
    let resolution = resolver.resolve_program(&entry).expect("resolve importing entry");
    let dependency_paths = [array_path, iterator_path];
    let (_, errors) = TypeChecker::check_entry(
        &mut entry,
        &resolution,
        &[&array, &iterator],
        Some(&dependency_paths),
        Some(entry_path),
        false,
        None,
        None,
        None,
        None,
    );

    assert!(errors.is_empty(), "qualified call must retain its imported module function identity: {errors:#?}");
}

#[test]
fn dependency_surface_expands_imported_module_aliases_in_public_signatures() {
    let root = PathBuf::from("/tmp/dependency-surface-import-alias");
    let cursor_path = root.join("Core/Text/Cursor.bd");
    let result_path = root.join("Core/Text/Parser/Result.bd");
    let parser_path = root.join("Core/Text/Parser.bd");
    let entry_path = root.join("Main.bd");
    let cursor = parse_program(
        r#"
pub type TextCursor { string source, i64 pos }

pub TextCursor From(string source) {
    return TextCursor { source: source, pos: 0 };
}
"#,
    )
    .expect("cursor");
    let result = parse_program("pub enum TextParseResult<T> { Ok(T value), Err(string message) }").expect("result");
    let parser = parse_program(
        r#"
use Core.Text.Cursor;
use Core.Text.Parser.Result;

pub Result.TextParseResult<string> Literal(Cursor.TextCursor cursor, string text) {
    return Result.TextParseResult<string>::Ok(text);
}
"#,
    )
    .expect("parser facade");
    let mut entry = parse_program(
        r#"
use Core.Text.Cursor;
use Core.Text.Parser;
use Core.Text.Parser.Result;

unit Main() {
    Cursor.TextCursor cursor = Cursor.From("abc");
    Result.TextParseResult<string> parsed = Parser.Literal(cursor, "ab");
}
"#,
    )
    .expect("entry");

    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(
        &cursor,
        &["Core".to_owned(), "Text".to_owned(), "Cursor".to_owned()],
        Some(&cursor_path),
    );
    resolver.collect_program_in_module(
        &result,
        &["Core".to_owned(), "Text".to_owned(), "Parser".to_owned(), "Result".to_owned()],
        Some(&result_path),
    );
    resolver.collect_program_in_module(
        &parser,
        &["Core".to_owned(), "Text".to_owned(), "Parser".to_owned()],
        Some(&parser_path),
    );
    resolver.set_current_source_path(Some(entry_path.clone()));
    let resolution = resolver.resolve_program(&entry).expect("resolve entry");
    let surface = build_unit_type_surface(&parser, &resolution, &parser_path);
    let text_cursor = resolution
        .items
        .iter()
        .find(|item| {
            item.name == "TextCursor"
                && item.kind == ItemKind::Type
                && item.source_path.as_ref().is_some_and(|source| crate::paths::same_file(source, &cursor_path))
        })
        .map(|item| item.id)
        .expect("Cursor.TextCursor");
    let text_parse_result = resolution
        .items
        .iter()
        .find(|item| {
            item.name == "TextParseResult"
                && item.kind == ItemKind::Enum
                && item.source_path.as_ref().is_some_and(|source| crate::paths::same_file(source, &result_path))
        })
        .map(|item| item.id)
        .expect("Result.TextParseResult");
    let literal = resolution
        .items
        .iter()
        .find(|item| {
            item.name == "Literal"
                && item.source_path.as_ref().is_some_and(|source| crate::paths::same_file(source, &parser_path))
        })
        .map(|item| item.id)
        .expect("Parser.Literal");
    let signature = surface.function_signatures.get(&literal).expect("Literal signature");

    let [cursor_param, text_param] = signature.params.as_slice() else {
        panic!("Parser.Literal must retain its two parameters")
    };
    assert_eq!(surface.types.get(*cursor_param), Some(&TypeInfo::Named(text_cursor)));
    assert_eq!(surface.types.get(*text_param), Some(&TypeInfo::Primitive(PrimitiveType::String)));
    let Some(TypeInfo::Applied { base, args }) = surface.types.get(signature.return_type) else {
        panic!("Parser.Literal must return TextParseResult<string>")
    };
    assert_eq!(*base, text_parse_result);
    let [result_arg] = args.as_slice() else { panic!("TextParseResult must retain its string argument") };
    assert_eq!(surface.types.get(*result_arg), Some(&TypeInfo::Primitive(PrimitiveType::String)));

    let dependency_paths = [cursor_path, result_path, parser_path];
    let (_, errors) = TypeChecker::check_entry(
        &mut entry,
        &resolution,
        &[&cursor, &result, &parser],
        Some(&dependency_paths),
        Some(entry_path),
        false,
        None,
        None,
        None,
        None,
    );
    assert!(errors.is_empty(), "Parser facade call must typecheck through its exported surface: {errors:#?}");
}

#[test]
fn dependency_surface_keeps_import_aliases_lexically_scoped() {
    let root = PathBuf::from("/tmp/dependency-surface-lexical-import-alias");
    let outer_cursor_path = root.join("Outer/Cursor.bd");
    let inner_cursor_path = root.join("Inner/Cursor.bd");
    let facade_path = root.join("Facade.bd");
    let outer_cursor = parse_program("pub type TextCursor { string outer }").expect("outer cursor");
    let inner_cursor = parse_program("pub type TextCursor { string inner }").expect("inner cursor");
    let facade = parse_program(
        r#"
use Outer.Cursor as Cursor;

pub Cursor.TextCursor OuterIdentity(Cursor.TextCursor cursor) {
    return cursor;
}

mod Nested {
    use Inner.Cursor as Cursor;

    pub Cursor.TextCursor InnerIdentity(Cursor.TextCursor cursor) {
        return cursor;
    }
}
"#,
    )
    .expect("facade");
    let empty = parse_program("").expect("empty entry");

    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(
        &outer_cursor,
        &["Outer".to_owned(), "Cursor".to_owned()],
        Some(&outer_cursor_path),
    );
    resolver.collect_program_in_module(
        &inner_cursor,
        &["Inner".to_owned(), "Cursor".to_owned()],
        Some(&inner_cursor_path),
    );
    resolver.collect_program_in_module(&facade, &["Facade".to_owned()], Some(&facade_path));
    let resolution = resolver.resolve_collected_program_for_api_documentation(&empty, None);
    let surface = build_unit_type_surface(&facade, &resolution, &facade_path);

    let item_id = |name: &str, kind: ItemKind, source_path: &PathBuf| {
        resolution
            .items
            .iter()
            .find(|item| {
                item.name == name
                    && item.kind == kind
                    && item.source_path.as_ref().is_some_and(|source| crate::paths::same_file(source, source_path))
            })
            .map(|item| item.id)
            .unwrap_or_else(|| panic!("missing {name} in {}", source_path.display()))
    };
    let outer_cursor_id = item_id("TextCursor", ItemKind::Type, &outer_cursor_path);
    let inner_cursor_id = item_id("TextCursor", ItemKind::Type, &inner_cursor_path);
    let outer_identity = item_id("OuterIdentity", ItemKind::Function, &facade_path);
    let inner_identity = item_id("InnerIdentity", ItemKind::Function, &facade_path);

    for (function, expected_cursor) in [(outer_identity, outer_cursor_id), (inner_identity, inner_cursor_id)] {
        let signature = surface.function_signatures.get(&function).expect("identity signature");
        let [parameter] = signature.params.as_slice() else { panic!("identity function must retain one parameter") };
        assert_eq!(surface.types.get(*parameter), Some(&TypeInfo::Named(expected_cursor)));
        assert_eq!(surface.types.get(signature.return_type), Some(&TypeInfo::Named(expected_cursor)));
    }
}

#[test]
fn dependency_surface_rejects_import_alias_shadowed_by_local_module() {
    let root = PathBuf::from("/tmp/dependency-surface-local-module-shadow");
    let imported_cursor_path = root.join("Outer/Cursor.bd");
    let facade_path = root.join("Facade.bd");
    let imported_cursor = parse_program("pub type TextCursor { string imported }").expect("imported cursor");
    let facade = parse_program(
        r#"
use Outer.Cursor as Cursor;

mod Cursor {
    pub type TextCursor { string local }
}

pub Cursor.TextCursor Select(Cursor.TextCursor cursor) {
    return cursor;
}
"#,
    )
    .expect("facade");
    let empty = parse_program("").expect("empty entry");

    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(
        &imported_cursor,
        &["Outer".to_owned(), "Cursor".to_owned()],
        Some(&imported_cursor_path),
    );
    resolver.collect_program_in_module(&facade, &["Facade".to_owned()], Some(&facade_path));
    let resolution = resolver.resolve_collected_program_for_api_documentation(&empty, None);
    let surface = build_unit_type_surface(&facade, &resolution, &facade_path);
    let select = resolution
        .items
        .iter()
        .find(|item| {
            item.name == "Select"
                && item.kind == ItemKind::Function
                && item.source_path.as_ref().is_some_and(|source| crate::paths::same_file(source, &facade_path))
        })
        .map(|item| item.id)
        .expect("Facade.Select");

    assert!(
        !surface.function_signatures.contains_key(&select),
        "a dependency surface must not select either nominal identity when an import alias collides with a local module"
    );
}
