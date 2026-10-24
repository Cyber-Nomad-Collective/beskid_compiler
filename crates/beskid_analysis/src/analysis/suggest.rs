//! "Did you mean": the closest candidate name to a misspelled one, within a bounded edit distance.

/// The candidate closest to `target` by optimal-string-alignment distance (insertions,
/// deletions, substitutions and adjacent transpositions), if one lies within the bound: a third of
/// the target's length, at least one edit. A candidate that differs only in letter case always
/// qualifies. Ties go to the lexicographically first candidate, so the answer is deterministic.
pub fn closest_name<'a>(target: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let bound = (target.chars().count() / 3).max(1);
    let mut best: Option<(usize, &str)> = None;
    for candidate in candidates {
        if candidate == target || candidate.is_empty() {
            continue;
        }
        let distance = if candidate.eq_ignore_ascii_case(target) { 0 } else { edit_distance(target, candidate) };
        if distance > bound {
            continue;
        }
        if best.is_none_or(|(best_distance, best_name)| (distance, candidate) < (best_distance, best_name)) {
            best = Some((distance, candidate));
        }
    }
    best.map(|(_, name)| name.to_string())
}

/// Optimal string alignment distance over characters.
fn edit_distance(left: &str, right: &str) -> usize {
    let (left, right): (Vec<char>, Vec<char>) = (left.chars().collect(), right.chars().collect());
    let width = right.len() + 1;
    let mut rows = vec![0usize; (left.len() + 1) * width];
    for (index, cell) in rows.iter_mut().enumerate().take(width) {
        *cell = index;
    }
    for row in 0..=left.len() {
        rows[row * width] = row;
    }
    for i in 1..=left.len() {
        for j in 1..=right.len() {
            let cost = usize::from(left[i - 1] != right[j - 1]);
            let mut value = (rows[(i - 1) * width + j] + 1)
                .min(rows[i * width + j - 1] + 1)
                .min(rows[(i - 1) * width + j - 1] + cost);
            if i > 1 && j > 1 && left[i - 1] == right[j - 2] && left[i - 2] == right[j - 1] {
                value = value.min(rows[(i - 2) * width + j - 2] + 1);
            }
            rows[i * width + j] = value;
        }
    }
    rows[left.len() * width + right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_the_closest_name_within_the_bound() {
        assert_eq!(closest_name("countr", ["counter", "total"]).as_deref(), Some("counter"));
        assert_eq!(closest_name("heigth", ["width", "height"]).as_deref(), Some("height"));
        assert_eq!(closest_name("Total", ["total"]).as_deref(), Some("total"));
        assert_eq!(closest_name("x", ["y"]).as_deref(), Some("y"));
    }

    #[test]
    fn far_names_and_the_name_itself_are_not_suggested() {
        assert_eq!(closest_name("height", ["width"]), None);
        assert_eq!(closest_name("count", ["count"]), None);
        assert_eq!(closest_name("value", Vec::<&str>::new()), None);
    }

    #[test]
    fn ties_are_deterministic() {
        assert_eq!(closest_name("cat", ["bat", "cab"]).as_deref(), Some("bat"));
        assert_eq!(closest_name("cat", ["cab", "bat"]).as_deref(), Some("bat"));
    }

    #[test]
    fn edit_distance_counts_transpositions_once() {
        assert_eq!(edit_distance("heigth", "height"), 1);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }
}
