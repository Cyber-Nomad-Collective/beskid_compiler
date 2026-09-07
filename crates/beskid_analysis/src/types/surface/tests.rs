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
