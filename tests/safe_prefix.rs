//! The `Ctrl+K` layer: the way to a command whose own chord the terminal keeps.

mod common;

use std::time::{Duration, Instant};

use common::*;
use crossterm::event::{KeyCode, KeyModifiers};
use vibe::ui::OverlayKind;

/// A function with a two-line body, so folding has something to hide.
const BLOCK: &str = "fn main() {\n    one();\n    two();\n}\nafter\n";

fn editor_for(scratch: &Scratch) -> vibe::app::App {
    // Pairs and auto-indent off, so the fixture lands as written: a fold is
    // about indentation, and a fixture that indents itself would be testing
    // a document nobody typed.
    editor_with(scratch.path(), |config| {
        config.editor.auto_pairs = false;
        config.editor.auto_indent = false;
    })
}

/// `Ctrl+K`, then `code` with `mods`.
fn safe(app: &mut vibe::app::App, code: KeyCode, mods: KeyModifiers) {
    press(app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert!(app.safe_prefix_armed(), "Ctrl+K arms the layer");
    press(app, code, mods);
}

#[test]
fn the_layer_folds_the_block_alt_f_could_not() {
    let scratch = Scratch::new();
    let mut app = editor_for(&scratch);
    type_str(&mut app, BLOCK);
    app.active_doc_mut().unwrap().goto_line(0);

    safe(&mut app, KeyCode::Char('f'), KeyModifiers::NONE);

    let doc = app.active_doc().unwrap();
    assert_eq!(doc.folds().hidden_at(0), Some(2));
    assert!(!app.safe_prefix_armed(), "the layer is spent by its key");
}

#[test]
fn the_shift_is_what_tells_fold_from_fold_all() {
    let scratch = Scratch::new();
    let mut app = editor_for(&scratch);
    type_str(&mut app, BLOCK);
    app.active_doc_mut().unwrap().goto_line(3);

    // On the closing brace `Ctrl+K f` finds no block to open...
    safe(&mut app, KeyCode::Char('f'), KeyModifiers::NONE);
    assert!(app.active_doc().unwrap().folds().is_empty());

    // ...while `Ctrl+K F` folds the file from wherever the caret is.
    safe(&mut app, KeyCode::Char('F'), KeyModifiers::SHIFT);
    assert_eq!(app.active_doc().unwrap().folds().hidden_at(0), Some(2));
}

#[test]
fn the_layer_moves_a_line_where_alt_up_was_eaten() {
    let scratch = Scratch::new();
    let mut app = editor_for(&scratch);
    type_str(&mut app, "one\ntwo\n");
    app.active_doc_mut().unwrap().goto_line(1);

    safe(&mut app, KeyCode::Up, KeyModifiers::NONE);

    assert_eq!(text(&app), "two\none\n");
}

#[test]
fn the_layer_opens_the_palette_without_the_shift_chord() {
    let scratch = Scratch::new();
    let mut app = editor_for(&scratch);

    // `Ctrl+Shift+P` is the binding a legacy terminal merges into `Ctrl+P`,
    // which opens quick-open instead. This is the way in that cannot merge.
    safe(&mut app, KeyCode::Char('P'), KeyModifiers::SHIFT);

    assert!(matches!(app.ui.overlay.kind(), OverlayKind::Palette));
}

#[test]
fn redo_reaches_through_the_layer_where_its_shifted_chord_undoes() {
    let scratch = Scratch::new();
    let mut app = editor_for(&scratch);
    type_str(&mut app, "a");
    ctrl(&mut app, 'z');
    assert_eq!(text(&app), "", "the typing is undone");

    // `Ctrl+Shift+Z` is 0x1A on a legacy wire -- the same byte as `Ctrl+Z`,
    // which would undo a second time. `Ctrl+K Z` cannot be mistaken.
    safe(&mut app, KeyCode::Char('Z'), KeyModifiers::SHIFT);

    assert_eq!(text(&app), "a");
}

#[test]
fn replace_reaches_through_the_layer_where_ctrl_h_is_backspace() {
    let scratch = Scratch::new();
    let mut app = editor_for(&scratch);
    type_str(&mut app, "ab");

    safe(&mut app, KeyCode::Char('h'), KeyModifiers::NONE);

    assert!(
        matches!(app.ui.overlay.kind(), OverlayKind::Replace),
        "Ctrl+H is 0x08 on the wire, which is Backspace"
    );
    assert_eq!(text(&app), "ab", "nothing was deleted on the way");
}

#[test]
fn a_key_the_layer_does_not_know_does_its_own_job() {
    let scratch = Scratch::new();
    let mut app = editor_for(&scratch);
    type_str(&mut app, "ab");

    safe(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);

    assert_eq!(text(&app), "abx", "the slip costs nothing");
    assert!(app.active_doc().unwrap().folds().is_empty());
}

#[test]
fn escape_drops_the_prefix_and_the_next_key_is_text() {
    let scratch = Scratch::new();
    let mut app = editor_for(&scratch);
    type_str(&mut app, "ab");

    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(!app.safe_prefix_armed());

    plain(&mut app, KeyCode::Char('f'));
    assert_eq!(text(&app), "abf");
}

#[test]
fn the_prefix_escape_does_not_cancel_a_selection() {
    let scratch = Scratch::new();
    let mut app = editor_for(&scratch);
    type_str(&mut app, "abc");

    ctrl(&mut app, 'a');
    assert!(app.active_doc().unwrap().has_selection());

    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert!(
        app.active_doc().unwrap().has_selection(),
        "Esc answered the prefix, not the editor"
    );
}

#[test]
fn a_prefix_left_alone_expires() {
    let scratch = Scratch::new();
    let mut app = editor_for(&scratch);
    type_str(&mut app, "ab");

    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    app.expire_safe_prefix(Instant::now() + Duration::from_secs(2));
    assert!(!app.safe_prefix_armed());

    plain(&mut app, KeyCode::Char('f'));
    assert_eq!(text(&app), "abf", "a lapsed prefix swallows nothing");
}

#[test]
fn the_bar_is_the_layer_while_it_waits() {
    let scratch = Scratch::new();
    let mut app = editor_for(&scratch);

    let before = screen(&mut app, 100, 12);
    let before = before.last().unwrap().clone();
    assert!(!before.contains("Ctrl+K"), "{before:?}");

    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    let after = screen(&mut app, 100, 12);
    let bar = after.last().unwrap();
    assert!(bar.contains("Ctrl+K"), "{bar:?}");
    assert!(bar.contains("fold"), "{bar:?}");
}

#[test]
fn a_binding_of_the_users_own_beats_the_prefix() {
    let scratch = Scratch::new();
    let mut app = editor_with(scratch.path(), |config| {
        config
            .keys
            .insert("ctrl+k".to_string(), "edit.selectAll".to_string());
    });
    type_str(&mut app, "abc");

    ctrl(&mut app, 'k');

    assert!(!app.safe_prefix_armed(), "the user's binding wins");
    assert!(app.active_doc().unwrap().has_selection());
}
