//! Search and replace.
//!
//! Literal search is the fast path and is delegated to the storage backend
//! (memchr over rope chunks). Regular expressions use the `regex` crate over a
//! document snapshot, which is slower and only built when the user actually
//! enables regex mode.
//!
//! Responsiveness strategy:
//!
//! * the query is only re-evaluated when it changes, not on every frame;
//! * while the query is being typed, results are capped at [`LIVE_MATCHES`], a
//!   *latency* budget: a scan runs on every keystroke, so it has to finish in
//!   milliseconds even in a multi-gigabyte file;
//! * once the search is committed, the list is completed to [`FULL_MATCHES`], a
//!   *memory* budget, on a worker thread so the editor keeps painting;
//! * next/previous navigation reuses the cached match list instead of
//!   rescanning.

use std::ops::Range;

use regex::RegexBuilder;

use crate::buffer::{Document, Snapshot, TextStorage};
use crate::error::{Error, Result};

/// Matches collected while the query is still being typed.
///
/// This is a latency budget, not a memory one. A three-letter word that occurs
/// three million times in a 2 GB file reaches 20 000 hits in ~11 ms, which is
/// below the point where typing feels sticky; scanning the whole file would
/// take 1.7 s *per keystroke*.
pub const LIVE_MATCHES: usize = 20_000;

/// Matches collected once the search is committed (Enter, F3, Replace All).
///
/// This is a memory budget: a hit is a `Range<usize>`, so four million hits is
/// about 64 MB. It is high enough that real queries are complete — `let` in a
/// 2 GB file is 3.3 M hits — while a pathological one (`e` in the same file is
/// some 200 M) still stops instead of exhausting memory.
pub const FULL_MATCHES: usize = 4_000_000;

/// A search query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

impl Query {
    pub fn literal(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Toggle that affects the result set; used to decide when to re-scan.
    pub fn options(&self) -> (bool, bool, bool) {
        (self.case_sensitive, self.whole_word, self.regex)
    }
}

/// What one scan produced.
///
/// `truncated` is the important half: it says "there may be more matches in the
/// file", which is what the committed full scan is started for. It is *not* the
/// same thing as "the list is full" — a whole-word scan can throw away most of
/// its hits and still have text it never looked at.
#[derive(Debug, Clone, Default)]
pub struct Scan {
    pub ranges: Vec<Range<usize>>,
    /// True when the scan stopped before the end of the document.
    pub truncated: bool,
}

/// A completed search.
#[derive(Debug, Clone, Default)]
pub struct MatchSet {
    pub ranges: Vec<Range<usize>>,
    /// Index into `ranges` of the focused match.
    pub current: Option<usize>,
    /// True when the scan stopped at [`MAX_MATCHES`].
    pub truncated: bool,
    /// Query error (invalid regex) to display instead of results.
    pub error: Option<String>,
}

impl MatchSet {
    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    pub fn current_range(&self) -> Option<Range<usize>> {
        self.current.and_then(|i| self.ranges.get(i).cloned())
    }

    /// Human readable counter for the status bar.
    ///
    /// `searching` says a full scan is still running, which is the only reason
    /// a truncated count can be trusted to change on its own — the trailing
    /// ellipsis is what stops "20000+" from reading as a final answer.
    pub fn counter(&self, searching: bool) -> String {
        if let Some(err) = &self.error {
            return err.clone();
        }
        if self.ranges.is_empty() {
            return "no matches".to_string();
        }
        let pos = self.current.map(|i| i + 1).unwrap_or(0);
        match (self.truncated, searching) {
            (true, true) => format!("{pos} of {}+ …", self.ranges.len()),
            (true, false) => format!("{pos} of {}+", self.ranges.len()),
            (false, _) => format!("{pos} of {}", self.ranges.len()),
        }
    }
}

/// The three things search needs from a document, over either a live buffer or
/// a frozen [`Snapshot`].
///
/// A snapshot exists so a committed search can run on a worker thread without
/// blocking the editor; it offers the same scans but not the editing half of
/// [`TextStorage`], hence this smaller trait rather than a second copy of
/// [`find_all`].
trait Source {
    fn find_literal(&self, needle: &str, case_sensitive: bool, limit: usize) -> Vec<usize>;
    fn char_at(&self, idx: usize) -> Option<char>;
    fn to_text(&self) -> String;
}

struct Live<'a>(&'a dyn TextStorage);

impl Source for Live<'_> {
    fn find_literal(&self, needle: &str, case_sensitive: bool, limit: usize) -> Vec<usize> {
        self.0.find_literal(needle, case_sensitive, limit)
    }
    fn char_at(&self, idx: usize) -> Option<char> {
        self.0.char_at(idx)
    }
    fn to_text(&self) -> String {
        self.0.as_string()
    }
}

struct Frozen<'a>(&'a Snapshot);

impl Source for Frozen<'_> {
    fn find_literal(&self, needle: &str, case_sensitive: bool, limit: usize) -> Vec<usize> {
        self.0.find_literal(needle, case_sensitive, limit)
    }
    fn char_at(&self, idx: usize) -> Option<char> {
        self.0.char_at(idx)
    }
    fn to_text(&self) -> String {
        self.0.to_string_lossy()
    }
}

/// Find every match of `query` in `storage`.
///
/// The returned [`Scan`] also says whether the scan reached the end of the
/// document, so a caller can tell "these are all of them" from "these are as
/// many as the budget allowed".
pub fn find_all(storage: &dyn TextStorage, query: &Query, limit: usize) -> Result<Scan> {
    find_in(&Live(storage), query, limit)
}

/// Find every match of `query` in a frozen snapshot.
///
/// Used by the committed-search worker, which cannot borrow the document.
pub fn find_all_in_snapshot(snapshot: &Snapshot, query: &Query, limit: usize) -> Result<Scan> {
    find_in(&Frozen(snapshot), query, limit)
}

fn find_in(source: &dyn Source, query: &Query, limit: usize) -> Result<Scan> {
    if query.is_empty() {
        return Ok(Scan::default());
    }
    if query.regex {
        return find_regex(source, query, limit);
    }

    // Whole-word filtering throws some hits away after the scan, so the scan
    // has to overshoot to still fill `limit`. Without the filter the overshoot
    // is pure waste: it quadrupled the cost of every keystroke.
    let scan_limit = if query.whole_word {
        limit.saturating_mul(4)
    } else {
        limit
    };
    let starts = source.find_literal(&query.text, query.case_sensitive, scan_limit);
    // `find_literal` stops at its own budget, so filling it means there is text
    // behind the window that was never looked at.
    let raw_capped = scan_limit > 0 && starts.len() >= scan_limit;
    let len = query.text.chars().count();

    // The filter runs *before* the budget is applied. Taking `limit` raw starts
    // first and filtering afterwards meant that a file whose first `limit` hits
    // are all sub-word hits — `foo` in a run of `foobar` — reported "no
    // matches" while bare matches sat further down, and nothing downstream
    // could notice: the list looked short rather than capped, so no committed
    // scan ever ran to correct it.
    let mut ranges = Vec::new();
    let mut overflowed = false;
    for start in starts {
        let range = start..start + len;
        if query.whole_word && !is_whole_word(source, &range) {
            continue;
        }
        if ranges.len() == limit {
            overflowed = true;
            break;
        }
        ranges.push(range);
    }

    Ok(Scan {
        ranges,
        // Either the raw scan was cut short, or the filter kept more hits than
        // the budget allows. Both mean "there is more in this file".
        truncated: raw_capped || overflowed,
    })
}

fn is_whole_word(source: &dyn Source, range: &Range<usize>) -> bool {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    if range.start > 0 {
        if let Some(prev) = source.char_at(range.start - 1) {
            if is_word(prev) {
                return false;
            }
        }
    }
    if let Some(next) = source.char_at(range.end) {
        if is_word(next) {
            return false;
        }
    }
    true
}

fn find_regex(source: &dyn Source, query: &Query, limit: usize) -> Result<Scan> {
    let re = RegexBuilder::new(&query.text)
        .case_insensitive(!query.case_sensitive)
        .multi_line(true)
        .build()
        .map_err(|e| Error::Regex(first_line(&e.to_string())))?;

    let text = source.to_text();
    // Byte offsets from `regex` must become character offsets. One pass over the
    // string keeps this linear even for thousands of matches.
    let mut out = Vec::new();
    let mut truncated = false;
    let mut byte_cursor = 0usize;
    let mut char_cursor = 0usize;
    for m in re.find_iter(&text) {
        // Advance the character cursor to the match start.
        while byte_cursor < m.start() {
            byte_cursor += char_len_at(&text, byte_cursor);
            char_cursor += 1;
        }
        let start = char_cursor;
        let mut end_char = char_cursor;
        let mut b = m.start();
        while b < m.end() {
            b += char_len_at(&text, b);
            end_char += 1;
        }
        if end_char == start {
            continue;
        }
        let range = start..end_char;
        // Same order as the literal path: the whole-word filter runs before the
        // budget, so a screenful of sub-word hits cannot hide the bare matches
        // behind them.
        if query.whole_word && !is_whole_word(source, &range) {
            continue;
        }
        if out.len() == limit {
            truncated = true;
            break;
        }
        out.push(range);
    }
    Ok(Scan {
        ranges: out,
        truncated,
    })
}

fn char_len_at(text: &str, byte: usize) -> usize {
    text[byte..]
        .chars()
        .next()
        .map(|c| c.len_utf8())
        .unwrap_or(1)
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or(s).to_string()
}

/// Expand `$1` / `$name` style backreferences in a replacement string.
///
/// Literal mode treats the replacement verbatim; regex mode expands captures.
/// `query` is the search the match came from, and it has to be the whole query
/// rather than just its pattern: the capture groups are re-resolved by running
/// the pattern again, so the flags have to be the ones the search used. A
/// case-insensitive search expanded with a case-sensitive pattern finds no
/// captures at all and inserts `$1` verbatim.
pub fn expand_replacement(
    replacement: &str,
    haystack: &str,
    range: &Range<usize>,
    query: Option<&Query>,
) -> String {
    let Some(query) = query else {
        return replacement.to_string();
    };
    if !replacement.contains('$') {
        return replacement.to_string();
    }
    // Re-run the pattern against the matched slice to resolve captures. Mirrors
    // `find_regex`, flag for flag.
    let Ok(re) = RegexBuilder::new(&query.text)
        .case_insensitive(!query.case_sensitive)
        .multi_line(true)
        .build()
    else {
        return replacement.to_string();
    };
    let slice: String = haystack
        .chars()
        .skip(range.start)
        .take(range.end - range.start)
        .collect();
    let Some(caps) = re.captures(&slice) else {
        return replacement.to_string();
    };
    let mut out = String::new();
    let mut chars = replacement.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        // `$$` is a literal dollar.
        if chars.peek() == Some(&'$') {
            chars.next();
            out.push('$');
            continue;
        }
        let mut name = String::new();
        while let Some(&n) = chars.peek() {
            if n.is_alphanumeric() || n == '_' {
                name.push(n);
                chars.next();
            } else {
                break;
            }
        }
        if name.is_empty() {
            out.push('$');
            continue;
        }
        let value = if let Ok(idx) = name.parse::<usize>() {
            caps.get(idx).map(|m| m.as_str()).unwrap_or("")
        } else {
            caps.name(&name).map(|m| m.as_str()).unwrap_or("")
        };
        out.push_str(value);
    }
    out
}

/// What a committed scan was started for.
///
/// A worker thread cannot borrow the document, so the result comes back to a
/// state that may have moved on. The ticket is what "moved on" is measured
/// against: the search generation, the document, and the document's revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ticket {
    /// Search generation at the moment the job was started.
    pub generation: u64,
    /// [`Document::id`] of the file that was scanned.
    pub doc: usize,
    /// [`Document::revision`] of that file at the moment the job was started.
    pub revision: u64,
}

impl Ticket {
    /// True when the scan this ticket was issued for still describes `doc`.
    ///
    /// Both halves matter. The id catches a result arriving after the user
    /// switched tabs — revision numbers are per-document, so two files can both
    /// be at revision 7. The revision catches an edit made while the scan ran,
    /// which leaves every position in the result pointing at the wrong text.
    pub fn describes(&self, doc: &Document) -> bool {
        self.doc == doc.id() && self.revision == doc.revision()
    }
}

/// Live search state owned by the application.
#[derive(Debug, Default)]
pub struct SearchState {
    pub query: Query,
    pub matches: MatchSet,
    /// Document revision the matches were computed from.
    scanned_revision: u64,
    /// Document the matches were computed from.
    ///
    /// Revisions are per-document, so the revision alone cannot tell "the same
    /// file, unchanged" from "a different file that happens to be at the same
    /// revision" — which is exactly the case when the user switches tabs.
    scanned_doc: Option<usize>,
    /// Query the matches were computed from.
    scanned_query: Query,
    pub active: bool,
    /// Cursor position when the search session started.
    ///
    /// Incremental search must not use the live cursor to pick the "current"
    /// match: selecting a match moves the cursor to its end, so the next
    /// keystroke would then pick the *following* match and the view would
    /// creep forward while typing. Anchoring on the origin keeps the highlight
    /// stable until the user navigates explicitly.
    pub origin: usize,
    /// Bumped by every rescan. A committed scan is tagged with the generation
    /// it started from, so a result that arrives after the query or the text
    /// moved on is discarded rather than installed.
    generation: u64,
    /// A committed (full) scan is running on a worker thread.
    searching: bool,
    /// The current query has already been scanned with the full budget.
    ///
    /// Distinct from `truncated`: a query that matches more than
    /// [`FULL_MATCHES`] stays truncated after the committed scan, and without
    /// this flag every subsequent `next` would start that scan again.
    completed: bool,
}

impl SearchState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Recompute matches when the document or the query changed.
    ///
    /// Returns `true` when a rescan happened.
    pub fn refresh(&mut self, doc: &Document) -> bool {
        if !self.active && self.query.is_empty() {
            return false;
        }
        let unchanged = self.scanned_revision == doc.revision()
            && self.scanned_doc == Some(doc.id())
            && self.scanned_query == self.query
            && !self.matches.ranges.is_empty();
        if unchanged {
            return false;
        }
        self.rescan(doc);
        true
    }

    /// Force a rescan with the live (typing) budget.
    pub fn rescan(&mut self, doc: &Document) {
        self.rescan_with(doc, LIVE_MATCHES);
    }

    /// Force a rescan with an explicit match budget.
    pub fn rescan_with(&mut self, doc: &Document, limit: usize) {
        self.scanned_revision = doc.revision();
        self.scanned_doc = Some(doc.id());
        self.scanned_query = self.query.clone();
        self.generation += 1;
        self.searching = false;
        // A scan that already spent the full budget does not need doing twice.
        self.completed = limit >= FULL_MATCHES;

        if self.query.is_empty() {
            self.matches = MatchSet::default();
            return;
        }

        match find_all(doc.storage(), &self.query, limit) {
            Ok(scan) => {
                // The scan decides this, not the list length: a whole-word scan
                // can hand back a short list that is still incomplete.
                let truncated = scan.truncated;
                self.matches = MatchSet {
                    ranges: scan.ranges,
                    current: None,
                    truncated,
                    error: None,
                };
                self.select_nearest(doc.cursor());
            }
            Err(e) => {
                self.matches = MatchSet {
                    ranges: Vec::new(),
                    current: None,
                    truncated: false,
                    error: Some(e.to_string()),
                };
            }
        }
    }

    /// Tag to hand a committed scan, or `None` when one is already running,
    /// when the query has already had its full scan, or when there is nothing
    /// left to find.
    pub fn start_complete(&mut self, doc: &Document) -> Option<Ticket> {
        if !self.matches.truncated || self.searching || self.completed || self.query.is_empty() {
            return None;
        }
        self.searching = true;
        Some(Ticket {
            generation: self.generation,
            doc: doc.id(),
            revision: doc.revision(),
        })
    }

    /// Install the result of a committed scan.
    ///
    /// Returns `false` when the result is stale. The focused match is preserved
    /// by identity: the full list is a superset of the truncated one in the same
    /// order, so the highlight does not jump when the count grows.
    ///
    /// "Stale" is all three halves of the ticket: a newer rescan bumped the
    /// generation, the matches on screen belong to a different document, or the
    /// document has been edited since the scan started. Comparing only the
    /// generation is not enough — a result for the file that was open when the
    /// job started would otherwise land in the file the user is looking at now,
    /// and revision numbers are per-document, so "same revision" does not mean
    /// "same file".
    pub fn install(&mut self, scan: Scan, ticket: Ticket, doc: &Document) -> bool {
        if ticket.generation != self.generation
            || self.scanned_doc != Some(ticket.doc)
            || !ticket.describes(doc)
        {
            return false;
        }
        let focused = self.current();
        self.matches = MatchSet {
            ranges: scan.ranges,
            current: None,
            truncated: scan.truncated,
            error: None,
        };
        self.matches.current = focused
            .and_then(|r| self.matches.ranges.iter().position(|x| *x == r))
            .or_else(|| {
                self.select_nearest(doc.cursor());
                self.matches.current
            });
        self.searching = false;
        self.completed = true;
        true
    }

    /// True while a committed scan is running.
    pub fn searching(&self) -> bool {
        self.searching
    }

    /// Forget a committed scan whose result is being thrown away.
    ///
    /// Without this the "still searching" marker would outlive the job it
    /// describes, because nothing else knows the job is gone.
    pub fn abandon(&mut self, ticket: Ticket) {
        if ticket.generation == self.generation {
            self.searching = false;
        }
    }

    /// Counter text for the status bar and the find bar.
    pub fn counter(&self) -> String {
        self.matches.counter(self.searching)
    }

    /// True when `pos` is inside the focused match, end included.
    ///
    /// The end counts because selecting a match leaves the caret *after* it, and
    /// that caret is what says "I am still on this match".
    pub fn focus_covers(&self, pos: usize) -> bool {
        self.current()
            .is_some_and(|r| r.start <= pos && pos <= r.end)
    }

    /// True when the focused match is the last one the live scan found.
    ///
    /// Wrapping from here would jump back to the top of the file and hide the
    /// tail, which is exactly the bug this whole mechanism exists to fix — so
    /// while the rest of the document is still being scanned, `find_next` waits
    /// instead of wrapping.
    pub fn at_scan_end(&self) -> bool {
        self.matches.truncated
            && self.matches.current == Some(self.matches.ranges.len().saturating_sub(1))
    }

    /// The mirror of [`Self::at_scan_end`] for backwards navigation.
    pub fn at_scan_start(&self) -> bool {
        self.matches.truncated && self.matches.current == Some(0)
    }

    /// Focus the match the cursor is on, or the first one after it.
    ///
    /// "On" includes the end, because selecting a match leaves the caret there.
    /// A match that contains the position wins over one that merely starts
    /// later: the position is what the user is looking at, and opening find
    /// from the middle of a word should keep that occurrence rather than jump
    /// to the next one.
    pub fn select_nearest(&mut self, cursor: usize) {
        if self.matches.ranges.is_empty() {
            self.matches.current = None;
            return;
        }
        let idx = self
            .matches
            .ranges
            .iter()
            .position(|r| r.start <= cursor && cursor <= r.end)
            .or_else(|| self.matches.ranges.iter().position(|r| r.start >= cursor))
            .unwrap_or(0);
        self.matches.current = Some(idx);
    }

    /// Point the focus at the match the cursor is standing *before* (or after,
    /// going backwards), so the following step lands on the match nearest the
    /// cursor rather than on whatever was focused last.
    ///
    /// `current` is the match the cursor is on, and stepping happens from
    /// there, so this deliberately selects the neighbour and not the match the
    /// cursor is already at. `None` means "before the first match", which
    /// [`Self::next_match`] turns into index 0.
    pub fn anchor_for(&mut self, cursor: usize, forward: bool) {
        if self.matches.ranges.is_empty() {
            self.matches.current = None;
            return;
        }
        let len = self.matches.ranges.len();
        self.matches.current = if forward {
            match self.matches.ranges.iter().position(|r| r.start >= cursor) {
                Some(0) => None,
                Some(i) => Some(i - 1),
                // Past the last match: wrap to the first on the next step.
                None => Some(len - 1),
            }
        } else {
            match self.matches.ranges.iter().position(|r| r.end > cursor) {
                Some(i) => Some(i),
                None => Some(0),
            }
        };
    }

    /// Advance to the next match, wrapping.
    pub fn next_match(&mut self) -> Option<Range<usize>> {
        let len = self.matches.ranges.len();
        if len == 0 {
            return None;
        }
        let idx = match self.matches.current {
            Some(i) => (i + 1) % len,
            None => 0,
        };
        self.matches.current = Some(idx);
        self.matches.ranges.get(idx).cloned()
    }

    /// Go to the previous match, wrapping.
    pub fn prev_match(&mut self) -> Option<Range<usize>> {
        let len = self.matches.ranges.len();
        if len == 0 {
            return None;
        }
        let idx = match self.matches.current {
            Some(0) | None => len - 1,
            Some(i) => i - 1,
        };
        self.matches.current = Some(idx);
        self.matches.ranges.get(idx).cloned()
    }

    pub fn clear(&mut self) {
        self.query = Query::default();
        self.matches = MatchSet::default();
        self.scanned_query = Query::default();
        self.scanned_revision = 0;
        self.scanned_doc = None;
        self.origin = 0;
        // Invalidate any committed scan still in flight.
        self.generation += 1;
        self.searching = false;
        self.completed = false;
    }

    /// Current match range, if any.
    pub fn current(&self) -> Option<Range<usize>> {
        self.matches.current_range()
    }

    /// True when `pos` falls inside a match (used to highlight).
    pub fn ranges(&self) -> &[Range<usize>] {
        &self.matches.ranges
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Document;

    fn doc(text: &str) -> Document {
        Document::from_text(text, None)
    }

    /// The matches only — most tests do not care whether the scan was capped.
    fn found(d: &Document, q: &Query, limit: usize) -> Vec<Range<usize>> {
        find_all(d.storage(), q, limit).unwrap().ranges
    }

    #[test]
    fn finds_literal_matches() {
        let d = doc("one two one two");
        let q = Query::literal("one");
        assert_eq!(found(&d, &q, 100), vec![0..3, 8..11]);
    }

    #[test]
    fn case_insensitive_search() {
        let d = doc("One ONE one");
        let mut q = Query::literal("one");
        q.case_sensitive = false;
        assert_eq!(found(&d, &q, 100).len(), 3);
        q.case_sensitive = true;
        assert_eq!(found(&d, &q, 100).len(), 1);
    }

    #[test]
    fn whole_word_filters_partial_matches() {
        let d = doc("foo foobar barfoo foo");
        let mut q = Query::literal("foo");
        q.whole_word = true;
        assert_eq!(found(&d, &q, 100), vec![0..3, 18..21]);
    }

    #[test]
    fn whole_word_filters_before_it_truncates() {
        // Twenty sub-word hits and then one bare match. The budget is ten, so
        // taking the first ten raw hits and filtering afterwards leaves nothing
        // — the bare `foo` at the end was never reached. Filtering first finds
        // it, and the scan is complete, not truncated: `find_literal` walked the
        // whole document (its own budget is four times the list budget).
        let d = doc(&format!("{}foo", "foobar ".repeat(20)));
        let mut q = Query::literal("foo");
        q.whole_word = true;
        let scan = find_all(d.storage(), &q, 10).unwrap();
        assert_eq!(scan.ranges, vec![140..143]);
        assert!(!scan.truncated, "the whole document was scanned");
    }

    #[test]
    fn a_filtered_down_scan_still_reports_that_more_may_follow() {
        // More sub-word hits than even the overshooting scan collects, so the
        // bare match at the end is still out of reach. The list is empty, but
        // it must not claim to be the answer: `truncated` is what makes the
        // committed scan run and find it.
        let d = doc(&format!("{}foo", "foobar ".repeat(60)));
        let mut q = Query::literal("foo");
        q.whole_word = true;
        let scan = find_all(d.storage(), &q, 10).unwrap();
        assert!(scan.ranges.is_empty(), "the bare match is out of reach");
        assert!(
            scan.truncated,
            "an empty list from a cut-short scan is not 'no matches'"
        );
    }

    #[test]
    fn whole_word_regex_filters_before_it_truncates() {
        let d = doc(&format!("{}foo", "foobar ".repeat(20)));
        let mut q = Query::literal("foo");
        q.regex = true;
        q.whole_word = true;
        assert_eq!(found(&d, &q, 10), vec![140..143]);
    }

    #[test]
    fn regex_search_and_captures() {
        let d = doc("a1 b22 c333");
        let mut q = Query::literal(r"(\d+)");
        q.regex = true;
        let ranges = found(&d, &q, 100);
        assert_eq!(ranges, vec![1..2, 4..6, 8..11]);

        let text = d.text();
        let mut pattern = Query::literal(r"(\d+)");
        pattern.regex = true;
        let expanded = expand_replacement("<$1>", &text, &ranges[2], Some(&pattern));
        assert_eq!(expanded, "<333>");
    }

    #[test]
    fn invalid_regex_reports_error() {
        let d = doc("x");
        let mut q = Query::literal("(");
        q.regex = true;
        assert!(find_all(d.storage(), &q, 100).is_err());
    }

    #[test]
    fn unicode_offsets_are_chars() {
        let d = doc("héllo héllo");
        let q = Query::literal("héllo");
        assert_eq!(found(&d, &q, 100), vec![0..5, 6..11]);
    }

    #[test]
    fn unicode_matches_do_not_overlap() {
        // A non-ASCII query takes the character-window scanner rather than the
        // byte scanner, and that one used to step forward by a single character
        // after a hit: "😀😀" in "😀😀😀" reported 0..2 *and* 1..3, which
        // double-counted in the status bar and gave replace-all overlapping
        // edits to apply.
        let d = doc("😀😀😀");
        let q = Query::literal("😀😀");
        assert_eq!(found(&d, &q, 100), vec![0..2]);
    }

    #[test]
    fn ascii_matches_do_not_overlap_either() {
        let d = doc("abab");
        let q = Query::literal("ab");
        assert_eq!(found(&d, &q, 100), vec![0..2, 2..4]);
    }

    #[test]
    fn navigation_wraps() {
        let mut d = doc("a a a");
        let mut s = SearchState::new();
        s.query = Query::literal("a");
        s.active = true;
        s.rescan(&d);
        assert_eq!(s.matches.len(), 3);
        s.select_nearest(0);
        assert_eq!(s.current(), Some(0..1));
        assert_eq!(s.next_match(), Some(2..3));
        assert_eq!(s.next_match(), Some(4..5));
        assert_eq!(s.next_match(), Some(0..1), "wraps to the first match");
        assert_eq!(s.prev_match(), Some(4..5), "wraps backwards");
        let _ = &mut d;
    }

    #[test]
    fn refresh_skips_unchanged_document() {
        let d = doc("abc abc");
        let mut s = SearchState::new();
        s.query = Query::literal("abc");
        s.active = true;
        assert!(s.refresh(&d));
        assert!(!s.refresh(&d), "no rescan when nothing changed");
    }

    #[test]
    fn refresh_reacts_to_edits() {
        let mut d = doc("abc");
        let mut s = SearchState::new();
        s.query = Query::literal("abc");
        s.active = true;
        assert!(s.refresh(&d));
        assert_eq!(s.matches.len(), 1);
        d.set_cursor(3);
        d.insert_text(" abc");
        assert!(s.refresh(&d));
        assert_eq!(s.matches.len(), 2);
    }

    #[test]
    fn completing_a_truncated_scan_keeps_the_focused_match() {
        let d = doc(&"a ".repeat(100));
        let mut s = SearchState::new();
        s.query = Query::literal("a");
        s.active = true;
        s.rescan_with(&d, 10);
        assert_eq!(s.matches.len(), 10);
        assert!(s.matches.truncated);
        s.matches.current = Some(3);

        let ticket = s
            .start_complete(&d)
            .expect("a truncated scan can be completed");
        let full = find_all(d.storage(), &s.query, FULL_MATCHES).unwrap();
        assert_eq!(full.ranges.len(), 100);
        assert!(s.install(full, ticket, &d));

        assert_eq!(s.matches.len(), 100, "the tail is now in the list");
        assert!(!s.matches.truncated);
        assert_eq!(s.matches.current, Some(3), "the highlight does not move");
        assert!(!s.searching(), "and the scan is no longer advertised");
    }

    #[test]
    fn a_scan_that_arrives_after_the_query_moved_on_is_dropped() {
        let d = doc(&"a ".repeat(100));
        let mut s = SearchState::new();
        s.query = Query::literal("a");
        s.active = true;
        s.rescan_with(&d, 10);
        let stale = s.start_complete(&d).expect("truncated");

        // Typing again invalidates the scan that is already running.
        s.query = Query::literal("a ");
        s.rescan_with(&d, 10);

        let later = find_all(d.storage(), &s.query, FULL_MATCHES).unwrap();
        assert!(!s.install(later, stale, &d));
        assert_eq!(s.matches.len(), 10, "the live results survive");
    }

    #[test]
    fn a_scan_for_another_document_is_dropped() {
        // Two files at the same revision. The ticket records which one the scan
        // was started for, so a result that arrives after the user switched
        // tabs cannot land in the file on screen.
        let first = doc(&"a ".repeat(100));
        let second = doc(&"a ".repeat(100));
        assert_eq!(
            first.revision(),
            second.revision(),
            "the revision alone cannot tell these apart"
        );

        let mut s = SearchState::new();
        s.query = Query::literal("a");
        s.active = true;
        s.rescan_with(&first, 10);
        let ticket = s.start_complete(&first).expect("truncated");

        // The user switched tabs; the search is rescanned against the new file.
        s.rescan_with(&second, 10);
        let scan = find_all(second.storage(), &s.query, FULL_MATCHES).unwrap();
        assert!(!s.install(scan, ticket, &second), "wrong document");
        assert_eq!(s.matches.len(), 10, "the live results survive");
    }

    #[test]
    fn a_scan_of_an_edited_document_is_dropped() {
        let mut d = doc(&"a ".repeat(100));
        let mut s = SearchState::new();
        s.query = Query::literal("a");
        s.active = true;
        s.rescan_with(&d, 10);
        let ticket = s.start_complete(&d).expect("truncated");

        // The text moved on without a rescan (the search was not active), so
        // every position in the result now points at the wrong text.
        s.active = false;
        d.set_cursor(0);
        d.insert_text("a ");
        s.active = true;

        let scan = find_all(d.storage(), &s.query, FULL_MATCHES).unwrap();
        assert!(!s.install(scan, ticket, &d), "the positions are stale");
    }

    #[test]
    fn the_counter_marks_a_scan_that_is_still_running() {
        let d = doc(&"a ".repeat(100));
        let mut s = SearchState::new();
        s.query = Query::literal("a");
        s.active = true;
        s.rescan_with(&d, 10);
        s.matches.current = Some(0);

        assert_eq!(s.counter(), "1 of 10+");
        s.start_complete(&d);
        assert_eq!(s.counter(), "1 of 10+ …", "still counting");
    }

    #[test]
    fn anchoring_puts_next_on_the_match_after_the_cursor() {
        let mut s = SearchState::new();
        s.matches.ranges = vec![0..3, 8..11, 16..19];
        s.matches.current = Some(0);

        s.anchor_for(8, true);
        assert_eq!(s.next_match(), Some(8..11), "the cursor is on this match");

        s.anchor_for(0, true);
        assert_eq!(s.next_match(), Some(0..3), "before the first match");

        s.anchor_for(99, true);
        assert_eq!(s.next_match(), Some(0..3), "past the last one, wraps");

        s.anchor_for(8, false);
        assert_eq!(s.prev_match(), Some(0..3), "backwards, the match before");
    }

    #[test]
    fn a_committed_scan_runs_once_per_query() {
        let d = doc(&"a ".repeat(100));
        let mut s = SearchState::new();
        s.query = Query::literal("a");
        s.active = true;
        s.rescan_with(&d, 10);

        let ticket = s.start_complete(&d).expect("truncated");
        assert!(s.start_complete(&d).is_none(), "one scan at a time");

        // Pretend even the full budget ran out: the list is still truncated, so
        // without a separate flag every later `next` would scan the file again.
        let full = find_all(d.storage(), &s.query, FULL_MATCHES).unwrap();
        assert!(s.install(full, ticket, &d));
        assert!(
            s.start_complete(&d).is_none(),
            "the same query is not rescanned after its committed scan"
        );

        // A new query earns a new scan.
        s.query = Query::literal("a ");
        s.rescan_with(&d, 10);
        assert!(s.start_complete(&d).is_some());
    }

    #[test]
    fn dollar_escape_in_replacement() {
        let pattern = Query::literal("x");
        assert_eq!(
            expand_replacement("$$5", "", &(0..0), Some(&pattern)),
            "$$5"
        );
        assert_eq!(
            expand_replacement("cost: $$5", "a", &(0..1), None),
            "cost: $$5"
        );
    }

    #[test]
    fn a_case_insensitive_replacement_still_expands_captures() {
        // The expansion used to rebuild the pattern with `multi_line` only, so a
        // case-insensitive search re-ran a case-sensitive pattern against the
        // matched slice, found no captures, and inserted `$1` verbatim.
        let d = doc("HELLO");
        let mut q = Query::literal("(hello)");
        q.regex = true;
        q.case_sensitive = false;
        let ranges = found(&d, &q, 100);
        assert_eq!(ranges, vec![0..5], "the search itself is case-insensitive");

        let text = d.text();
        assert_eq!(
            expand_replacement("<$1>", &text, &ranges[0], Some(&q)),
            "<HELLO>",
            "and the backreference resolves to what the search matched"
        );
    }
}
