// SPDX-License-Identifier: GPL-3.0-or-later
use std::ops::Range;

/// A minimized player's queue can retain its old "now playing" row. Anchor
/// to the actual SMTC song only when its occurrence in the exposed queue is
/// unambiguous. The initial row is authoritative when it already matches.
pub fn upcoming_range(titles: &[&str], current: &str) -> Option<Range<usize>> {
    if current.trim().is_empty() {
        return None;
    }
    let index = if titles.first().is_some_and(|t| t.trim() == current.trim()) {
        0
    } else {
        let mut matches = titles
            .iter()
            .enumerate()
            .filter(|(_, title)| title.trim() == current.trim());
        let (index, _) = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        index
    };
    let start = index + 1;
    Some(start..titles.len().min(start + 5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_queue_preserves_order_and_repeated_songs() {
        let titles = ["Now", "A", "A", "B", "C", "D", "E"];
        assert_eq!(upcoming_range(&titles, "Now"), Some(1..6));
    }
    #[test]
    fn advances_cached_queue_to_the_actual_current_song() {
        let titles = [
            "Old",
            "2TONE",
            "COMË N GO",
            "PUT IT ONG",
            "M.F.U.",
            "LOCO",
            "IM YEAT",
            "More",
        ];
        assert_eq!(upcoming_range(&titles, "2TONE"), Some(2..7));
    }
    #[test]
    fn does_not_guess_an_ambiguous_or_missing_current_song() {
        assert_eq!(
            upcoming_range(&["Old", "Now", "A", "Now", "B"], "Now"),
            None
        );
        assert_eq!(upcoming_range(&["Old", "A"], "Missing"), None);
        assert_eq!(upcoming_range(&[], "Now"), None);
        assert_eq!(upcoming_range(&[""], ""), None);
    }
    #[test]
    fn empty_and_short_queues_are_not_padded() {
        assert_eq!(upcoming_range(&["Now"], "Now"), Some(1..1));
        assert_eq!(upcoming_range(&["Now", "A", "B"], "Now"), Some(1..3));
    }
}
