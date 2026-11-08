//! Search in a document with more matches than the live scan collects.
//!
//! The live scan is capped so that typing in the find bar stays fast even in a
//! multi-gigabyte file. That cap used to be the *only* scan, which made every
//! match past it invisible: the counter read "20000+" and `next` wrapped back to
//! the first match, so the tail of the file could not be reached at all. These
//! tests pin the contract that replaces it — the cap is a latency budget for
//! typing, not a limit on what can be found.

mod common;

use std::time::{Duration, Instant};

use common::*;
use crossterm::event::{KeyCode, KeyModifiers};
use vibe::app::App;
use vibe::search::LIVE_MATCHES;

/// One match per line, and enough of them to overflow the live scan.
const TOTAL: usize = LIVE_MATCHES + 100;
/// `"let x\n"` — the match starts at `LINE * i`.
const LINE: usize = 6;

fn fixture() -> (Scratch, std::path::PathBuf) {
    let scratch = Scratch::new();
    let path = scratch.file("big.txt", &"let x\n".repeat(TOTAL));
    (scratch, path)
}

/// Open the find bar and search for `needle`, stopping while the live scan is
/// all there is.
fn searching(app: &mut App, needle: &str) {
    ctrl(app, 'f');
    ctrl(app, 'u'); // clear the word Ctrl+F pre-filled
    type_str(app, needle);
}

/// Pump background jobs until the committed scan has landed.
fn wait_for_search(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while app.search.searching() {
        app.poll_jobs();
        assert!(
            Instant::now() < deadline,
            "the committed scan never finished"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn typing_is_capped_but_committing_finds_everything() {
    let (scratch, path) = fixture();
    let mut app = editor(scratch.path());
    open(&mut app, &path);
    ctrl(&mut app, KeyCode::Home);

    searching(&mut app, "let");
    assert_eq!(
        app.search.matches.len(),
        LIVE_MATCHES,
        "the live scan stops at its latency budget"
    );
    assert!(
        app.search.matches.truncated,
        "and knows that it stopped early"
    );

    plain(&mut app, KeyCode::Enter);
    wait_for_search(&mut app);

    assert_eq!(
        app.search.matches.len(),
        TOTAL,
        "committing the search collects the matches past the cap"
    );
    assert!(!app.search.matches.truncated, "and the count is final");
}

#[test]
fn the_last_match_is_reachable_and_next_wraps_from_it() {
    let (scratch, path) = fixture();
    let mut app = editor(scratch.path());
    open(&mut app, &path);
    ctrl(&mut app, KeyCode::Home);

    searching(&mut app, "let");
    plain(&mut app, KeyCode::Enter);
    wait_for_search(&mut app);
    plain(&mut app, KeyCode::Esc);

    // Stand on the final match — the one the old cap made unreachable.
    let last = (TOTAL - 1) * LINE;
    app.active_doc_mut().unwrap().set_cursor(last);

    plain(&mut app, KeyCode::F(3));
    assert_eq!(cursor(&app), last + 3, "F3 reaches the very last match");

    plain(&mut app, KeyCode::F(3));
    assert_eq!(
        cursor(&app),
        3,
        "and only then wraps around to the first match"
    );
}

#[test]
fn previous_from_the_first_match_reaches_the_true_last_one() {
    let (scratch, path) = fixture();
    let mut app = editor(scratch.path());
    open(&mut app, &path);
    ctrl(&mut app, KeyCode::Home);

    searching(&mut app, "let");
    plain(&mut app, KeyCode::Enter);
    wait_for_search(&mut app);
    plain(&mut app, KeyCode::Esc);

    app.active_doc_mut().unwrap().set_cursor(0);
    shift(&mut app, KeyCode::F(3));
    assert_eq!(
        cursor(&app),
        (TOTAL - 1) * LINE + 3,
        "backwards wraps to the last match in the file, not to the end of the live scan"
    );
}

#[test]
fn next_starts_from_the_cursor_not_from_the_last_match() {
    let scratch = Scratch::new();
    let path = scratch.file("hay.txt", "one two one two one\n");
    let mut app = editor(scratch.path());
    open(&mut app, &path);
    ctrl(&mut app, KeyCode::Home);

    searching(&mut app, "one");
    plain(&mut app, KeyCode::Esc); // close the bar, keep the query

    // Clicking away and pressing F3 continues from where the cursor is.
    app.active_doc_mut().unwrap().set_cursor(8);
    plain(&mut app, KeyCode::F(3));
    assert_eq!(
        cursor(&app),
        11,
        "the match at the cursor, then the next one"
    );

    app.active_doc_mut().unwrap().set_cursor(0);
    plain(&mut app, KeyCode::F(3));
    assert_eq!(cursor(&app), 3, "from before the first match, that match");
}

#[test]
fn replace_all_does_not_stop_at_the_live_cap() {
    let (scratch, path) = fixture();
    let mut app = editor(scratch.path());
    open(&mut app, &path);
    ctrl(&mut app, KeyCode::Home);

    ctrl(&mut app, 'h');
    ctrl(&mut app, 'u');
    type_str(&mut app, "let");
    plain(&mut app, KeyCode::Tab); // move to the replacement field
    type_str(&mut app, "var");
    press(&mut app, KeyCode::Enter, KeyModifiers::ALT);

    assert_eq!(
        app.last_message().unwrap(),
        format!("Replaced {TOTAL} occurrence(s)"),
        "every match is replaced, not just the ones the live scan saw"
    );
    assert_eq!(
        text(&app),
        "var x\n".repeat(TOTAL),
        "and the tail of the file is actually rewritten"
    );
}

/// A file whose `foo` occurrences are almost all sub-word hits, followed by one
/// bare match at the very end.
///
/// `"foobar "` is seven characters, so the bare `foo` starts at `SUB_WORD * 7`.
fn whole_word_fixture(sub_word: usize) -> (Scratch, std::path::PathBuf, usize) {
    let scratch = Scratch::new();
    let body = format!("{}foo", "foobar ".repeat(sub_word));
    let path = scratch.file("words.txt", &body);
    (scratch, path, sub_word * 7)
}

#[test]
fn whole_word_finds_a_bare_match_past_a_run_of_sub_word_hits() {
    // More sub-word hits than the live scan collects. Taking the budget before
    // the whole-word filter meant the first `LIVE_MATCHES` raw hits were all
    // `foobar`, every one of them was filtered out, and the bar said "no
    // matches" while a bare `foo` sat at the end of the file.
    let (scratch, path, bare) = whole_word_fixture(LIVE_MATCHES + 5_000);
    let mut app = editor(scratch.path());
    open(&mut app, &path);
    ctrl(&mut app, KeyCode::Home);

    ctrl(&mut app, 'f');
    ctrl(&mut app, 'u');
    type_str(&mut app, "foo");
    press(&mut app, KeyCode::Char('w'), KeyModifiers::ALT); // whole word

    assert_eq!(
        app.search.matches.len(),
        1,
        "the only whole-word match is the bare `foo`, not the run of `foobar`"
    );
    assert_eq!(app.search.ranges().len(), 1, "only the bare match counts");
    assert_eq!(app.search.ranges()[0], bare..bare + 3);
    assert!(
        !app.search.matches.truncated,
        "the raw scan reached the end of the file, so the answer is final"
    );
}

#[test]
fn a_whole_word_scan_that_ran_out_reports_more_and_finds_it_on_commit() {
    // Enough sub-word hits that even the overshooting raw scan is cut short, so
    // the bare match is genuinely out of reach for the live scan. The list is
    // empty, but it must not claim to be the answer — the counter has to keep
    // advertising a scan so committing one can find the match.
    let (scratch, path, bare) = whole_word_fixture(LIVE_MATCHES * 5);
    let mut app = editor(scratch.path());
    open(&mut app, &path);
    ctrl(&mut app, KeyCode::Home);

    ctrl(&mut app, 'f');
    ctrl(&mut app, 'u');
    type_str(&mut app, "foo");
    press(&mut app, KeyCode::Char('w'), KeyModifiers::ALT);

    assert!(
        app.search.matches.is_empty(),
        "the live scan cannot see past the sub-word hits"
    );
    assert!(
        app.search.matches.truncated,
        "but it knows it stopped early"
    );

    plain(&mut app, KeyCode::Enter);
    wait_for_search(&mut app);

    assert_eq!(
        app.search.ranges().len(),
        1,
        "committing the search finds the bare match"
    );
    assert_eq!(app.search.ranges()[0], bare..bare + 3);
    assert!(!app.search.matches.truncated, "and the count is final");
}
