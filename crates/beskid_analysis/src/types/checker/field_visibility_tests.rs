//! Legacy front-end parity for default field privacy (E1211): a field without `pub` is private to
//! the source unit that declares its type.

use std::path::PathBuf;

use crate::resolve::Resolver;
use crate::services::parse_program;
use crate::types::checker::TypeChecker;
use crate::types::result::TypeError;

const MODEL_SOURCE: &str = "pub type Secret { i64 hidden, pub i64 open, }";

/// Type-check `entry_source` (module `Main`) against the dependency unit `Shapes/Secret.bd`.
fn entry_errors(entry_source: &str) -> Vec<TypeError> {
    let model_path = PathBuf::from("/tmp/v06-field-visibility/src/Shapes/Secret.bd");
    let entry_path = PathBuf::from("/tmp/v06-field-visibility/src/Main.bd");
    let model = parse_program(MODEL_SOURCE).expect("model parses");
    let mut entry = parse_program(entry_source).expect("entry parses");
    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(&model, &["Shapes".into(), "Secret".into()], Some(&model_path));
    resolver.set_current_source_path(Some(entry_path.clone()));
    let resolution =
        resolver.resolve_entry_program_in_module(&entry, Some(&["Main".to_string()])).expect("entry resolves");
    let dependency_paths = [model_path];
    let (_, errors) = TypeChecker::check_entry(
        &mut entry,
        &resolution,
        &[&model],
        Some(&dependency_paths),
        Some(entry_path),
        false,
        None,
        None,
        None,
        None,
    );
    errors
}

fn inaccessible(errors: &[TypeError], field: &str) -> bool {
    errors.iter().any(|error| matches!(error, TypeError::InaccessibleStructField { name, .. } if name == field))
}

#[test]
fn cross_unit_read_of_a_private_field_is_inaccessible() {
    let errors = entry_errors("i64 Reveal(Shapes.Secret.Secret value) { return value.hidden; }");
    assert!(inaccessible(&errors, "hidden"), "expected E1211 for `hidden`: {errors:#?}");
}

#[test]
fn cross_unit_read_of_a_pub_field_is_accessible() {
    let errors = entry_errors("i64 Reveal(Shapes.Secret.Secret value) { return value.open; }");
    assert!(errors.is_empty(), "a pub field stays readable from another unit: {errors:#?}");
}

#[test]
fn cross_unit_literal_with_a_private_field_is_inaccessible() {
    let errors = entry_errors(
        "Shapes.Secret.Secret Forge() { return Shapes.Secret.Secret { hidden: 1_i64, open: 2_i64 }; }",
    );
    assert!(inaccessible(&errors, "hidden"), "expected E1211 for the supplied private field: {errors:#?}");
}

#[test]
fn same_unit_private_field_is_accessible() {
    let errors = entry_errors(
        "pub type Local { i64 hidden, } i64 Reveal(Local value) { return value.hidden; } \
         Local Make() { return Local { hidden: 1_i64 }; }",
    );
    assert!(errors.is_empty(), "the declaring unit keeps private field access: {errors:#?}");
}
