use beskid_abi::runtime_source::{CANONICAL_CALLBACKS_SOURCE_PATH, canonical_runtime_sources};

fn canonical_callbacks() -> String {
    canonical_runtime_sources()
        .into_iter()
        .find(|unit| unit.logical_path == CANONICAL_CALLBACKS_SOURCE_PATH)
        .unwrap_or_else(|| panic!("missing canonical callback source"))
        .source
}

#[test]
fn canonical_callbacks_use_a_manifest_owned_per_runtime_registry() {
    let callbacks = canonical_callbacks();

    assert!(callbacks.contains("const CALLBACK_REGISTRY_STATE_OFFSET = 24;"));
    assert!(callbacks.contains("const CALLBACK_REGISTRY_SIZE = 1056;"));
    assert!(callbacks.contains("pub pointer CallbackRegistrySlot()"));
    assert!(callbacks.contains("pointer_add(state, CALLBACK_REGISTRY_STATE_OFFSET)"));
    assert!(callbacks.contains("pointer replacement = SystemAllocate(CALLBACK_REGISTRY_SIZE, 8);"));
    assert!(callbacks.contains("memory_set(replacement, 0, CALLBACK_REGISTRY_SIZE);"));
    assert!(callbacks.contains("raw_word_store(slot, NativeWord(replacement));"));
    assert!(!callbacks.contains("pointer_add(state, 33000)"));
    assert!(!callbacks.contains("static "));
}

#[test]
fn canonical_callback_and_handler_registration_validate_before_publishing_a_snapshot() {
    let callbacks = canonical_callbacks();

    assert!(callbacks.contains("pub word CallbackRegistrationValid(pointer entries, word count)"));
    assert!(callbacks.contains("if entries == NativePointer(0) || count == 0 || count > CallbackTableMax { return 0; }"));
    assert!(callbacks.contains("if raw_word_load(entry) == 0 || raw_word_load(pointer_add(entry, 8)) == 0 { return 0; }"));
    assert!(callbacks.contains("pub pointer CallbackRegistryReplacement("));
    assert!(callbacks.contains("if CallbackRegistrationValid(entries, count) == 0 { return NativePointer(0); }"));
    assert!(callbacks.contains("raw_word_store(slot, NativeWord(replacement));"));
    assert!(callbacks.contains("pub unit BeskidRegisterHandlers(pointer entries, word count)"));
    assert!(!callbacks.contains("pub unit BeskidRegisterHandlers(pointer table, word count) {\n    return;\n}"));
}

#[test]
fn canonical_trampoline_rejects_unregistered_targets_before_publication() {
    let callbacks = canonical_callbacks();

    assert!(callbacks.contains("pub pointer CallbackLookup(pointer identity)"));
    assert!(callbacks.contains("if raw_word_load(entry) == NativeWord(identity)"));
    assert!(callbacks.contains("pointer selected = CallbackLookup(handler);"));
    assert!(callbacks.contains("if selected == NativePointer(0) { CallbackLeaveScope(scopeSlot); return; }"));
    assert!(callbacks.contains("raw_word_store(target, NativeWord(selected));"));
}
