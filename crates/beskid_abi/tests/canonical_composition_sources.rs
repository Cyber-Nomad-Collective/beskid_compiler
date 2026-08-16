use beskid_abi::runtime_source::{CANONICAL_COMPOSITION_SOURCE_PATH, canonical_runtime_sources};

fn canonical_composition() -> String {
    canonical_runtime_sources()
        .into_iter()
        .find(|unit| unit.logical_path == CANONICAL_COMPOSITION_SOURCE_PATH)
        .unwrap_or_else(|| panic!("missing canonical composition source"))
        .source
}

#[test]
fn canonical_composition_owns_compiler_sized_slot_storage() {
    let composition = canonical_composition();

    assert!(composition.contains("const COMPOSITION_CONTAINER_SIZE = 48;"));
    assert!(composition.contains("const COMPOSITION_PLAN_ENTRY_MAX = 256;"));
    assert!(composition.contains("pub pointer CompositionContainerCreate(word slotCount)"));
    assert!(composition.contains("raw_word_store(pointer_add(container, COMPOSITION_SLOT_COUNT_OFFSET), slotCount);"));
    assert!(composition.contains("pub word CompositionContainerValid(pointer container)"));
}

#[test]
fn canonical_composition_accepts_only_compiler_selected_immutable_slots() {
    let composition = canonical_composition();

    assert!(composition.contains("pub bool CompositionSlotStore(pointer container, word slot, pointer service)"));
    assert!(composition.contains("if slot >= slotCount || GcExternalRootCount() >= GC_EXTERNAL_ROOT_CAPACITY"));
    assert!(composition.contains("if raw_word_load(destination) != 0 { return false; }"));
    assert!(composition.contains("if GcRegisterRoot(destination) == false"));
    assert!(composition.contains("pub bool CompositionLaunch(pointer container)"));
    assert!(
        composition.contains("raw_word_store(pointer_add(container, COMPOSITION_STATUS_OFFSET), COMPOSITION_ACTIVE);")
    );
}

#[test]
fn canonical_composition_has_no_runtime_service_lookup_or_plural_registry() {
    let composition = canonical_composition();

    assert!(!composition.contains("CompositionResolve"));
    assert!(!composition.contains("CompositionResolvePlural"));
    assert!(!composition.contains("CompositionBindPlural"));
    assert!(!composition.contains("composition_resolve"));
    assert!(!composition.contains("COMPOSITION_PLURAL"));
    assert!(!composition.contains("keySlot"));
    assert!(!composition.contains("while index < count"));
}

#[test]
fn canonical_composition_preserves_scope_and_reverse_shutdown_ownership() {
    let composition = canonical_composition();

    assert!(composition.contains("pub unit CompositionScopeEnter(pointer container)"));
    assert!(composition.contains("pub unit CompositionScopeLeave()"));
    assert!(composition.contains("pub unit CompositionShutdown(pointer container)"));
    assert!(composition.contains("while activated > 0"));
    assert!(composition.contains("GcUnregisterRoot(pointer_add(slots, activated * 8));"));
    assert!(composition.contains("raw_word_load(pointer_add(container, COMPOSITION_OPEN_SCOPE_COUNT_OFFSET)) != 0"));
}
