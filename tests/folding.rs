mod common;

use common::*;
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

/// A function with a two-line body and a line after it, so a fold has something
/// to hide and something to leave alone.
const BLOCK: &str = "fn main() {\n    one();\n    two();\n}\nafter\n";

fn fold_editor(scratch: &Scratch) -> vibe::app::App {
    // Pairs and auto-indent are off so the fixture lands on disk as written:
    // a fold is about indentation, and a fixture that quietly indents itself
    // would be testing the wrong document.
    editor_with(scratch.path(), |config| {
        config.editor.auto_pairs = false;
        config.editor.auto_indent = false;
    })
}

fn fold(app: &mut vibe::app::App) {
    alt(app, KeyCode::Char('f'));
}

fn click(app: &mut vibe::app::App, column: u16, row: u16) {
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
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
fn alt_f_folds_the_block_under_the_cursor() {
    let scratch = Scratch::new();
    let mut app = fold_editor(&scratch);
    type_str(&mut app, BLOCK);
    app.active_doc_mut().unwrap().goto_line(0);

    fold(&mut app);
    let doc = app.active_doc().unwrap();
    assert_eq!(
        doc.folds().hidden_at(0),
        Some(2),
        "the body is what folds, not the closing brace"
    );
    assert!(doc.is_line_hidden(1) && doc.is_line_hidden(2));
    assert!(!doc.is_line_hidden(3), "the brace still says where it ends");

    let rows = screen(&mut app, 60, 12);
    let all = rows.join("\n");
    assert!(all.contains("fn main() {"), "{all}");
    assert!(!all.contains("one();"), "the body is gone:\n{all}");
    assert!(all.contains('}'), "the brace is not:\n{all}");
    assert!(
        all.contains("after"),
        "and nothing past the block moved:\n{all}"
    );
    assert!(all.contains("⋯ 2"), "the line says how much it hid:\n{all}");
    assert!(all.contains('▸'), "and the gutter marks it:\n{all}");
}

#[test]
fn folding_twice_opens_it_again() {
    let scratch = Scratch::new();
    let mut app = fold_editor(&scratch);
    type_str(&mut app, BLOCK);
    app.active_doc_mut().unwrap().goto_line(0);

    fold(&mut app);
    fold(&mut app);
    assert!(app.active_doc().unwrap().folds().is_empty());
    let all = screen(&mut app, 60, 12).join("\n");
    assert!(all.contains("one();"), "the body came back:\n{all}");
    assert!(!all.contains('▸'), "and the marker went with it:\n{all}");
}

#[test]
fn the_arrow_keys_step_over_a_fold_instead_of_into_it() {
    let scratch = Scratch::new();
    let mut app = fold_editor(&scratch);
    type_str(&mut app, BLOCK);
    app.active_doc_mut().unwrap().goto_line(0);
    fold(&mut app);

    plain(&mut app, KeyCode::Down);
    assert_eq!(
        cursor_1(&app).0,
        4,
        "down from the header lands after the fold, not inside it"
    );
    plain(&mut app, KeyCode::Up);
    assert_eq!(cursor_1(&app).0, 1, "and up lands back on the header");
}

#[test]
fn a_click_on_the_fold_column_toggles_the_block() {
    let scratch = Scratch::new();
    let mut app = fold_editor(&scratch);
    type_str(&mut app, BLOCK);
    screen(&mut app, 80, 16);

    let area = app.editor_area();
    let column = vibe::ui::editor::fold_column(app.active_doc().unwrap().line_count(), &app.config)
        .expect("a gutter to draw the marker in");

    click(&mut app, area.x + column, area.y);
    assert_eq!(app.active_doc().unwrap().folds().hidden_at(0), Some(2));
    click(&mut app, area.x + column, area.y);
    assert!(app.active_doc().unwrap().folds().is_empty());
}

#[test]
fn unfold_all_reopens_every_block() {
    let scratch = Scratch::new();
    let mut app = fold_editor(&scratch);
    type_str(&mut app, "outer\n    inner\n        deep\n    back\nlast");
    app.active_doc_mut().unwrap().goto_line(0);
    fold(&mut app);
    app.active_doc_mut().unwrap().goto_line(1);
    fold(&mut app);
    assert_eq!(
        app.active_doc().unwrap().folds().len(),
        1,
        "merged, not nested"
    );

    press(
        &mut app,
        KeyCode::Char('e'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert!(app.active_doc().unwrap().folds().is_empty());
    assert!(screen(&mut app, 60, 12).join("\n").contains("deep"));
}

#[test]
fn folding_is_neither_an_edit_nor_an_undo_step() {
    let scratch = Scratch::new();
    let mut app = fold_editor(&scratch);
    type_str(&mut app, BLOCK);
    app.active_doc_mut().unwrap().goto_line(0);
    let dirty_before = modified(&app);

    fold(&mut app);
    assert_eq!(
        modified(&app),
        dirty_before,
        "folding is not a change to the file"
    );
    assert_eq!(text(&app), BLOCK, "and it moves no text");

    // Undo takes back the last thing typed, not the last thing looked at.
    press(&mut app, KeyCode::Char('z'), KeyModifiers::CONTROL);
    assert_eq!(
        app.active_doc().unwrap().folds().hidden_at(0),
        Some(2),
        "the fold is still there"
    );
}

#[test]
fn a_line_inserted_above_a_fold_carries_it_down() {
    let scratch = Scratch::new();
    let mut app = fold_editor(&scratch);
    type_str(&mut app, BLOCK);
    app.active_doc_mut().unwrap().goto_line(0);
    fold(&mut app);

    // Enter at the very start of the header pushes the header down a line.
    app.active_doc_mut().unwrap().goto_line_col(0, 0);
    plain(&mut app, KeyCode::Enter);

    let doc = app.active_doc().unwrap();
    assert_eq!(
        doc.folds().hidden_at(1),
        Some(3),
        "the fold followed the function it describes"
    );
    assert!(!doc.is_line_hidden(1), "and the header is still on screen");
}

#[test]
fn a_break_typed_at_the_end_of_the_header_grows_the_fold() {
    let scratch = Scratch::new();
    let mut app = fold_editor(&scratch);
    type_str(&mut app, BLOCK);
    app.active_doc_mut().unwrap().goto_line(0);
    fold(&mut app);

    // The caret goes to the end of the header, which is a line that is still
    // there; the break it inserts belongs inside the block.
    app.active_doc_mut().unwrap().goto_line_col(0, 11);
    plain(&mut app, KeyCode::Enter);
    assert_eq!(
        app.active_doc().unwrap().folds().hidden_at(0),
        Some(3),
        "the new line is inside the fold"
    );
}

#[test]
fn folding_survives_word_wrap() {
    let scratch = Scratch::new();
    let mut app = editor_with(scratch.path(), |config| {
        config.editor.auto_pairs = false;
        config.editor.auto_indent = false;
        config.editor.word_wrap = true;
    });
    type_str(&mut app, BLOCK);
    app.active_doc_mut().unwrap().goto_line(0);
    fold(&mut app);

    let all = screen(&mut app, 40, 12).join("\n");
    assert!(all.contains("fn main() {"), "{all}");
    assert!(!all.contains("one();"), "hidden lines stay hidden:\n{all}");
    assert!(all.contains("after"), "{all}");
}

#[test]
fn a_jump_into_a_fold_opens_it() {
    let scratch = Scratch::new();
    let mut app = fold_editor(&scratch);
    type_str(&mut app, BLOCK);
    app.active_doc_mut().unwrap().goto_line(0);
    fold(&mut app);
    assert!(app.active_doc().unwrap().is_line_hidden(1));

    // Go to line 2, which the fold is hiding: asking to go there is asking to
    // see it.
    app.active_doc_mut().unwrap().goto_line(1);
    assert!(
        app.active_doc().unwrap().folds().is_empty(),
        "the fold opened rather than swallowing the caret"
    );
    assert_eq!(cursor_1(&app).0, 2);
}

#[test]
fn the_last_screen_of_a_folded_file_is_still_reachable() {
    let scratch = Scratch::new();
    let mut app = fold_editor(&scratch);
    let body: String = (0..40).map(|n| format!("line-{n}\n")).collect();
    type_str(&mut app, &format!("head\n{}", body));
    app.active_doc_mut().unwrap().goto_line(0);
    fold(&mut app);
    screen(&mut app, 60, 12);

    let area = app.editor_area();
    for _ in 0..40 {
        wheel_down(&mut app, area.x + area.width / 2, area.y + 1);
    }
    let all = screen(&mut app, 60, 12).join("\n");
    assert!(
        all.contains("line-39"),
        "the wheel reaches the end even with a fold in the way:\n{all}"
    );
}
