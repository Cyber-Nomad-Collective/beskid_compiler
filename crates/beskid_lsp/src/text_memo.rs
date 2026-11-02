//! Small per-text memos for request work that depends on a document's text alone.
//!
//! An editor asks for the same text's code actions many times (every cursor move), and a
//! formatting request often follows for that text too. Formatting a large file or building its
//! documentation facts costs tens to hundreds of milliseconds; the memo keeps the last few
//! answers, keyed by the whole text (compared in full, so an answer never belongs to another
//! text).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// The answers for the last `capacity` distinct texts, newest last.
pub(crate) struct TextMemo<V> {
    capacity: usize,
    entries: Mutex<VecDeque<(Arc<str>, V)>>,
}

impl<V: Clone> TextMemo<V> {
    pub(crate) const fn new(capacity: usize) -> Self {
        Self { capacity, entries: Mutex::new(VecDeque::new()) }
    }

    /// The remembered answer for `text`.
    pub(crate) fn get(&self, text: &str) -> Option<V> {
        let entries = self.entries.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.iter().rev().find(|(known, _)| known.as_ref() == text).map(|(_, value)| value.clone())
    }

    pub(crate) fn insert(&self, text: &str, value: V) {
        let mut entries = self.entries.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.retain(|(known, _)| known.as_ref() != text);
        if entries.len() >= self.capacity {
            entries.pop_front();
        }
        entries.push_back((Arc::from(text), value));
    }

    /// The remembered answer for `text`, or `compute`'s (remembered). The lock is not held while
    /// `compute` runs, so two requests for a new text may both compute it.
    pub(crate) fn get_or_compute(&self, text: &str, compute: impl FnOnce() -> V) -> V {
        if let Some(value) = self.get(text) {
            return value;
        }
        let value = compute();
        self.insert(text, value.clone());
        value
    }
}

#[cfg(test)]
mod tests {
    use super::TextMemo;

    #[test]
    fn answers_are_kept_per_whole_text_and_the_oldest_go_first() {
        let memo = TextMemo::new(2);
        let mut runs = 0;
        assert_eq!(
            memo.get_or_compute("a", || {
                runs += 1;
                1
            }),
            1
        );
        assert_eq!(
            memo.get_or_compute("a", || {
                runs += 1;
                9
            }),
            1,
            "remembered"
        );
        assert_eq!(runs, 1);
        memo.insert("b", 2);
        memo.insert("c", 3);
        assert_eq!(memo.get("a"), None, "the oldest of three is gone");
        assert_eq!((memo.get("b"), memo.get("c")), (Some(2), Some(3)));
        assert_eq!(memo.get("ab"), None, "another text never shares an answer");
    }
}
