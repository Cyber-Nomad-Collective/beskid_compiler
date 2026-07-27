use beskid_abi::runtime_source::{CANONICAL_EVENTS_SOURCE_PATH, canonical_runtime_sources};

fn canonical_source(path: &str) -> String {
    canonical_runtime_sources()
        .into_iter()
        .find(|unit| unit.logical_path == path)
        .unwrap_or_else(|| panic!("missing canonical runtime source {path}"))
        .source
}

#[test]
fn canonical_events_are_lazily_owned_by_the_field_slot_and_preserve_handler_order() {
    let events = canonical_source(CANONICAL_EVENTS_SOURCE_PATH);

    assert!(events.contains("pub word EventSubscribe(pointer eventSlot, pointer handler, word capacity)"));
    assert!(events.contains("pointer event = NativePointer(raw_word_load(eventSlot));"));
    assert!(events.contains("event = SystemAllocate(16 + capacity * 8, 8);"));
    assert!(events.contains("memory_set(event, 0, 16 + capacity * 8);"));
    assert!(events.contains("raw_word_store(eventSlot, NativeWord(event));"));
    assert!(events.contains("raw_word_store(pointer_add(event, 8), count + 1);"));
    assert!(events.contains("pub word EventUnsubscribeFirst(pointer eventSlot, pointer handler)"));
    assert!(events.contains("while index + 1 < count"));
    assert!(events.contains("raw_word_store(pointer_add(event, 16 + index * 8), next);"));
    assert!(events.contains("raw_word_store(pointer_add(event, 8), count - 1);"));
    assert!(!events.contains("RuntimeState()"));
    assert!(!events.contains("EventTable"));
    assert!(!events.contains("const EventMax"));
    assert!(!events.contains("pointer_add(state, 28000)"));
}
