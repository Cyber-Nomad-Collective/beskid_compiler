//! Deterministic source mutation for the internal-error sweep.
//!
//! Three mutations per file, as ticket 01 describes: delete one token, duplicate one line, swap
//! two adjacent tokens. Tokens are lexical (identifier/number runs, string literals, comments,
//! single punctuation characters); whitespace is preserved so the mutant stays diffable. The
//! generator never needs to parse Beskid: a mutant that is no longer valid source is the point.

use crate::rng::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationKind {
    DeleteToken,
    DuplicateLine,
    SwapTokens,
}

impl MutationKind {
    pub const ALL: [MutationKind; 3] =
        [MutationKind::DeleteToken, MutationKind::DuplicateLine, MutationKind::SwapTokens];

    pub fn name(self) -> &'static str {
        match self {
            MutationKind::DeleteToken => "delete-token",
            MutationKind::DuplicateLine => "duplicate-line",
            MutationKind::SwapTokens => "swap-tokens",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

/// Byte span of one lexical token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
}

/// Split `source` into non-whitespace tokens.
pub fn tokenize(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        let start = index;
        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
        } else if byte == b'"' {
            index += 1;
            while index < bytes.len() && bytes[index] != b'"' && bytes[index] != b'\n' {
                if bytes[index] == b'\\' {
                    index += 1;
                }
                index += 1;
            }
            if index < bytes.len() && bytes[index] == b'"' {
                index += 1;
            }
        } else if byte == b'_' || byte.is_ascii_alphanumeric() {
            while index < bytes.len() && (bytes[index] == b'_' || bytes[index].is_ascii_alphanumeric()) {
                index += 1;
            }
        } else {
            // One punctuation character, or one whole non-ASCII scalar.
            index += 1;
            while index < bytes.len() && (bytes[index] & 0xC0) == 0x80 {
                index += 1;
            }
        }
        tokens.push(Token { start, end: index.min(bytes.len()) });
    }
    tokens
}

/// Apply one mutation. Returns `None` when the source has nothing the mutation can act on
/// (no tokens, fewer than two tokens for a swap, no non-blank line to duplicate) or when the
/// result would equal the input (swapping two identical tokens).
pub fn mutate(source: &str, kind: MutationKind, rng: &mut Rng) -> Option<String> {
    let mutant = match kind {
        MutationKind::DeleteToken => {
            let tokens = tokenize(source);
            if tokens.is_empty() {
                return None;
            }
            let token = tokens[rng.below(tokens.len())];
            format!("{}{}", &source[..token.start], &source[token.end..])
        }
        MutationKind::SwapTokens => {
            let tokens = tokenize(source);
            if tokens.len() < 2 {
                return None;
            }
            let index = rng.below(tokens.len() - 1);
            let (first, second) = (tokens[index], tokens[index + 1]);
            format!(
                "{}{}{}{}{}",
                &source[..first.start],
                &source[second.start..second.end],
                &source[first.end..second.start],
                &source[first.start..first.end],
                &source[second.end..]
            )
        }
        MutationKind::DuplicateLine => {
            let lines: Vec<&str> = source.split_inclusive('\n').collect();
            let candidates: Vec<usize> =
                lines.iter().enumerate().filter(|(_, line)| !line.trim().is_empty()).map(|(i, _)| i).collect();
            if candidates.is_empty() {
                return None;
            }
            let chosen = candidates[rng.below(candidates.len())];
            let mut out = String::with_capacity(source.len() + lines[chosen].len() + 1);
            for (i, line) in lines.iter().enumerate() {
                out.push_str(line);
                if i == chosen {
                    if !line.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str(line);
                }
            }
            out
        }
    };
    (mutant != source).then_some(mutant)
}

/// The deterministic mutant of `source` for (`seed`, `label`, `kind`), where `label` is normally
/// the file's path relative to its corpus root. Same inputs always give the same mutant.
pub fn mutant_for(source: &str, seed: u64, label: &str, kind: MutationKind) -> Option<String> {
    let mut rng = Rng::derived(seed, &format!("{label}\u{0}{}", kind.name()));
    mutate(source, kind, &mut rng)
}

/// One planned mutation: which file (by label) and which kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub label: String,
    pub kind: MutationKind,
}

/// Plan the mutants for `files` (label, source): every (file, kind) pair that actually yields a
/// mutant, in a seed-dependent order, truncated to `sample` when given. Labels are processed in
/// sorted order before shuffling, so the plan depends only on the seed and the file contents.
pub fn plan(files: &[(String, String)], seed: u64, sample: Option<usize>) -> Vec<Planned> {
    let mut sorted: Vec<&(String, String)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut planned: Vec<Planned> = sorted
        .iter()
        .flat_map(|(label, source)| {
            MutationKind::ALL
                .into_iter()
                .filter(move |kind| mutant_for(source, seed, label, *kind).is_some())
                .map(move |kind| Planned { label: label.clone(), kind })
        })
        .collect();
    let mut rng = Rng::derived(seed, "plan-order");
    for i in (1..planned.len()).rev() {
        planned.swap(i, rng.below(i + 1));
    }
    if let Some(limit) = sample {
        planned.truncate(limit);
    }
    planned
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "i32 Main() {\n    mut i32 value = 1;\n    return value;\n}\n";

    fn texts(source: &str) -> Vec<&str> {
        tokenize(source).into_iter().map(|t| &source[t.start..t.end]).collect()
    }

    #[test]
    fn tokenizer_splits_words_punctuation_and_keeps_literals_whole() {
        assert_eq!(
            texts("a.b(\"x y\", 12); // tail\n"),
            vec!["a", ".", "b", "(", "\"x y\"", ",", "12", ")", ";", "// tail"]
        );
    }

    #[test]
    fn tokenizer_handles_escaped_quotes_and_unterminated_strings() {
        assert_eq!(texts(r#""a\"b" c"#), vec![r#""a\"b""#, "c"]);
        assert_eq!(texts("\"open\nnext"), vec!["\"open", "next"]);
    }

    #[test]
    fn tokenizer_keeps_non_ascii_scalars_whole() {
        assert_eq!(texts("x \u{105}\u{107} y"), vec!["x", "\u{105}", "\u{107}", "y"]);
    }

    #[test]
    fn delete_token_removes_exactly_one_token() {
        let mutant = mutant_for(SAMPLE, 1, "Main.bd", MutationKind::DeleteToken).unwrap();
        assert_eq!(tokenize(&mutant).len(), tokenize(SAMPLE).len() - 1);
    }

    #[test]
    fn duplicate_line_adds_one_line_that_already_existed() {
        let mutant = mutant_for(SAMPLE, 1, "Main.bd", MutationKind::DuplicateLine).unwrap();
        assert_eq!(mutant.lines().count(), SAMPLE.lines().count() + 1);
        let added = mutant.lines().find(|l| mutant.lines().filter(|m| m == l).count() == 2).unwrap();
        assert!(SAMPLE.lines().any(|l| l == added));
    }

    #[test]
    fn duplicate_line_handles_a_last_line_without_newline() {
        let mutant = mutate("only", MutationKind::DuplicateLine, &mut Rng::new(0)).unwrap();
        assert_eq!(mutant, "only\nonly");
    }

    #[test]
    fn swap_tokens_keeps_the_multiset_and_changes_the_text() {
        let mutant = mutant_for(SAMPLE, 9, "Main.bd", MutationKind::SwapTokens);
        let mutant = mutant.expect("swap changes this sample");
        let mut before = texts(SAMPLE);
        let mut after = texts(&mutant);
        before.sort_unstable();
        after.sort_unstable();
        assert_eq!(before, after);
        assert_ne!(mutant, SAMPLE);
    }

    #[test]
    fn swap_of_identical_tokens_is_not_a_mutation() {
        assert_eq!(mutate("a a", MutationKind::SwapTokens, &mut Rng::new(0)), None);
    }

    #[test]
    fn empty_and_tiny_inputs_yield_none() {
        assert_eq!(mutate("", MutationKind::DeleteToken, &mut Rng::new(0)), None);
        assert_eq!(mutate("  \n", MutationKind::DuplicateLine, &mut Rng::new(0)), None);
        assert_eq!(mutate("x", MutationKind::SwapTokens, &mut Rng::new(0)), None);
    }

    #[test]
    fn mutants_are_deterministic_for_a_seed_and_label() {
        for kind in MutationKind::ALL {
            assert_eq!(mutant_for(SAMPLE, 42, "a/B.bd", kind), mutant_for(SAMPLE, 42, "a/B.bd", kind));
        }
    }

    #[test]
    fn different_seeds_reach_different_mutants() {
        let distinct: std::collections::HashSet<_> =
            (0..32).filter_map(|seed| mutant_for(SAMPLE, seed, "Main.bd", MutationKind::DeleteToken)).collect();
        assert!(distinct.len() > 3, "seeds should spread over tokens, got {}", distinct.len());
    }

    #[test]
    fn kind_names_round_trip() {
        for kind in MutationKind::ALL {
            assert_eq!(MutationKind::parse(kind.name()), Some(kind));
        }
        assert_eq!(MutationKind::parse("nope"), None);
    }

    #[test]
    fn plan_lists_every_file_and_kind_with_a_mutant_and_is_deterministic() {
        let files = vec![
            ("b/Two.bd".to_string(), SAMPLE.to_string()),
            ("a/One.bd".to_string(), SAMPLE.to_string()),
            ("empty.bd".to_string(), String::new()),
        ];
        let planned = plan(&files, 5, None);
        assert_eq!(planned.len(), 6, "two real files times three kinds, the empty file yields none");
        assert!(planned.iter().all(|p| p.label != "empty.bd"));
        assert_eq!(planned, plan(&files, 5, None));
        let mut reversed = files.clone();
        reversed.reverse();
        assert_eq!(planned, plan(&reversed, 5, None), "input order must not matter");
    }

    #[test]
    fn plan_sample_truncates() {
        let files = vec![("One.bd".to_string(), SAMPLE.to_string())];
        assert_eq!(plan(&files, 1, Some(2)).len(), 2);
        assert_eq!(plan(&files, 1, Some(0)).len(), 0);
    }
}
