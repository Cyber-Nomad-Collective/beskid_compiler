use super::*;

macro_rules! generated_event_methods {
    () => {
        fn emit_event_subscribe(&mut self, _key: AstNodeKey) -> Option<Value> {
            None
        }

        fn emit_event_unsubscribe_first(&mut self, _key: AstNodeKey) -> Option<Value> {
            None
        }

        fn emit_event_raise(&mut self, _key: AstNodeKey) -> Option<Value> {
            None
        }
    };
}

pub(super) use generated_event_methods;
