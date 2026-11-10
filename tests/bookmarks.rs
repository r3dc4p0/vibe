mod common;

use common::*;
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn click(app: &mut vibe::app::App, column: u16, row: u16) {
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

fn double_click(app: &mut vibe::app::App, column: u16, row: u16) {
    click(app, column, row);
    click(app, column, row);
}

fn wheel_down(app: &mut vibe::app::App, column: u16, row: u16) {
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

#[test]
fn gutter_click_toggles_and_draws_the_marker() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "first\nsecond");
    screen(&mut app, 80, 16);
    let area = app.editor_area();
    let gutter =
        vibe::ui::editor::gutter_width(app.active_doc().unwrap().line_count(), &app.config);

    click(&mut app, area.x + gutter.saturating_sub(1), area.y + 1);
    assert!(!app.active_doc().unwrap().bookmarks().contains_key(&1));
    click(&mut app, area.x + gutter.saturating_sub(1), area.y + 1);
    assert!(app.active_doc().unwrap().bookmarks().contains_key(&1));
    assert_eq!(
        app.active_doc().unwrap().bookmark_caption(1),
        Some("bookmark_1")
    );
    assert!(app.ui.bookmarks.open, "creating a bookmark opens its panel");

    let theme = vibe::ui::theme_of(&app).clone();
    let mut terminal = Terminal::new(TestBackend::new(80, 16)).unwrap();
    terminal
        .draw(|frame| vibe::ui::draw(frame, &mut app))
        .unwrap();
    let buffer = terminal.backend().buffer();

    // The mark is a glyph in the gutter's first column, ahead of the numbers,
    // not a repaint of the gutter: the line number keeps its own background and
    // stays readable. Line 1 sits one row lower than its line number suggests,
    // because its caption occupies a row of its own above it.
    let rule_x = area.x + gutter - 2;
    let marker = &buffer[(area.x, area.y + 2)];
    assert_eq!(marker.symbol(), vibe::ui::editor::BOOKMARK_MARKER);
    assert_eq!(marker.fg, theme.mark);
    let number = (0..gutter)
        .map(|c| &buffer[(area.x + c, area.y + 2)])
        .find(|cell| cell.symbol() == "2")
        .expect("the number is still drawn");
    assert_eq!(
        number.bg, theme.gutter_bg,
        "and it is no longer painted as a filled block"
    );

    // The caption hangs off the gutter: a corner at the far edge and a rule
    // running out to the hairline, so the row reads as a bracket rather than as
    // another line of the document.
    assert_eq!(buffer[(area.x, area.y + 1)].symbol(), "┌");
    assert_eq!(
        buffer[(area.x, area.y + 1)].fg,
        theme.mark,
        "the bracket is drawn in the same accent as the caption"
    );
    assert_eq!(buffer[(rule_x - 1, area.y + 1)].symbol(), "─");
    assert_eq!(
        buffer[(rule_x, area.y + 1)].symbol(),
        "▏",
        "the hairline closes the bracket"
    );

    // The unmarked line above has an empty marker column.
    assert_eq!(buffer[(area.x, area.y)].symbol(), " ");

    double_click(&mut app, area.x + gutter.saturating_sub(1), area.y + 2);
    assert!(!app.active_doc().unwrap().bookmarks().contains_key(&1));
}

#[test]
fn closing_the_panel_folds_the_caption_into_a_rectangle() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "first");
    screen(&mut app, 80, 16);
    let area = app.editor_area();
    let gutter =
        vibe::ui::editor::gutter_width(app.active_doc().unwrap().line_count(), &app.config);
    double_click(&mut app, area.x + gutter.saturating_sub(1), area.y);

    let render = |app: &mut vibe::app::App| {
        let mut terminal = Terminal::new(TestBackend::new(80, 16)).unwrap();
        terminal.draw(|frame| vibe::ui::draw(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();
        // The mark sits on whichever row its line landed on, which depends on
        // whether the caption above it is being drawn. The caption's own
        // corner is in the same column, so only the two bookmark glyphs are
        // collected.
        let marks: Vec<(u16, String)> = (area.y..area.y + area.height)
            .filter_map(|y| {
                let symbol = buffer[(area.x, y)].symbol().to_string();
                matches!(symbol.as_str(), "└" | "▌").then_some((y, symbol))
            })
            .collect();
        let rows: Vec<String> = (0..80u16)
            .map(|x| buffer[(x, area.y)].symbol().to_string())
            .collect();
        (marks, rows.concat())
    };

    assert!(app.ui.bookmarks.open, "creation opens the panel");
    let (marks, row) = render(&mut app);
    assert_eq!(
        marks,
        vec![(area.y + 1, "└".to_string())],
        "the mark is a corner, one row below its caption"
    );
    assert!(
        row.contains("bookmark_1"),
        "and the caption is on that row:\n{row}"
    );

    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    let (marks, row) = render(&mut app);
    assert_eq!(
        marks,
        vec![(area.y, "▌".to_string())],
        "with the panel closed the corner folds into a rectangle"
    );
    assert!(
        !row.contains("bookmark_1"),
        "and the caption gives its row back to the text:\n{row}"
    );
    assert!(row.contains("first"), "which is now on the top row:\n{row}");

    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    let (marks, row) = render(&mut app);
    assert_eq!(marks, vec![(area.y + 1, "└".to_string())], "and back again");
    assert!(row.contains("bookmark_1"), "with its caption:\n{row}");
}

#[test]
fn creating_a_bookmark_shows_a_hidden_panel() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "first\nsecond");
    screen(&mut app, 80, 16);
    let area = app.editor_area();
    let gutter =
        vibe::ui::editor::gutter_width(app.active_doc().unwrap().line_count(), &app.config);

    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert!(!app.ui.bookmarks.open, "the panel starts this test closed");

    double_click(&mut app, area.x + gutter.saturating_sub(1), area.y + 1);
    assert!(app.active_doc().unwrap().bookmarks().contains_key(&1));
    assert!(
        app.ui.bookmarks.open,
        "a new bookmark brings its panel back into view"
    );
}

#[test]
fn emptying_a_caption_asks_before_removing_the_bookmark() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "first");
    app.active_doc_mut().unwrap().toggle_bookmark(0);
    app.active_doc_mut()
        .unwrap()
        .set_bookmark_caption(0, String::new());
    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );

    plain(&mut app, KeyCode::Backspace);
    assert!(app.ui.bookmarks.pending_delete, "the panel asks first");
    assert!(
        app.active_doc().unwrap().bookmarks().contains_key(&0),
        "and the bookmark is still there to say no to"
    );
    let rows = screen(&mut app, 80, 16);
    assert!(
        rows.iter().any(|row| row.contains("delete?")),
        "the question is on the row: {rows:?}"
    );

    plain(&mut app, KeyCode::Esc);
    assert!(!app.ui.bookmarks.pending_delete, "Esc is a no");
    assert!(app.active_doc().unwrap().bookmarks().contains_key(&0));

    plain(&mut app, KeyCode::Backspace);
    assert!(app.ui.bookmarks.pending_delete);
    plain(&mut app, KeyCode::Enter);
    assert!(
        !app.active_doc().unwrap().bookmarks().contains_key(&0),
        "Enter is the yes"
    );
    assert!(!app.ui.bookmarks.pending_delete);
}

#[test]
fn a_bookmark_left_unnamed_is_given_a_name() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "first\nsecond");
    let doc = app.active_doc_mut().unwrap();
    doc.toggle_bookmark(0);
    doc.set_bookmark_caption(0, "kept".into());
    doc.toggle_bookmark(1);
    doc.set_bookmark_caption(1, String::new());
    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );

    assert_eq!(
        app.active_doc().unwrap().bookmark_caption(1),
        Some(""),
        "still blank while the caret is in it"
    );
    plain(&mut app, KeyCode::Esc);
    assert_eq!(
        app.active_doc().unwrap().bookmark_caption(1),
        Some("bookmark_2"),
        "and named with the first number that is free when it is left"
    );
    assert_eq!(
        app.active_doc().unwrap().bookmark_caption(0),
        Some("kept"),
        "the other caption is untouched"
    );
}

#[test]
fn new_bookmarks_receive_progressive_default_captions() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "first\nsecond\nthird");
    screen(&mut app, 80, 16);
    let area = app.editor_area();
    let gutter =
        vibe::ui::editor::gutter_width(app.active_doc().unwrap().line_count(), &app.config);

    double_click(&mut app, area.x + gutter.saturating_sub(1), area.y);
    screen(&mut app, 80, 16);
    double_click(&mut app, area.x + gutter.saturating_sub(1), area.y + 3);

    let doc = app.active_doc().unwrap();
    assert_eq!(doc.bookmark_caption(0), Some("bookmark_1"));
    assert_eq!(doc.bookmark_caption(2), Some("bookmark_2"));
}

#[test]
fn bookmark_caption_has_a_phantom_row_and_is_editable_from_panel() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "first\nsecond");
    screen(&mut app, 80, 16);
    let area = app.editor_area();
    let gutter =
        vibe::ui::editor::gutter_width(app.active_doc().unwrap().line_count(), &app.config);
    double_click(&mut app, area.x + gutter.saturating_sub(1), area.y + 1);

    assert!(app.ui.bookmarks.open);
    screen(&mut app, 80, 16);
    assert!(app.editor_area().width < area.width);
    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );
    type_str(&mut app, "Review");

    assert_eq!(
        app.active_doc().unwrap().bookmark_caption(1),
        Some("Review")
    );
    let hit = app.ui.bookmarks.hits[0];
    click(&mut app, hit.caption_x + 2, hit.area.y);
    plain(&mut app, KeyCode::Char('!'));
    assert_eq!(
        app.active_doc().unwrap().bookmark_caption(1),
        Some("Re!view")
    );
    let rows = screen(&mut app, 80, 16);
    assert_eq!(text(&app), "first\nsecond");
    assert!(rows.iter().filter(|row| row.contains("Re!view")).count() >= 2);

    plain(&mut app, KeyCode::Enter);
    assert_eq!(cursor_1(&app).0, 2);
    assert!(!app.ui.bookmarks.focused);
}

#[test]
fn cursor_stays_visible_when_bookmark_captions_add_visual_rows() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    let body = (0..12)
        .map(|line| format!("item-{line}"))
        .collect::<Vec<_>>()
        .join("\n");
    type_str(&mut app, &body);
    let doc = app.active_doc_mut().unwrap();
    for line in 0..doc.line_count() {
        doc.toggle_bookmark(line);
        doc.set_bookmark_caption(line, format!("note-{line}"));
    }
    doc.goto_line_col(11, 0);
    // The captions only take their rows while the panel that explains them is
    // open, so the phantom rows these tests are about need it shown.
    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );

    let rows = screen(&mut app, 80, 8);
    assert!(rows.iter().any(|row| row.contains("item-11")));
}

#[test]
fn scrolling_reaches_the_last_line_with_phantom_rows() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    let body = (0..20)
        .map(|line| format!("item-{line:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    type_str(&mut app, &body);
    let doc = app.active_doc_mut().unwrap();
    for line in 0..doc.line_count() {
        doc.toggle_bookmark(line);
        doc.set_bookmark_caption(line, format!("note-{line}"));
    }
    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    screen(&mut app, 80, 8);
    let area = app.editor_area();
    for _ in 0..20 {
        wheel_down(&mut app, area.x + 5, area.y + 2);
    }
    let rows = screen(&mut app, 80, 8);
    assert!(rows.iter().any(|row| row.contains("item-19")));
}

#[test]
fn text_click_maps_past_a_phantom_row_and_returns_focus_to_editor() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "first\nsecond");
    let doc = app.active_doc_mut().unwrap();
    doc.toggle_bookmark(0);
    doc.set_bookmark_caption(0, "note".into());

    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    screen(&mut app, 80, 16);
    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );
    screen(&mut app, 80, 16);
    let area = app.editor_area();
    let gutter =
        vibe::ui::editor::gutter_width(app.active_doc().unwrap().line_count(), &app.config);
    click(&mut app, area.x + gutter, area.y + 1);
    assert!(!app.ui.bookmarks.focused);
    assert_eq!(cursor_1(&app).0, 1);

    plain(&mut app, KeyCode::Char('X'));
    assert_eq!(app.active_doc().unwrap().bookmark_caption(0), Some("note"));
    assert!(text(&app).starts_with("Xfirst"));
}

#[test]
fn backspace_at_caption_start_does_not_remove_bookmark() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "first");
    app.active_doc_mut().unwrap().toggle_bookmark(0);
    app.active_doc_mut()
        .unwrap()
        .set_bookmark_caption(0, "note".into());
    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );
    plain(&mut app, KeyCode::Home);
    plain(&mut app, KeyCode::Backspace);
    assert_eq!(app.active_doc().unwrap().bookmark_caption(0), Some("note"));
}

#[test]
fn panel_groups_open_files_and_enter_navigates_to_selected_buffer() {
    let scratch = Scratch::new();
    let first = scratch.file("one.txt", "first\nsecond");
    let second = scratch.file("two.txt", "third\nfourth");
    let mut app = editor(scratch.path());

    open(&mut app, &first);
    app.active_doc_mut().unwrap().toggle_bookmark(1);
    open(&mut app, &second);
    app.active_doc_mut().unwrap().toggle_bookmark(0);

    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    let rows = screen(&mut app, 80, 16);
    assert!(rows.iter().any(|row| row.contains("one.txt")));
    assert!(rows.iter().any(|row| row.contains("two.txt")));

    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );
    plain(&mut app, KeyCode::Enter);
    assert_eq!(app.active_doc().unwrap().path(), Some(first.as_path()));
    assert_eq!(cursor_1(&app).0, 2);
}

#[test]
fn escape_returns_focus_without_closing_bookmark_panel() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );
    assert!(app.ui.bookmarks.focused);
    plain(&mut app, KeyCode::Esc);
    assert!(!app.ui.bookmarks.focused);
    assert!(app.ui.bookmarks.open);
}
