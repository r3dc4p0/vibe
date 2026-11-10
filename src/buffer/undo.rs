//! Undo/redo history.
//!
//! Every mutation is recorded as a [`Transaction`] holding one or more
//! [`Change`]s. A transaction stores the cursor *and* selection anchor before
//! and after the edit, so undo restores exactly the state the user saw.
//!
//! Consecutive single-character typing (and consecutive backspaces) is folded
//! into one transaction, which is what makes undo feel natural instead of
//! undoing one letter at a time. Coalescing is bounded by both a time window and
//! a character budget, and is disabled across a save point so that "save, keep
//! typing, undo" cannot silently return to a state that was never saved.

use std::time::{Duration, Instant};

use super::Caret;

/// One contiguous replacement: `removed` at `start` became `inserted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub start: usize,
    pub removed: String,
    pub inserted: String,
}

impl Change {
    pub fn len_removed(&self) -> usize {
        self.removed.chars().count()
    }

    pub fn len_inserted(&self) -> usize {
        self.inserted.chars().count()
    }
}

/// Coarse classification used to decide whether two edits may be merged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxKind {
    Insert,
    DeleteBack,
    DeleteForward,
    /// A bookmark was placed or taken away.
    Bookmark,
    /// A bookmark that was already there was given another name.
    BookmarkCaption,
    Other,
}

/// One bookmark changed on a line: `before` became `after`.
///
/// `None` is "no bookmark on that line", which is what makes an addition, a
/// removal and a rename the same record seen from different ends — undo only
/// has to put `before` back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookmarkChange {
    pub line: usize,
    pub before: Option<String>,
    pub after: Option<String>,
}

impl BookmarkChange {
    fn bytes(&self) -> usize {
        self.before.as_ref().map_or(0, String::len) + self.after.as_ref().map_or(0, String::len)
    }
}

/// A single undoable step.
#[derive(Debug, Clone)]
pub struct Transaction {
    pub changes: Vec<Change>,
    /// The bookmark this step touched, if it touched one. A transaction can
    /// carry text or a bookmark but never both: an edit prunes bookmarks past
    /// the end of the document, and that consequence is not itself a step.
    pub bookmark: Option<BookmarkChange>,
    pub cursor_before: usize,
    pub anchor_before: Option<usize>,
    pub cursor_after: usize,
    pub anchor_after: Option<usize>,
    /// Secondary carets before and after the edit, so undo restores a
    /// multi-cursor session instead of collapsing it to one caret.
    pub extra_before: Vec<Caret>,
    pub extra_after: Vec<Caret>,
    pub kind: TxKind,
    pub time: Instant,
}

impl Transaction {
    /// True when this step changed the text, which is what "modified" means.
    fn text_changed(&self) -> bool {
        !self.changes.is_empty()
    }

    fn inserted_chars(&self) -> usize {
        self.changes.iter().map(Change::len_inserted).sum()
    }

    fn removed_chars(&self) -> usize {
        self.changes.iter().map(Change::len_removed).sum()
    }

    fn bytes(&self) -> usize {
        self.changes
            .iter()
            .map(|c| c.removed.len() + c.inserted.len())
            .sum::<usize>()
            + self.bookmark.as_ref().map_or(0, BookmarkChange::bytes)
    }
}

/// Bounded linear undo history.
#[derive(Debug)]
pub struct History {
    entries: std::collections::VecDeque<Transaction>,
    /// Number of applied entries. `entries[index..]` is the redo stack.
    index: usize,
    /// Applied entries that changed the text.
    ///
    /// Dirtiness is counted in these and not in entries: a bookmark is not part
    /// of the file, so placing one must not make a buffer look unsaved, and
    /// undoing one must not make it look saved. Two positions in the history
    /// with the same count differ only by bookmarks.
    text_applied: usize,
    /// `text_applied` as it was when the file was last written.
    saved_text: Option<usize>,
    /// Suppress merging into the previous entry (set after save / undo / redo).
    barrier: bool,
    limit_entries: usize,
    limit_bytes: usize,
    bytes: usize,
}

/// Typing is coalesced while the gap between keystrokes stays below this.
const COALESCE_WINDOW: Duration = Duration::from_millis(500);
/// ...and while the merged run stays below this many characters.
const COALESCE_MAX_CHARS: usize = 512;

impl Default for History {
    fn default() -> Self {
        Self::new(5_000, 64 * 1024 * 1024)
    }
}

impl History {
    pub fn new(limit_entries: usize, limit_bytes: usize) -> Self {
        Self {
            entries: std::collections::VecDeque::new(),
            index: 0,
            text_applied: 0,
            saved_text: Some(0),
            barrier: true,
            limit_entries,
            limit_bytes,
            bytes: 0,
        }
    }

    pub fn can_undo(&self) -> bool {
        self.index > 0
    }

    pub fn can_redo(&self) -> bool {
        self.index < self.entries.len()
    }

    pub fn is_dirty(&self) -> bool {
        self.saved_text != Some(self.text_applied)
    }

    /// Number of applied entries (monotonic only within a save epoch).
    pub fn depth(&self) -> usize {
        self.index
    }

    pub fn mark_saved(&mut self) {
        self.saved_text = Some(self.text_applied);
        self.barrier = true;
    }

    /// Forget the saved state (used when a buffer's file disappears).
    pub fn invalidate_save_point(&mut self) {
        self.saved_text = None;
    }

    /// Record an edit. Returns `false` when the edit was merged into the
    /// previous transaction.
    pub fn push(&mut self, mut tx: Transaction) -> bool {
        tx.time = Instant::now();
        // A new edit invalidates the redo stack.
        while self.entries.len() > self.index {
            if let Some(dropped) = self.entries.pop_back() {
                self.bytes = self.bytes.saturating_sub(dropped.bytes());
            }
        }

        if !self.barrier && self.index > 0 {
            if let Some(last) = self.entries.back_mut() {
                if let Some(merged) = try_merge(last, &tx) {
                    self.bytes = self
                        .bytes
                        .saturating_sub(last.bytes())
                        .saturating_add(merged.bytes());
                    *last = merged;
                    self.barrier = false;
                    return false;
                }
            }
        }

        self.bytes = self.bytes.saturating_add(tx.bytes());
        if tx.text_changed() {
            self.text_applied += 1;
        }
        self.entries.push_back(tx);
        self.index = self.entries.len();
        self.barrier = false;
        self.trim();
        true
    }

    /// The transaction to undo, together with the cursor state to restore.
    pub fn undo(&mut self) -> Option<Transaction> {
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        self.barrier = true;
        let tx = self.entries.get(self.index).cloned();
        if tx.as_ref().is_some_and(Transaction::text_changed) {
            self.text_applied = self.text_applied.saturating_sub(1);
        }
        tx
    }

    /// The transaction to redo.
    pub fn redo(&mut self) -> Option<Transaction> {
        if self.index >= self.entries.len() {
            return None;
        }
        let tx = self.entries.get(self.index).cloned();
        self.index += 1;
        self.barrier = true;
        if tx.as_ref().is_some_and(Transaction::text_changed) {
            self.text_applied += 1;
        }
        tx
    }

    fn trim(&mut self) {
        let mut trimmed = 0usize;
        let mut trimmed_text = 0usize;
        while self.entries.len() > self.limit_entries || self.bytes > self.limit_bytes {
            if self.entries.len() <= 1 {
                break;
            }
            if let Some(dropped) = self.entries.pop_front() {
                self.bytes = self.bytes.saturating_sub(dropped.bytes());
                if dropped.text_changed() {
                    trimmed_text += 1;
                }
                trimmed += 1;
            }
        }
        if trimmed > 0 {
            self.index = self.index.saturating_sub(trimmed);
            self.text_applied = self.text_applied.saturating_sub(trimmed_text);
            self.saved_text = match self.saved_text {
                // The saved state fell out of the history window: stay dirty
                // until the next explicit save.
                Some(s) if s < trimmed_text => None,
                Some(s) => Some(s - trimmed_text),
                None => None,
            };
        }
    }
}

/// Attempt to fold `next` into `prev`. Returns the merged transaction.
fn try_merge(prev: &Transaction, next: &Transaction) -> Option<Transaction> {
    if prev.kind != next.kind {
        return None;
    }
    if next.time.duration_since(prev.time) > COALESCE_WINDOW {
        return None;
    }
    // A caption is typed one character at a time, so a rename arrives as a run
    // of one-character steps. They fold together when they meet end to end on
    // the same line, which leaves one undo for one rename. Placements and
    // removals are never folded: an add followed by a remove meets end to end
    // too, and undoing the pair would leave nothing at all.
    if prev.kind == TxKind::BookmarkCaption {
        if let (Some(a), Some(b)) = (&prev.bookmark, &next.bookmark) {
            if prev.changes.is_empty() && next.changes.is_empty() {
                if a.line != b.line || a.after != b.before {
                    return None;
                }
                return Some(Transaction {
                    changes: Vec::new(),
                    bookmark: Some(BookmarkChange {
                        line: a.line,
                        before: a.before.clone(),
                        after: b.after.clone(),
                    }),
                    cursor_before: prev.cursor_before,
                    anchor_before: prev.anchor_before,
                    cursor_after: next.cursor_after,
                    anchor_after: next.anchor_after,
                    extra_before: prev.extra_before.clone(),
                    extra_after: next.extra_after.clone(),
                    kind: prev.kind,
                    time: next.time,
                });
            }
        }
    }
    if prev.anchor_after.is_some() || next.anchor_before.is_some() {
        return None;
    }
    // Multi-cursor runs are never coalesced: every keystroke carries its own
    // caret set, and a wrong merge would leave carets in the wrong place.
    if !prev.extra_after.is_empty() || !next.extra_before.is_empty() {
        return None;
    }
    if prev.changes.len() != 1 || next.changes.len() != 1 {
        return None;
    }
    let a = &prev.changes[0];
    let b = &next.changes[0];

    let merged = match next.kind {
        TxKind::Insert => {
            if !a.inserted.is_empty() && a.inserted.contains('\n') {
                return None;
            }
            if b.inserted.contains('\n') {
                return None;
            }
            let expected = a.start + a.len_inserted();
            if b.start != expected || !b.removed.is_empty() {
                return None;
            }
            if prev.inserted_chars() + next.inserted_chars() > COALESCE_MAX_CHARS {
                return None;
            }
            Change {
                start: a.start,
                removed: String::new(),
                inserted: format!("{}{}", a.inserted, b.inserted),
            }
        }
        TxKind::DeleteBack => {
            // Deleting backwards: the new deletion ends where the previous began.
            if a.start != b.start + b.len_removed() || !a.inserted.is_empty() {
                return None;
            }
            if a.removed.contains('\n') || b.removed.contains('\n') {
                return None;
            }
            if prev.removed_chars() + next.removed_chars() > COALESCE_MAX_CHARS {
                return None;
            }
            Change {
                start: b.start,
                removed: format!("{}{}", b.removed, a.removed),
                inserted: String::new(),
            }
        }
        TxKind::DeleteForward => {
            if a.start != b.start || !a.inserted.is_empty() || !b.inserted.is_empty() {
                return None;
            }
            if a.removed.contains('\n') || b.removed.contains('\n') {
                return None;
            }
            if prev.removed_chars() + next.removed_chars() > COALESCE_MAX_CHARS {
                return None;
            }
            Change {
                start: a.start,
                removed: format!("{}{}", a.removed, b.removed),
                inserted: String::new(),
            }
        }
        TxKind::Other | TxKind::Bookmark | TxKind::BookmarkCaption => return None,
    };

    Some(Transaction {
        changes: vec![merged],
        bookmark: None,
        cursor_before: prev.cursor_before,
        anchor_before: prev.anchor_before,
        cursor_after: next.cursor_after,
        anchor_after: next.anchor_after,
        extra_before: prev.extra_before.clone(),
        extra_after: next.extra_after.clone(),
        kind: prev.kind,
        time: next.time,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert_tx(start: usize, text: &str, kind: TxKind) -> Transaction {
        Transaction {
            changes: vec![Change {
                start,
                removed: String::new(),
                inserted: text.to_string(),
            }],
            bookmark: None,
            cursor_before: start,
            anchor_before: None,
            cursor_after: start + text.chars().count(),
            anchor_after: None,
            extra_before: Vec::new(),
            extra_after: Vec::new(),
            kind,
            time: Instant::now(),
        }
    }

    fn bookmark_tx(line: usize, before: Option<&str>, after: Option<&str>) -> Transaction {
        // The same rule the document uses: a change with something at both ends
        // is a rename, anything else is a placement or a removal.
        let kind = match (before, after) {
            (Some(_), Some(_)) => TxKind::BookmarkCaption,
            _ => TxKind::Bookmark,
        };
        Transaction {
            changes: Vec::new(),
            bookmark: Some(BookmarkChange {
                line,
                before: before.map(str::to_string),
                after: after.map(str::to_string),
            }),
            cursor_before: 0,
            anchor_before: None,
            cursor_after: 0,
            anchor_after: None,
            extra_before: Vec::new(),
            extra_after: Vec::new(),
            kind,
            time: Instant::now(),
        }
    }

    #[test]
    fn coalesces_consecutive_typing() {
        let mut h = History::default();
        h.push(insert_tx(0, "a", TxKind::Insert));
        h.push(insert_tx(1, "b", TxKind::Insert));
        h.push(insert_tx(2, "c", TxKind::Insert));
        assert_eq!(h.depth(), 1);
        let tx = h.undo().unwrap().clone();
        assert_eq!(tx.changes[0].inserted, "abc");
    }

    #[test]
    fn newline_stops_coalescing() {
        let mut h = History::default();
        h.push(insert_tx(0, "a", TxKind::Insert));
        h.push(insert_tx(1, "\n", TxKind::Insert));
        assert_eq!(h.depth(), 2);
    }

    #[test]
    fn redo_stack_cleared_on_new_edit() {
        let mut h = History::default();
        h.push(insert_tx(0, "a", TxKind::Insert));
        h.push(insert_tx(1, "\n", TxKind::Insert));
        assert_eq!(h.depth(), 2);
        h.undo();
        assert!(h.can_redo());
        h.push(insert_tx(1, "z", TxKind::Insert));
        assert!(!h.can_redo());
        assert_eq!(h.depth(), 2);
    }

    #[test]
    fn dirty_tracks_save_point() {
        let mut h = History::default();
        assert!(!h.is_dirty());
        h.push(insert_tx(0, "a", TxKind::Insert));
        assert!(h.is_dirty());
        h.mark_saved();
        assert!(!h.is_dirty());
        h.push(insert_tx(1, "b", TxKind::Insert));
        assert!(h.is_dirty());
        h.undo();
        assert!(!h.is_dirty(), "undo back to the save point is clean");
    }

    #[test]
    fn save_point_is_not_merged_across() {
        let mut h = History::default();
        h.push(insert_tx(0, "a", TxKind::Insert));
        h.mark_saved();
        h.push(insert_tx(1, "b", TxKind::Insert));
        assert_eq!(h.depth(), 2);
    }

    #[test]
    fn trimming_keeps_indices_consistent() {
        let mut h = History::new(2, usize::MAX);
        for i in 0..5 {
            h.push(insert_tx(i, "\n", TxKind::Insert));
        }
        assert!(h.entries.len() <= 2);
        assert_eq!(h.index, h.entries.len());
    }

    #[test]
    fn a_bookmark_is_undoable_but_never_makes_the_file_dirty() {
        let mut h = History::default();
        h.push(insert_tx(0, "a", TxKind::Insert));
        h.mark_saved();
        assert!(!h.is_dirty());

        h.push(bookmark_tx(3, None, Some("")));
        assert!(!h.is_dirty(), "a bookmark is not part of the file");
        assert!(h.can_undo());

        let tx = h.undo().unwrap();
        assert_eq!(tx.bookmark.unwrap().line, 3);
        assert!(!h.is_dirty());
        assert!(h.can_redo());

        h.redo();
        assert!(!h.is_dirty());
    }

    #[test]
    fn undoing_a_text_edit_under_a_bookmark_is_still_dirty() {
        let mut h = History::default();
        h.push(bookmark_tx(0, None, Some("")));
        h.mark_saved();
        h.push(insert_tx(0, "a", TxKind::Insert));
        assert!(h.is_dirty());
        h.undo();
        assert!(!h.is_dirty(), "back at the saved text");
    }

    #[test]
    fn a_typed_caption_folds_into_one_rename() {
        let mut h = History::default();
        h.push(bookmark_tx(1, None, Some("")));
        h.mark_saved();
        h.push(bookmark_tx(1, Some(""), Some("R")));
        h.push(bookmark_tx(1, Some("R"), Some("Re")));
        h.push(bookmark_tx(1, Some("Re"), Some("Rev")));
        assert_eq!(h.depth(), 2, "the rename is one step on top of the add");

        let tx = h.undo().unwrap();
        let mark = tx.bookmark.unwrap();
        assert_eq!(
            (mark.before.as_deref(), mark.after.as_deref()),
            (Some(""), Some("Rev"))
        );

        let tx = h.undo().unwrap();
        let mark = tx.bookmark.unwrap();
        assert_eq!((mark.before, mark.after), (None, Some(String::new())));
    }

    #[test]
    fn naming_a_new_bookmark_is_a_step_of_its_own() {
        let mut h = History::default();
        h.push(bookmark_tx(1, None, Some("bookmark_1")));
        h.push(bookmark_tx(1, Some("bookmark_1"), Some("R")));
        assert_eq!(h.depth(), 2, "the name is not folded into the placement");

        let mark = h.undo().unwrap().bookmark.unwrap();
        assert_eq!(
            (mark.before, mark.after),
            (Some("bookmark_1".to_string()), Some("R".to_string()))
        );
        let mark = h.undo().unwrap().bookmark.unwrap();
        assert_eq!(mark.before, None, "and the placement is still its own step");
    }

    #[test]
    fn two_bookmarks_do_not_merge_across_lines() {
        let mut h = History::default();
        h.push(bookmark_tx(1, None, Some("")));
        h.push(bookmark_tx(2, None, Some("")));
        assert_eq!(h.depth(), 2);
    }
}
