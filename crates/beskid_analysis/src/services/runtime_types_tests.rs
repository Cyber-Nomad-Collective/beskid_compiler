use super::{parse_program_with_source_name, resolve_and_type_program};

#[test]
fn pointer_and_word_are_real_primitive_signature_types() {
    let source = "pub pointer Identity(pointer value) { return value; }\n\
                  pub word Advance(word value) { return value + 1; }\n\
                  pub bool Same(pointer left, pointer right) { return left == right; }";
    let program = parse_program_with_source_name("Primitives.bd", source).unwrap();
    let result = resolve_and_type_program(&program);
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn module_integer_constant_is_a_literal_fact_without_a_storage_item() {
    let source = "const Capacity = 32;\npub word Size() { return Capacity; }";
    let program = parse_program_with_source_name("Constants.bd", source).unwrap();
    let (_, resolution, _) = resolve_and_type_program(&program).expect("constant reference typechecks");
    assert!(!resolution.tables.integer_constants.is_empty());
    assert!(!resolution.items.iter().any(|item| item.name == "Capacity"));
}

#[test]
fn constant_does_not_coerce_an_unrelated_value_to_pointer() {
    let source = "const Capacity = 32;\npub pointer Invalid() { return Capacity; }";
    let program = parse_program_with_source_name("Invalid.bd", source).unwrap();
    assert!(resolve_and_type_program(&program).is_err());
}

#[test]
fn pointer_builtin_parameter_does_not_shift_following_word_arguments() {
    let source = "pub pointer Offset(pointer value, word count) { memory_set(value, 0, count); return pointer_add(value, count); }";
    let program = parse_program_with_source_name("Offsets.bd", source).unwrap();
    let result = resolve_and_type_program(&program);
    assert!(result.is_ok(), "{result:?}");
}
