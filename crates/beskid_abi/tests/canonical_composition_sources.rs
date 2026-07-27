use beskid_abi::runtime_source::{CANONICAL_COMPOSITION_SOURCE_PATH, canonical_runtime_sources};

fn canonical_composition() -> String {
    canonical_runtime_sources()
        .into_iter()
        .find(|unit| unit.logical_path == CANONICAL_COMPOSITION_SOURCE_PATH)
        .unwrap_or_else(|| panic!("missing canonical composition source"))
        .source
}

#[test]
fn canonical_composition_owns_a_frozen_plan_container() {
    let composition = canonical_composition();

    assert!(composition.contains("const COMPOSITION_CONTAINER_SIZE = 64;"));
    assert!(composition.contains("const COMPOSITION_PLAN_ENTRY_MAX = 256;"));
    assert!(composition.contains("pointer container = SystemAllocate(COMPOSITION_CONTAINER_SIZE, 8);"));
    assert!(composition.contains("raw_word_store(container, NativeWord(state));"));
    assert!(composition.contains("pointer registrations = SystemAllocate(COMPOSITION_PLAN_ENTRY_MAX * 8, 8);"));
    assert!(composition.contains("raw_word_store(pointer_add(container, COMPOSITION_REGISTRATIONS_OFFSET), NativeWord(registrations));"));
    assert!(composition.contains("pub word CompositionContainerValid(pointer container)"));
    assert!(!composition.contains("pub pointer CompositionContainerCreate() { return NativePointer(0); }"));
    assert!(!composition.contains("pub bool CompositionRegister(pointer container, pointer service) { return false; }"));
}

#[test]
fn canonical_composition_admits_only_validated_plan_entries_before_activation() {
    let composition = canonical_composition();

    assert!(composition.contains("pub bool CompositionRegister(pointer container, pointer service)"));
    assert!(composition.contains("if CompositionContainerValid(container) == 0 || service == NativePointer(0) { return false; }"));
    assert!(composition.contains("if raw_word_load(pointer_add(container, COMPOSITION_STATUS_OFFSET)) != COMPOSITION_CONFIGURING { return false; }"));
    assert!(composition.contains("raw_word_store(pointer_add(registrations, count * 8), NativeWord(service));"));
    assert!(composition.contains("pub bool CompositionLaunch(pointer container)"));
    assert!(composition.contains("raw_word_store(pointer_add(container, COMPOSITION_STATUS_OFFSET), COMPOSITION_ACTIVE);"));
    assert!(composition.contains("if raw_word_load(pointer_add(container, COMPOSITION_STATUS_OFFSET)) != COMPOSITION_ACTIVE { return NativePointer(0); }"));
    assert!(!composition.contains("pub bool CompositionLaunch(pointer container) { return false; }"));
}

#[test]
fn canonical_composition_uses_container_owned_singular_and_plural_plan_order() {
    let composition = canonical_composition();

    assert!(composition.contains("pub unit CompositionBindPlural(pointer container, pointer key, pointer factory)"));
    assert!(composition.contains("raw_word_store(pointer_add(pluralBindings, count * 16), NativeWord(key));"));
    assert!(composition.contains("raw_word_store(pointer_add(pluralBindings, count * 16 + 8), NativeWord(factory));"));
    assert!(composition.contains("pub pointer CompositionResolve(pointer container, pointer key)"));
    assert!(composition.contains("if raw_word_load(pointer_add(registrations, index * 8)) == NativeWord(key)"));
    assert!(composition.contains("pub pointer CompositionResolvePlural(pointer container, pointer key)"));
    assert!(composition.contains("if raw_word_load(pointer_add(pluralBindings, index * 16)) == NativeWord(key)"));
    assert!(!composition.contains("static "));
    assert!(!composition.contains("RuntimeState() +"));
}
