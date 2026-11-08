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
fn gutter_click_toggles_and_highlights_bookmark_rectangle() {
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

    let theme = vibe::ui::theme_of(&app).clone();
    let mut terminal = Terminal::new(TestBackend::new(80, 16)).unwrap();
    terminal
        .draw(|frame| vibe::ui::draw(frame, &mut app))
        .unwrap();
    let number = &terminal.backend().buffer()[(area.x + 2, area.y + 2)];
    let gutter_fill = &terminal.backend().buffer()[(area.x + 1, area.y + 2)];
    assert_eq!(gutter_fill.bg, theme.mark);
    assert_eq!(number.fg, theme.gutter_bg);
    assert_eq!(number.bg, theme.gutter_active_fg);
    assert_ne!(number.fg, number.bg);

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

    let render_gutter_bg = |app: &mut vibe::app::App| {
        let mut terminal = Terminal::new(TestBackend::new(80, 16)).unwrap();
        terminal.draw(|frame| vibe::ui::draw(frame, app)).unwrap();
        terminal.backend().buffer()[(area.x + 1, area.y + 1)].bg
    };
    let theme = vibe::ui::theme_of(&app).clone();
    assert_eq!(render_gutter_bg(&mut app), theme.gutter_bg);

    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert_eq!(render_gutter_bg(&mut app), theme.mark);

    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert_eq!(render_gutter_bg(&mut app), theme.gutter_bg);
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

    press(
        &mut app,
        KeyCode::Char('b'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
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
