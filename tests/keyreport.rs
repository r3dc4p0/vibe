//! The key report: what the host takes, and what the wire merges.
//!
//! Two halves. The first is the model — which byte a chord becomes, what the
//! editor makes of that byte, and which hosts are in the way — checked against
//! the real default keymap. The second drives the panel itself, because the
//! panel's whole value is the echo: press the shortcut that does nothing and
//! read what actually arrived.

mod common;

use common::*;
use crossterm::event::{KeyCode, KeyModifiers};
use vibe::commands::Command;
use vibe::keymap::{KeyChord, Keymap};
use vibe::keyreport::{self, Host, Kind};

fn chord(spec: &str) -> KeyChord {
    KeyChord::parse(spec).expect("a valid chord")
}

fn defaults() -> Keymap {
    Keymap::defaults()
}

#[test]
fn shift_is_the_first_thing_the_wire_loses() {
    // Ctrl+Shift+S and Ctrl+S are one byte, and the byte is the unshifted one.
    assert_eq!(keyreport::legacy_byte(&chord("ctrl+shift+s")), Some(0x13));
    assert_eq!(keyreport::legacy_byte(&chord("ctrl+s")), Some(0x13));
    assert_eq!(keyreport::arrives_as(0x13), chord("ctrl+s"));
}

#[test]
fn some_control_keys_are_really_other_keys() {
    // These never reach the editor as themselves: the byte they share with a
    // named key is decoded as that key first.
    for (spec, expected) in [
        ("ctrl+i", KeyCode::Tab),
        ("ctrl+m", KeyCode::Enter),
        ("ctrl+j", KeyCode::Enter),
        ("ctrl+h", KeyCode::Backspace),
        ("ctrl+[", KeyCode::Esc),
    ] {
        let byte = keyreport::legacy_byte(&chord(spec)).expect(spec);
        assert_eq!(
            keyreport::arrives_as(byte).code,
            expected,
            "{spec} arrives as the key for byte {byte:#04x}"
        );
    }
}

#[test]
fn a_chord_the_wire_carries_unchanged_is_not_a_problem() {
    let found = keyreport::problems(&defaults(), &[], false);
    assert!(
        found.iter().all(|p| p.command != Command::Save),
        "Ctrl+S arrives as Ctrl+S"
    );
    assert!(
        found.iter().all(|p| p.command != Command::Undo),
        "Ctrl+Z arrives as Ctrl+Z"
    );
}

#[test]
fn chords_with_no_byte_make_no_claim() {
    // Alt is an escape prefix, function keys have their own sequences, and
    // Ctrl+Tab is not a control byte at all. Guessing about these would
    // produce warnings that are not true.
    for spec in ["alt+d", "alt+up", "f1", "ctrl+tab", "shift+f3"] {
        assert_eq!(keyreport::legacy_byte(&chord(spec)), None, "{spec}");
    }
}

#[test]
fn save_as_is_lost_to_save_without_the_protocol() {
    let found = keyreport::problems(&defaults(), &[], false);
    let save_as = found
        .iter()
        .find(|p| p.command == Command::SaveAs)
        .expect("Ctrl+Shift+S is reported");
    match &save_as.kind {
        Kind::Rewritten { arrives, runs, .. } => {
            assert_eq!(*arrives, chord("ctrl+s"));
            assert_eq!(*runs, Some(Command::Save), "the wrong thing happens");
        }
        other => panic!("expected a rewritten binding, got {other:?}"),
    }
}

#[test]
fn the_copy_escape_hatch_is_not_reported() {
    // Ctrl+Shift+C exists for terminals that turn Ctrl+C into SIGINT. Without
    // the protocol it lands on Ctrl+C, which copies — nothing is lost, and a
    // warning here would be noise.
    let found = keyreport::problems(&defaults(), &[], false);
    assert!(found.iter().all(|p| p.command != Command::Copy));
}

#[test]
fn the_extended_protocol_removes_every_merged_binding() {
    let found = keyreport::problems(&defaults(), &[], true);
    assert!(
        found
            .iter()
            .all(|p| !matches!(p.kind, Kind::Rewritten { .. })),
        "with the protocol on, no chord is merged: {found:?}"
    );
}

#[test]
fn the_vs_code_terminal_takes_the_palette_key() {
    let found = keyreport::problems(&defaults(), &[Host::Vscode], true);
    let palette = found
        .iter()
        .find(|p| p.command == Command::CommandPalette)
        .expect("Ctrl+Shift+P is reported");
    match &palette.kind {
        Kind::Taken { host, fix, .. } => {
            assert_eq!(*host, "the VS Code terminal");
            assert!(fix.contains("commandsToSkipShell"), "{fix}");
        }
        other => panic!("expected a taken binding, got {other:?}"),
    }
    // And quick open, which VS Code also keeps.
    assert!(found.iter().any(|p| p.command == Command::QuickOpen));
}

#[test]
fn hosts_are_detected_innermost_first() {
    let env = |name: &str| match name {
        "TMUX" => Some("/tmp/tmux-501/default,1,0".to_string()),
        "TERM_PROGRAM" => Some("iTerm.app".to_string()),
        _ => None,
    };
    assert_eq!(
        keyreport::detect_with(&env),
        vec![Host::Tmux, Host::Iterm],
        "the multiplexer is the last thing between us and the terminal"
    );
}

#[test]
fn a_terminal_with_nothing_to_say_is_named_not_guessed() {
    let env = |_: &str| None;
    assert_eq!(keyreport::detect_with(&env), vec![Host::Other]);
    assert_eq!(Host::Other.takes(), &[], "no table, no claims");
}

#[test]
fn the_report_names_the_host_and_says_what_arrives() {
    let lines = keyreport::report(&defaults(), &[Host::Tmux, Host::Iterm], false);
    let text = lines.join("\n");
    assert!(text.contains("Host: tmux inside iTerm2"), "{text}");
    assert!(text.contains("Extended keys: off"), "{text}");
    assert!(
        text.contains("arrives as Ctrl+s, which runs Save"),
        "{text}"
    );
}

#[test]
fn a_clean_terminal_says_so() {
    let lines = keyreport::report(&defaults(), &[Host::Kitty], true);
    let text = lines.join("\n");
    assert!(text.contains("Nothing in the way."), "{text}");
}

#[test]
fn f2_opens_the_report() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    plain(&mut app, KeyCode::F(2));
    let rows = screen(&mut app, 80, 30);
    assert!(
        rows.iter().any(|r| r.contains("Which Keys Reach Vibe")),
        "{rows:#?}"
    );
}

#[test]
fn the_panel_echoes_the_key_that_arrived_and_what_it_runs() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    plain(&mut app, KeyCode::F(2));

    // The case the panel exists for: on a terminal without the protocol this
    // is what Ctrl+Shift+P becomes, and it runs the wrong command.
    press(
        &mut app,
        KeyCode::Char('p'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    // Wide enough for the whole echo line: a narrow terminal truncates it, and
    // the label at the front is the part that must survive.
    let rows = screen(&mut app, 120, 32);
    let echo = rows
        .iter()
        .find(|r| r.contains("Last key:"))
        .expect("the echo row");
    assert!(echo.contains("Ctrl+Shift+P"), "{echo}");
    assert!(echo.contains("→ runs Command Palette"), "{echo}");

    // The same key without Shift is a different command, which is the point.
    press(&mut app, KeyCode::Char('p'), KeyModifiers::CONTROL);
    let rows = screen(&mut app, 120, 32);
    let echo = rows
        .iter()
        .find(|r| r.contains("Last key:"))
        .expect("the echo row");
    assert!(echo.contains("Ctrl+p"), "{echo}");
    assert!(echo.contains("→ runs Quick Open File"), "{echo}");
}

#[test]
fn the_echo_shows_the_raw_modifiers_not_just_a_pretty_label() {
    // A label alone would hide the problem: "Ctrl+p" looks deliberate. The raw
    // event is what shows that Shift never arrived.
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    plain(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::Char('p'), KeyModifiers::CONTROL);
    let rows = screen(&mut app, 80, 30);
    let echo = rows.iter().find(|r| r.contains("Last key:")).unwrap();
    assert!(echo.contains("CTRL"), "{echo}");
    assert!(!echo.contains("SHIFT"), "no shift was sent: {echo}");
}

#[test]
fn a_typed_character_is_reported_as_typing() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    plain(&mut app, KeyCode::F(2));
    plain(&mut app, KeyCode::Char('x'));
    let rows = screen(&mut app, 80, 30);
    let echo = rows.iter().find(|r| r.contains("Last key:")).unwrap();
    assert!(echo.contains("→ types 'x'"), "{echo}");
    // And it did not reach the document: the panel is modal.
    assert!(text(&app).is_empty(), "the panel swallows text");
}

#[test]
fn esc_closes_the_report() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    plain(&mut app, KeyCode::F(2));
    plain(&mut app, KeyCode::Esc);
    let rows = screen(&mut app, 80, 30);
    assert!(
        !rows.iter().any(|r| r.contains("Which Keys Reach Vibe")),
        "the panel is gone"
    );
}

#[test]
fn the_panel_scrolls_to_the_end_of_a_long_report() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    plain(&mut app, KeyCode::F(2));
    // The default keymap on a legacy terminal reports a dozen or more merged
    // chords; walk past the bottom and the list must stay put, not blank out.
    for _ in 0..40 {
        plain(&mut app, KeyCode::Down);
    }
    let rows = screen(&mut app, 80, 30);
    assert!(
        rows.iter()
            .any(|r| r.contains("speaks the extended protocol")),
        "the last line of the report is on screen: {rows:#?}"
    );
    assert!(
        rows.iter().any(|r| r.contains("Which Keys Reach Vibe")),
        "and the panel is still open"
    );
    assert!(rows.iter().any(|r| r.contains("Last key:")), "echo stays");
}

/// Dumps the panel cell by cell, for a visual check.
///
/// A terminal UI cannot be judged from a text dump: the borders, the rule above
/// the echo and the heading weight are half of what the panel is. Render the
/// dump with `tools/cells_to_png.py` and look at it.
#[test]
#[ignore = "writes /tmp/keyreport-cells.txt for a visual check"]
fn dump_the_panel_cells() {
    let scratch = Scratch::new();
    let mut app = editor(scratch.path());
    plain(&mut app, KeyCode::F(2));
    press(
        &mut app,
        KeyCode::Char('p'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 34)).unwrap();
    terminal.draw(|f| vibe::ui::draw(f, &mut app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let cell = &buf[(x, y)];
            let symbol = cell.symbol();
            if symbol.is_empty() || symbol == " " {
                continue;
            }
            out.push_str(&format!(
                "{x}\t{y}\t{:?}\t{:?}\t{}\n",
                cell.fg, cell.bg, symbol
            ));
        }
    }
    std::fs::write("/tmp/keyreport-cells.txt", out).unwrap();
}
