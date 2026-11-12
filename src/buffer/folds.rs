//! Code folding: which lines a collapsed fold hides, and how a block is found.
//!
//! A fold is stored only while it is *collapsed*. An open block is simply not
//! in the map, which is what makes "toggle" a lookup rather than a state
//! machine: fold the line if it is absent, unfold it if it is there. Nothing
//! else has to remember that a block was once folded and then opened.
//!
//! Ranges are inclusive, `start < end`, and disjoint. `start` is the header
//! line — the one that stays on screen and carries the marker — and
//! `start + 1 ..= end` are the lines that go away.
//!
//! Folds are line-anchored, like bookmarks: they are not part of the text, so
//! they cannot be saved and they move with the lines they were put on. An edit
//! that changes the number of lines above a fold shifts it; see
//! [`Folds::shift`].

use std::collections::BTreeMap;

use super::storage::TextStorage;

/// Collapsed folds, keyed by their header line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folds {
    ranges: BTreeMap<usize, usize>,
}

impl Folds {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    /// Every fold, in line order.
    pub fn iter(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.ranges.iter().map(|(start, end)| (*start, *end))
    }

    pub fn clear(&mut self) {
        self.ranges.clear();
    }

    /// The fold that hides `line`, as `(header, last hidden line)`.
    ///
    /// A header line is not hidden by its own fold: it is the line the fold
    /// leaves behind, so it answers `None` here and `Some` from
    /// [`hidden_at`](Self::hidden_at).
    pub fn covering(&self, line: usize) -> Option<(usize, usize)> {
        let (&start, &end) = self.ranges.range(..=line).next_back()?;
        (start < line && line <= end).then_some((start, end))
    }

    pub fn is_hidden(&self, line: usize) -> bool {
        self.covering(line).is_some()
    }

    /// Last line hidden by the fold whose header is `line`, if it is collapsed.
    pub fn hidden_at(&self, line: usize) -> Option<usize> {
        self.ranges.get(&line).copied()
    }

    /// How many lines the fold at `line` hides.
    pub fn hidden_count(&self, line: usize) -> Option<usize> {
        self.ranges.get(&line).map(|end| end - line)
    }

    /// Collapse `start..=end`, absorbing any fold it touches.
    ///
    /// Absorbing rather than nesting keeps the set disjoint, and a nested fold
    /// inside a collapsed one is invisible anyway — the parent hides every line
    /// the child would have. Merging the two is the same thing said once.
    pub fn insert(&mut self, start: usize, end: usize) {
        if end <= start {
            return;
        }
        let (mut lo, mut hi) = (start, end);
        // Touching ranges merge too: two folds that between them cover a block
        // should not stay two folds that cover the same block between them.
        loop {
            let hit = self
                .ranges
                .iter()
                .find(|(s, e)| **s <= hi.saturating_add(1) && **e >= lo.saturating_sub(1))
                .map(|(&s, &e)| (s, e));
            let Some((s, e)) = hit else { break };
            lo = lo.min(s);
            hi = hi.max(e);
            self.ranges.remove(&s);
        }
        self.ranges.insert(lo, hi);
    }

    /// Expand the fold whose header is `start`. True when there was one.
    pub fn remove(&mut self, start: usize) -> bool {
        self.ranges.remove(&start).is_some()
    }

    /// Move the folds through an edit that started at `(line, col)` and changed
    /// the newline count by `delta`, leaving `line_count` lines behind.
    ///
    /// A change above a fold moves it whole; a change inside one grows or
    /// shrinks its body; a change below one leaves it alone. The one ambiguous
    /// case is a change *on* the header line, and the column settles it: a break
    /// inserted or removed at column zero means the header itself moved, while
    /// anything further along the line is the body changing shape.
    ///
    /// Folds the edit swallowed are dropped rather than left pointing past the
    /// end of the buffer.
    pub fn shift(&mut self, line: usize, col: usize, delta: isize, line_count: usize) {
        if delta == 0 || self.ranges.is_empty() {
            return;
        }
        let last = line_count.saturating_sub(1) as isize;
        let at = line as isize;
        let old = std::mem::take(&mut self.ranges);
        for (start, end) in old {
            let (start, end) = (start as isize, end as isize);
            let (start, end) = if at < start || (at == start && col == 0) {
                (start + delta, end + delta)
            } else if at <= end {
                // Inside the body: the header stays where it is and the body
                // takes the change.
                (start, end + delta)
            } else {
                (start, end)
            };
            if start < 0 || end <= start || end > last {
                continue;
            }
            self.ranges.insert(start as usize, end as usize);
        }
    }
}

/// Display columns of a line's leading whitespace.
///
/// Measured in columns rather than characters so that a tab and the run of
/// spaces it stands for compare equal — otherwise a file that mixes the two
/// would look like it changes depth between adjacent lines.
pub fn indent_columns(text: &str, tab_width: usize) -> usize {
    let mut col = 0usize;
    for c in text.chars() {
        match c {
            ' ' => col += 1,
            '\t' => col += tab_width.max(1) - col % tab_width.max(1),
            _ => break,
        }
    }
    col
}

/// Last line of the block that `line` opens, if it opens one.
///
/// The rule is indentation, which every language has and no parser is needed
/// for: the block runs while the lines are deeper than the header, and blank
/// lines inside it belong to it. Trailing blank lines do not — a fold that ends
/// with the empty lines after a function hides the gap that separates it from
/// the next one, and the file looks like it has lost a line.
///
/// A closing brace at the header's own indent is deliberately *not* folded: it
/// is the line that says where the block ends, so hiding it would defeat the
/// point of collapsing the body.
pub fn block_end(storage: &dyn TextStorage, line: usize, tab_width: usize) -> Option<usize> {
    let line_count = storage.len_lines();
    if line >= line_count {
        return None;
    }
    let header = storage.line_slice(line);
    if header.trim().is_empty() {
        return None;
    }
    let depth = indent_columns(&header, tab_width);
    let mut last = line;
    let mut probe = line + 1;
    while probe < line_count {
        let text = storage.line_slice(probe);
        if text.trim().is_empty() {
            // Blank: it may be inside the block, but it does not extend it.
            probe += 1;
            continue;
        }
        if indent_columns(&text, tab_width) <= depth {
            break;
        }
        last = probe;
        probe += 1;
    }
    (last > line).then_some(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::storage::RopeStorage;

    fn storage(text: &str) -> RopeStorage {
        RopeStorage::from_text(text)
    }

    #[test]
    fn a_block_ends_at_the_last_deeper_line() {
        let s = storage("fn f() {\n    a;\n    b;\n}\nnext\n");
        assert_eq!(block_end(&s, 0, 4), Some(2));
    }

    #[test]
    fn the_closing_brace_is_not_folded() {
        // Hiding the `}` would hide the only thing that says where the block
        // ends; the body is what folds.
        let s = storage("if x {\n    a;\n}\n");
        assert_eq!(block_end(&s, 0, 4), Some(1));
    }

    #[test]
    fn blank_lines_inside_belong_and_trailing_ones_do_not() {
        let s = storage("fn f() {\n    a;\n\n    b;\n\n}\n");
        assert_eq!(block_end(&s, 0, 4), Some(3), "the gap before `b` is inside");
        let s = storage("fn f() {\n    a;\n\n\nnext\n");
        assert_eq!(block_end(&s, 0, 4), Some(1), "the gap after `a` is not");
    }

    #[test]
    fn a_line_with_nothing_deeper_opens_nothing() {
        let s = storage("a\nb\n");
        assert_eq!(block_end(&s, 0, 4), None);
        assert_eq!(block_end(&s, 1, 4), None);
    }

    #[test]
    fn a_blank_line_opens_nothing() {
        let s = storage("\n    a;\n");
        assert_eq!(block_end(&s, 0, 4), None);
    }

    #[test]
    fn tabs_and_spaces_measure_alike() {
        let s = storage("a\n\tb\n");
        assert_eq!(block_end(&s, 0, 4), Some(1));
        let s = storage("a\n    b\n");
        assert_eq!(block_end(&s, 0, 4), Some(1));
    }

    #[test]
    fn a_hidden_line_is_not_a_header() {
        let mut f = Folds::new();
        f.insert(2, 5);
        assert_eq!(f.covering(2), None, "the header stays on screen");
        assert_eq!(f.covering(3), Some((2, 5)));
        assert_eq!(f.covering(5), Some((2, 5)));
        assert_eq!(f.covering(6), None);
        assert_eq!(f.covering(1), None);
        assert!(f.is_hidden(4));
        assert!(!f.is_hidden(2));
    }

    #[test]
    fn inserting_absorbs_what_it_overlaps() {
        let mut f = Folds::new();
        f.insert(4, 6);
        f.insert(10, 12);
        f.insert(2, 11);
        assert_eq!(f.iter().collect::<Vec<_>>(), vec![(2, 12)]);
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn inserting_leaves_distant_folds_alone() {
        let mut f = Folds::new();
        f.insert(1, 3);
        f.insert(20, 22);
        f.insert(8, 10);
        assert_eq!(
            f.iter().collect::<Vec<_>>(),
            vec![(1, 3), (8, 10), (20, 22)]
        );
    }

    #[test]
    fn an_edit_above_a_fold_moves_it() {
        let mut f = Folds::new();
        f.insert(5, 9);
        f.shift(0, 0, 2, 20);
        assert_eq!(f.iter().collect::<Vec<_>>(), vec![(7, 11)]);
        f.shift(0, 0, -2, 20);
        assert_eq!(f.iter().collect::<Vec<_>>(), vec![(5, 9)]);
    }

    #[test]
    fn an_edit_inside_a_fold_grows_it() {
        let mut f = Folds::new();
        f.insert(5, 9);
        f.shift(7, 4, 3, 20);
        assert_eq!(f.iter().collect::<Vec<_>>(), vec![(5, 12)], "the body grew");
        f.shift(7, 4, -3, 20);
        assert_eq!(f.iter().collect::<Vec<_>>(), vec![(5, 9)]);
    }

    #[test]
    fn an_edit_below_a_fold_leaves_it_alone() {
        let mut f = Folds::new();
        f.insert(5, 9);
        f.shift(15, 0, 4, 30);
        assert_eq!(f.iter().collect::<Vec<_>>(), vec![(5, 9)]);
    }

    #[test]
    fn a_break_at_the_start_of_the_header_moves_it_down() {
        // Enter at column zero pushes the header onto the next line rather than
        // growing the body: the fold has to follow the text it describes.
        let mut f = Folds::new();
        f.insert(5, 9);
        f.shift(5, 0, 1, 20);
        assert_eq!(f.iter().collect::<Vec<_>>(), vec![(6, 10)]);
    }

    #[test]
    fn a_break_inside_the_header_grows_the_body() {
        // The other half of the same line: a break after `fn f() {` is a line
        // inside the block, not a new place for the header.
        let mut f = Folds::new();
        f.insert(5, 9);
        f.shift(5, 7, 1, 20);
        assert_eq!(f.iter().collect::<Vec<_>>(), vec![(5, 10)]);
    }

    #[test]
    fn a_fold_that_the_edit_emptied_is_dropped() {
        let mut f = Folds::new();
        f.insert(5, 6);
        f.shift(6, 4, -1, 20);
        assert!(f.is_empty(), "the body went away, so the fold did too");
    }

    #[test]
    fn a_fold_past_the_new_end_is_dropped() {
        let mut f = Folds::new();
        f.insert(5, 18);
        f.shift(0, 0, -10, 9);
        assert!(f.is_empty());
    }
}
