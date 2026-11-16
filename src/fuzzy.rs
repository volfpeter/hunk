//! Matching what is typed against names, as fuzzy pickers do.

use std::cmp::Reverse;

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// The `items` whose `name` holds every word `typed`, each word's letters in order but not
/// necessarily together, ignoring case. The best matches come first, equal ones in the order
/// given. Nothing typed keeps every item.
pub fn rank<T>(typed: &str, items: Vec<T>, name: impl Fn(&T) -> &str) -> Vec<T> {
    let pattern = Pattern::new(typed, CaseMatching::Ignore, Normalization::Smart, AtomKind::Fuzzy);
    if pattern.atoms.is_empty() {
        return items;
    }
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let mut chars = Vec::new();
    let mut scored: Vec<(u32, T)> = items
        .into_iter()
        .filter_map(|item| {
            let score = pattern.score(Utf32Str::new(name(&item), &mut chars), &mut matcher)?;
            Some((score, item))
        })
        .collect();
    scored.sort_by_key(|(score, _)| Reverse(*score));
    scored.into_iter().map(|(_, item)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranked<'a>(typed: &str, names: &[&'a str]) -> Vec<&'a str> {
        rank(typed, names.to_vec(), |name| name)
    }

    #[test]
    fn matches_scattered_letters_of_every_word_in_any_order_ignoring_case() {
        let names = ["src/app.rs", "src/view.rs", "docs/plans/01-kickstart.md"];
        assert_eq!(ranked("apprs", &names), ["src/app.rs"]);
        assert_eq!(ranked("RS view", &names), ["src/view.rs"]);
        assert_eq!(ranked("xyz", &names), Vec::<&str>::new());
    }

    #[test]
    fn puts_the_best_match_first_and_keeps_the_order_of_equals() {
        assert_eq!(ranked("view", &["src/review.rs", "src/view.rs"]), ["src/view.rs", "src/review.rs"]);
        assert_eq!(ranked("", &["b", "a"]), ["b", "a"]);
        assert_eq!(ranked("a", &["b/a", "c/a"]), ["b/a", "c/a"]);
    }
}
