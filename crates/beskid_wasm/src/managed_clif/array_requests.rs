//! Consume the canonical ISLE initializer for a sized array allocation.

use std::collections::HashMap;

use cranelift_codegen::ir::{Block, StackSlot, Value};

const ELEMENT: usize = 0;
const LENGTH: usize = 1;
const DESCRIPTOR: usize = 2;
const RESERVED: usize = 3;

#[derive(Clone, Copy)]
pub(super) enum Word {
    Static { plan: usize, word: usize },
    Length(Value),
}

#[derive(Default)]
pub(super) struct Requests(HashMap<StackSlot, (Block, [Option<Word>; 4])>);

pub(super) fn word_index(byteOffset: i32) -> Option<usize> {
    match byteOffset {
        0 => Some(ELEMENT),
        8 => Some(LENGTH),
        16 => Some(DESCRIPTOR),
        24 => Some(RESERVED),
        _ => None,
    }
}

impl Requests {
    pub(super) fn store(
        &mut self,
        block: Block,
        slot: StackSlot,
        index: usize,
        word: Word,
    ) -> Result<(), &'static str> {
        let (previousBlock, words) = self.0.entry(slot).or_insert((block, [None; 4]));
        let valid = match word {
            Word::Static { word: source, .. } => index != LENGTH && source == index,
            Word::Length(_) => index == LENGTH,
        };
        if !valid || *previousBlock != block || words[index].is_some() {
            return Err("unsupported native array request initializer");
        }
        words[index] = Some(word);
        Ok(())
    }

    pub(super) fn take(&mut self, block: Block, slot: StackSlot) -> Result<(usize, Value), &'static str> {
        let Some((
            initialized,
            [
                Some(Word::Static { plan, .. }),
                Some(Word::Length(length)),
                Some(Word::Static { plan: descriptor, .. }),
                Some(Word::Static { plan: reserved, .. }),
            ],
        )) = self.0.remove(&slot)
        else {
            return Err("incomplete native array request");
        };
        if initialized != block || plan != descriptor || plan != reserved {
            return Err("native array request crossed a block boundary or mixed layouts");
        }
        Ok((plan, length))
    }
}
