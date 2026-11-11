//! External-change detection: a file that changes under the editor.
//!
//! The decisions are driven through `App::check_files_on_disk`, which runs the
//! same reconciliation the watcher runs and does not depend on how quickly a
//! platform delivers an event. Two tests at the end drive the real watcher, so
//! the wiring is covered as well as the reasoning.

mod common;

use std::time::{Duration, Instant};

use common::*;

#[test]
fn a_clean_buffer_follows_a_change_made_by_another_program() {
    let scratch = Scratch::new();
    let path = scratch.file("notes.txt", "one\ntwo\n");
    let mut app = editor(scratch.path());
    open(&mut app, &path);
    assert_eq!(text(&app), "one\ntwo\n");

    std::fs::write(&path, "one\nTWO\nthree\n").expect("rewrite the file");
    app.check_files_on_disk();

    assert_eq!(text(&app), "one\nTWO\nthree\n", "the buffer followed");
    assert!(!modified(&app), "and it is not a change of the user's");
    assert!(
        !app.active_doc().unwrap().externally_modified(),
        "nothing is pending, so nothing is flagged"
    );
}

#[test]
fn a_modified_buffer_is_flagged_and_never_overwritten() {
    let scratch = Scratch::new();
    let path = scratch.file("notes.txt", "one\n");
    let mut app = editor(scratch.path());
    open(&mut app, &path);
    type_str(&mut app, "mine ");
    assert_eq!(text(&app), "mine one\n");

    std::fs::write(&path, "theirs\n").expect("rewrite the file");
    app.check_files_on_disk();

    assert_eq!(
        text(&app),
        "mine one\n",
        "the edits are the only copy of themselves"
    );
    assert!(modified(&app));
    assert!(
        app.active_doc().unwrap().externally_modified(),
        "so the buffer is flagged instead"
    );
    assert!(
        screen_has(&mut app, 100, 20, "changed on disk"),
        "and the status bar says why"
    );
}

#[test]
fn a_deleted_file_is_flagged_and_the_text_kept() {
    let scratch = Scratch::new();
    let path = scratch.file("gone.txt", "keep me\n");
    let mut app = editor(scratch.path());
    open(&mut app, &path);

    std::fs::remove_file(&path).expect("delete the file");
    app.check_files_on_disk();

    assert_eq!(text(&app), "keep me\n", "the text survives the file");
    assert!(app.active_doc().unwrap().externally_modified());
    assert!(
        screen_has(&mut app, 100, 20, "changed on disk"),
        "the status bar flags it"
    );
}

#[test]
fn saving_is_not_an_external_change() {
    let scratch = Scratch::new();
    let path = scratch.file("notes.txt", "one\n");
    let mut app = editor(scratch.path());
    open(&mut app, &path);
    type_str(&mut app, "two\n");

    ctrl(&mut app, 's');
    assert!(!modified(&app), "the save landed");

    app.check_files_on_disk();

    assert!(!app.active_doc().unwrap().externally_modified());
    assert_eq!(text(&app), "two\none\n", "and it did not reload itself");
}

#[test]
fn a_reload_keeps_the_caret_on_its_line() {
    let scratch = Scratch::new();
    let path = scratch.file("notes.txt", "alpha\nbravo\ncharlie\n");
    let mut app = editor(scratch.path());
    open(&mut app, &path);
    app.active_doc_mut().unwrap().goto_line_col(1, 2);
    assert_eq!(cursor_1(&app), (2, 3));

    // The caret holds its line and column, not its offset: an edit above it
    // would otherwise leave the caret part-way through a different word.
    std::fs::write(&path, "new\nalpha\nbravo\ncharlie\n").expect("rewrite the file");
    app.check_files_on_disk();

    assert_eq!(text(&app), "new\nalpha\nbravo\ncharlie\n");
    assert_eq!(cursor_1(&app), (2, 3), "the caret did not jump");
}

#[test]
fn a_reload_leaves_the_scroll_position_alone() {
    let scratch = Scratch::new();
    let body: String = (0..200).map(|i| format!("line {i}\n")).collect();
    let path = scratch.file("long.txt", &body);
    let mut app = editor(scratch.path());
    open(&mut app, &path);
    app.active_doc_mut().unwrap().goto_line_col(150, 0);
    screen(&mut app, 80, 20);
    let before = app.active_doc().unwrap().scroll_top();

    std::fs::write(&path, body.replace("line 0", "LINE 0")).expect("rewrite the file");
    app.check_files_on_disk();
    screen(&mut app, 80, 20);

    assert_eq!(
        app.active_doc().unwrap().scroll_top(),
        before,
        "a reload is not a jump"
    );
}

#[test]
fn the_watcher_reloads_a_clean_buffer_without_being_asked() {
    let scratch = Scratch::new();
    let path = scratch.file("notes.txt", "one\n");
    let mut app = editor(scratch.path());
    app.enable_disk_watch();
    open(&mut app, &path);

    // Give the platform watcher a moment to attach before the write it has to
    // see; the events themselves are queued from then on.
    std::thread::sleep(Duration::from_millis(100));
    std::fs::write(&path, "two\n").expect("rewrite the file");

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        app.poll_jobs();
        if text(&app) == "two\n" {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(text(&app), "two\n", "the watcher reloaded the buffer");
}

#[test]
fn save_as_moves_the_watch_to_the_new_file() {
    let scratch = Scratch::new();
    let first = scratch.file("first.txt", "one\n");
    let second = scratch.path().join("second.txt");
    let mut app = editor(scratch.path());
    app.enable_disk_watch();
    open(&mut app, &first);
    std::thread::sleep(Duration::from_millis(100));

    save_as(&mut app, &second);
    assert_eq!(text(&app), "one\n");
    std::thread::sleep(Duration::from_millis(100));

    // The old name is no longer this buffer's file.
    std::fs::write(&first, "stale\n").expect("rewrite the old file");
    // The new one is.
    std::fs::write(&second, "two\n").expect("rewrite the new file");

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        app.poll_jobs();
        if text(&app) == "two\n" {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(text(&app), "two\n", "the watch followed the buffer");
}
