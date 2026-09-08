use std::path::PathBuf;
use std::sync::Arc;

use crate::resolve::{ItemId, ItemKind, Resolver};
use crate::services::parse_program;
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
fn generic_array_function_surface_preserves_element_parameter() {
    let dependency_path = PathBuf::from("/tmp/surface-generic-array/Core/Collections/Array.bd");
    let entry_path = PathBuf::from("/tmp/surface-generic-array/Main.bd");
    let dependency = parse_program(
        r#"
pub T[] Empty<T>() {
    return __array_new<T>(0);
}

pub i64 Len<T>(T[] values) {
    return __array_len(values);
}
"#,
    )
    .expect("parse generic array dependency");
    let entry = parse_program(
        r#"
use Core.Collections.Array;

unit Main() {
    i64[] values = Array.Empty<i64>();
    Array.Len<i64>(values);
}
"#,
    )
    .expect("parse importing entry");

    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(
        &dependency,
        &["Core".to_owned(), "Collections".to_owned(), "Array".to_owned()],
        Some(&dependency_path),
    );
    resolver.set_current_source_path(Some(entry_path));
    let resolution = resolver.resolve_program(&entry).expect("resolve importing entry");
    let surface = build_unit_type_surface(&dependency, &resolution, &dependency_path);

    let signature = |name: &str| {
        let item = resolution
            .items
            .iter()
            .find(|item| item.kind == ItemKind::Function && item.name == name)
            .map(|item| item.id)
            .unwrap_or_else(|| panic!("{name} item"));
        surface.function_signatures.get(&item).unwrap_or_else(|| panic!("{name} signature"))
    };
    let generic_array = |type_id| {
        let Some(TypeInfo::Array(element)) = surface.types.get(type_id) else {
            panic!("generic array function must retain an array type, got {:?}", surface.types.get(type_id));
        };
        assert!(matches!(surface.types.get(*element), Some(TypeInfo::GenericParam(name)) if name == "T"));
    };

    let empty = signature("Empty");
    assert!(empty.params.is_empty());
    generic_array(empty.return_type);

    let len = signature("Len");
    let [values] = len.params.as_slice() else { panic!("Len must retain its array parameter") };
    generic_array(*values);
    assert!(matches!(surface.types.get(len.return_type), Some(TypeInfo::Primitive(crate::syntax::PrimitiveType::I64))));
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
fn entry_only_cross_unit_typecheck_keeps_same_named_module_function_signatures_distinct() {
    let array_path = PathBuf::from("/tmp/entry-only-current/Core/Collections/Array.bd");
    let iterator_path = PathBuf::from("/tmp/entry-only-current/Query/ArrayIterator.bd");
    let entry_path = PathBuf::from("/tmp/entry-only-current/QueryTests.bd");
    let array = parse_program(
        r#"
pub T[] Empty<T>() {
    return __array_new<T>(0);
}

pub T Current<T>(T[] values, i64 iterator) {
    return values[iterator];
}
"#,
    )
    .expect("parse Array dependency");
    let iterator = parse_program(
        r#"
use Core.Collections.Array;

pub enum Option<T> { Some(T value), None }
pub type ArrayIterator<T> { T[] source }
pub Option<T> Current<T>(ArrayIterator<T> iterator) {
    return Option::None();
}
"#,
    )
    .expect("parse ArrayIterator dependency");
    let mut entry = parse_program(
        r#"
use Core.Collections.Array;
use Query.ArrayIterator;

unit Main() {
    ArrayIterator<i64> iterator = ArrayIterator<i64> { source: Array.Empty<i64>() };
    Option<i64> current = ArrayIterator.Current<i64>(iterator);
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

    assert!(errors.is_empty(), "qualified Current calls must retain their module signature: {errors:#?}");
}
