//! The repeat slot: the bar remembers what just ran, and `F4` runs it again.
//!
//! The point of the slot is the command you cannot easily ask for twice — one
//! picked out of the palette, or one with no binding at all. So these tests
//! drive both doors into the same record: the palette and a shortcut.

mod common;

use common::*;
use crossterm::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use vibe::app::App;
use vibe::commands::Command;
use vibe::ui::{Overlay, PaletteMode};

fn f4(app: &mut App) {
    press(app, KeyCode::F(4), KeyModifiers::NONE);
}

fn click(app: &mut App, column: u16, row: u16) {
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

/// The bar is the bottom row.
fn bar(app: &mut App, width: u16, height: u16) -> String {
    let rows = screen(app, width, height);
    rows[height as usize - 1].clone()
}

#[test]
fn the_bar_names_the_command_that_just_ran() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "one");

    app.execute(Command::DuplicateLine);

    let bar = bar(&mut app, 120, 20);
    assert!(
        bar.contains("F4 repeat Duplicate Line"),
        "the slot is at the right of the bar: {bar:?}"
    );
}

#[test]
fn f4_runs_it_again() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "one");

    app.execute(Command::DuplicateLine);
    assert_eq!(text(&app), "one\none");

    f4(&mut app);
    assert_eq!(text(&app), "one\none\none", "the second press repeated it");
    // Repeating a command keeps it the last one, so a third press still works.
    f4(&mut app);
    assert_eq!(text(&app), "one\none\none\none");
}

#[test]
fn a_command_chosen_from_the_palette_is_what_f4_repeats() {
    // The case the slot exists for: the command has no shortcut, so without
    // this the only way to run it twice is to open the palette twice.
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "one");

    app.open_palette(PaletteMode::Commands);
    type_str(&mut app, "duplicate");
    plain(&mut app, KeyCode::Enter);

    assert_eq!(app.last_command(), Some(Command::DuplicateLine));
    assert_eq!(text(&app), "one\none", "the palette ran it");

    f4(&mut app);
    assert_eq!(text(&app), "one\none\none", "and F4 ran it again");
}

#[test]
fn opening_the_palette_does_not_become_the_thing_to_repeat() {
    // Otherwise the slot would name the menu instead of the command, and the
    // one command the user wanted to repeat would be gone.
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "one");
    app.execute(Command::DuplicateLine);

    press(
        &mut app,
        KeyCode::Char('p'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert!(
        matches!(app.ui.overlay, Overlay::Palette(_)),
        "the palette opened"
    );
    assert_eq!(
        app.last_command(),
        Some(Command::DuplicateLine),
        "the record still names the command"
    );

    plain(&mut app, KeyCode::Esc);
    f4(&mut app);
    assert_eq!(text(&app), "one\none\none");
}

#[test]
fn the_slot_is_hidden_while_a_palette_is_open() {
    // The bar's job while an overlay is up is to teach the overlay's keys.
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "one");
    app.execute(Command::DuplicateLine);

    app.open_palette(PaletteMode::Commands);
    let bar = bar(&mut app, 120, 20);
    assert!(
        !bar.contains("repeat"),
        "no slot under the palette: {bar:?}"
    );
}

#[test]
fn clicking_the_slot_runs_it_again() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "one");
    app.execute(Command::DuplicateLine);

    let bar = bar(&mut app, 120, 20);
    let byte = bar.find("repeat").expect("the slot is drawn");
    let column = bar[..byte].chars().count() as u16;
    click(&mut app, column, 19);

    assert_eq!(
        text(&app),
        "one\none\none",
        "the click repeated the command"
    );
}

#[test]
fn a_bar_with_no_room_drops_the_slot_rather_than_the_hints() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "one");
    app.execute(Command::DuplicateLine);

    // Forty columns is half a bar's worth of slot: still there, trimmed.
    let narrow = bar(&mut app, 40, 16);
    assert!(narrow.contains("repeat"), "trimmed but present: {narrow:?}");
    assert!(narrow.contains('…'), "and cut with an ellipsis: {narrow:?}");

    // Twenty is not enough for the key and a word: the hints keep the row.
    let tiny = bar(&mut app, 20, 16);
    assert!(!tiny.contains("repeat"), "the slot stands down: {tiny:?}");
}

#[test]
fn the_hints_never_run_into_the_slot() {
    // The bar is one row, so the reservation has to hold at every width: the
    // last hint drawn must end before the slot begins, and the slot must end
    // inside the bar.
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "one");
    app.execute(Command::AddCursorToNextOccurrence);

    for width in [40u16, 60, 80, 100, 120] {
        screen(&mut app, width, 16);
        let hits = app.ui.hint_hits.clone();
        let slot = hits
            .iter()
            .find(|h| h.command == Command::RepeatLastCommand)
            .unwrap_or_else(|| panic!("the slot is drawn at {width}"));
        assert!(slot.end <= width, "the slot ends inside the bar: {slot:?}");
        for hit in hits
            .iter()
            .filter(|h| h.command != Command::RepeatLastCommand)
        {
            assert!(
                hit.end <= slot.start,
                "at {width} columns {hit:?} runs into the slot at {}",
                slot.start
            );
        }
    }
}

#[test]
fn the_slot_is_a_button_the_pointer_can_find() {
    // It is drawn last, so it is the last hit; hovering it has to name it and
    // nothing before it, or the bar would light up the wrong thing.
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    type_str(&mut app, "one");
    app.execute(Command::DuplicateLine);
    screen(&mut app, 120, 20);

    let slot = *app.ui.hint_hits.last().expect("a slot");
    assert_eq!(slot.command, Command::RepeatLastCommand);

    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: slot.start + 1,
        row: slot.row,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(
        app.ui.hovered_hint,
        Some(app.ui.hint_hits.len() - 1),
        "the pointer is on the slot"
    );
}

#[test]
fn f4_says_so_when_there_is_nothing_to_repeat() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());

    f4(&mut app);

    assert_eq!(app.last_command(), None, "nothing was run");
    assert!(
        screen_has(&mut app, 100, 20, "Nothing to repeat"),
        "and the bar says why nothing happened"
    );
}
