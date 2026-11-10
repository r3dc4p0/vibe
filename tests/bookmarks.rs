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
        Some("bookmark 1")
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
fn bookmark_gutter_highlight_can_be_bound_to_panel_visibility() {
    let scratch = Scratch::new();
    let mut app = editor_with(scratch.path(), |config| {
        config.editor.bookmark_gutter_highlight_requires_panel = true;
    });
    type_str(&mut app, "first");
    screen(&mut app, 80, 16);
    let area = app.editor_area();
    let gutter =
        vibe::ui::editor::gutter_width(app.active_doc().unwrap().line_count(), &app.config);
    double_click(&mut app, area.x + gutter.saturating_sub(1), area.y);

    let render_styles = |app: &mut vibe::app::App| {
        let mut terminal = Terminal::new(TestBackend::new(80, 16)).unwrap();
        terminal.draw(|frame| vibe::ui::draw(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();
        // The marker moves down a row when the caption is shown, so look for it
        // anywhere in the gutter rather than at a fixed row.
        let marked = (area.y..area.y + area.height).any(|y| {
            (0..gutter)
                .any(|c| buffer[(area.x + c, y)].symbol() == vibe::ui::editor::BOOKMARK_MARKER)
        });
        let first_row: String = (0..80u16)
            .map(|x| buffer[(x, area.y)].symbol().to_string())
            .collect();
        (marked, first_row)
    };
    assert!(app.ui.bookmarks.open, "creation opens the panel");
    let (marked, row) = render_styles(&mut app);
    assert!(marked, "the marker is drawn");
    assert!(
        row.contains("bookmark 1"),
        "and the caption with it:\n{row}"
    );

    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    let (marked, row) = render_styles(&mut app);
    assert!(!marked, "closing the panel hides the marker");
    assert!(row.contains("first"), "and the caption:\n{row}");
    click(&mut app, area.x + gutter + 2, area.y);
    assert_eq!(cursor_1(&app), (1, 3));

    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    let (marked, row) = render_styles(&mut app);
    assert!(marked, "the marker comes back with the panel");
    assert!(
        row.contains("bookmark 1"),
        "and so does the caption:\n{row}"
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
    assert_eq!(doc.bookmark_caption(0), Some("bookmark 1"));
    assert_eq!(doc.bookmark_caption(2), Some("bookmark 2"));
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
