//! The keys that survive the terminal.
//!
//! # Why this exists
//!
//! A shortcut the terminal, the multiplexer or the window manager handles never
//! reaches this process. [`crate::keyreport`] can say which of our bindings a
//! known host takes, and which ones the wire merges into a sibling, but knowing
//! that does not make the key arrive. What is left is a second way to reach the
//! same command, on keys nothing in the middle claims.
//!
//! # The rule
//!
//! `Ctrl+K` is a prefix. It is a plain control byte, which every terminal
//! delivers, and nothing in the default keymap is bound to it. After it, the
//! next key names a command by the key it already had: the modifiers are
//! dropped and the key is kept, shift included.
//!
//! ```text
//! Alt+F          ->  Ctrl+K f
//! Ctrl+Shift+F   ->  Ctrl+K F
//! Ctrl+Shift+E   ->  Ctrl+K E
//! Alt+Up         ->  Ctrl+K Up
//! Ctrl+Shift+Up  ->  Ctrl+K Shift+Up
//! ```
//!
//! Keeping the key is the point: a user who reaches for `Alt+F`, gets nothing,
//! and knows the rule has one thing to remember rather than a second keymap.
//! Dropping the modifiers is what makes it safe. The wire loses `Shift` on a
//! *control* chord — `Ctrl+Shift+F` and `Ctrl+F` are both `0x06` — but a shifted
//! plain key is an ordinary character and arrives as one.
//!
//! Shift is a tie-breaker rather than a requirement: `Ctrl+K F` is fold-all and
//! `Ctrl+K f` is fold, but `Ctrl+K e` and `Ctrl+K E` both reach unfold-all,
//! because only one command claims that key and refusing the other case would
//! punish a habit instead of teaching a rule.
//!
//! # What is not here
//!
//! `Ctrl+Alt+W` (close pane) has no slot: its key is `w`, which `Alt+W` (word
//! wrap) already claims, and `W` is word count. Closing a pane is in the
//! palette. Every other binding a terminal can take or merge has one.

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::commands::Command;
use crate::keymap::KeyChord;

/// How long the prefix waits for its key.
///
/// Long enough to be deliberate, short enough that a `Ctrl+K` pressed by
/// accident does not swallow the next thing typed.
pub const TIMEOUT: Duration = Duration::from_millis(1500);

/// One command's safe key.
pub struct Safe {
    /// The key pressed after the prefix.
    pub code: KeyCode,
    /// Whether it is pressed with shift.
    pub shift: bool,
    pub command: Command,
    /// The binding it stands in for, in config notation.
    pub mirrors: &'static str,
    /// What the bar calls it — one or two words, because the bar is one row.
    pub label: &'static str,
}

impl Safe {
    /// The key as it is shown: `f`, `F`, `↑`.
    pub fn key_label(&self) -> String {
        match self.code {
            KeyCode::Char(c) if self.shift => c.to_ascii_uppercase().to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Up if self.shift => "Shift+↑".to_string(),
            KeyCode::Down if self.shift => "Shift+↓".to_string(),
            KeyCode::Up => "↑".to_string(),
            KeyCode::Down => "↓".to_string(),
            KeyCode::Left => "←".to_string(),
            KeyCode::Right => "→".to_string(),
            _ => "?".to_string(),
        }
    }

    /// The binding this replaces, as it is shown: `Alt+F`.
    pub fn mirrors_label(&self) -> String {
        KeyChord::parse(self.mirrors)
            .map(|c| c.label())
            .unwrap_or_default()
    }
}

/// Every command a terminal can take away, and the key that gets it back.
///
/// Ordered by what a user is most likely to reach for, because the bar shows
/// the first few and the rest only fit on the help screen.
pub const LAYER: &[Safe] = &[
    Safe {
        code: KeyCode::Char('f'),
        shift: false,
        command: Command::ToggleFold,
        mirrors: "alt+f",
        label: "fold",
    },
    Safe {
        code: KeyCode::Char('f'),
        shift: true,
        command: Command::FoldAll,
        mirrors: "ctrl+shift+f",
        label: "fold all",
    },
    Safe {
        code: KeyCode::Char('e'),
        shift: true,
        command: Command::UnfoldAll,
        mirrors: "ctrl+shift+e",
        label: "unfold",
    },
    Safe {
        code: KeyCode::Char('p'),
        shift: true,
        command: Command::CommandPalette,
        mirrors: "ctrl+shift+p",
        label: "commands",
    },
    Safe {
        code: KeyCode::Char('w'),
        shift: false,
        command: Command::ToggleWordWrap,
        mirrors: "alt+w",
        label: "wrap",
    },
    Safe {
        code: KeyCode::Char('z'),
        shift: false,
        command: Command::ToggleWhitespace,
        mirrors: "alt+z",
        label: "spaces",
    },
    Safe {
        code: KeyCode::Char('d'),
        shift: false,
        command: Command::CompareWith,
        mirrors: "alt+d",
        label: "compare",
    },
    Safe {
        code: KeyCode::Up,
        shift: false,
        command: Command::MoveLineUp,
        mirrors: "alt+up",
        label: "line up",
    },
    Safe {
        code: KeyCode::Down,
        shift: false,
        command: Command::MoveLineDown,
        mirrors: "alt+down",
        label: "line down",
    },
    Safe {
        code: KeyCode::Char('d'),
        shift: true,
        command: Command::DuplicateLine,
        mirrors: "ctrl+shift+d",
        label: "duplicate",
    },
    Safe {
        code: KeyCode::Char('k'),
        shift: true,
        command: Command::DeleteLine,
        mirrors: "ctrl+shift+k",
        label: "del line",
    },
    Safe {
        code: KeyCode::Char('s'),
        shift: true,
        command: Command::SaveAs,
        mirrors: "ctrl+shift+s",
        label: "save as",
    },
    Safe {
        code: KeyCode::Char('b'),
        shift: false,
        command: Command::FocusBookmarkPanel,
        mirrors: "ctrl+alt+b",
        label: "panel",
    },
    Safe {
        code: KeyCode::Char('b'),
        shift: true,
        command: Command::ToggleBookmarkPanel,
        mirrors: "ctrl+shift+b",
        label: "bookmarks",
    },
    Safe {
        code: KeyCode::Char('t'),
        shift: true,
        command: Command::ToggleTheme,
        mirrors: "ctrl+shift+t",
        label: "theme",
    },
    Safe {
        code: KeyCode::Char('g'),
        shift: true,
        command: Command::ToggleIndentGuides,
        mirrors: "ctrl+shift+g",
        label: "guides",
    },
    Safe {
        code: KeyCode::Char('u'),
        shift: true,
        command: Command::ToggleUiStyle,
        mirrors: "ctrl+shift+u",
        label: "ui style",
    },
    Safe {
        code: KeyCode::Char('a'),
        shift: true,
        command: Command::ToggleAutoPairs,
        mirrors: "ctrl+shift+a",
        label: "pairs",
    },
    Safe {
        code: KeyCode::Char('w'),
        shift: true,
        command: Command::WordCount,
        mirrors: "ctrl+shift+w",
        label: "words",
    },
    Safe {
        code: KeyCode::Char('i'),
        shift: true,
        command: Command::SetIndentation,
        mirrors: "ctrl+shift+i",
        label: "indent",
    },
    Safe {
        code: KeyCode::Char('r'),
        shift: true,
        command: Command::Reload,
        mirrors: "ctrl+shift+r",
        label: "reload",
    },
    Safe {
        code: KeyCode::Char('c'),
        shift: true,
        command: Command::Copy,
        mirrors: "ctrl+shift+c",
        label: "copy",
    },
    Safe {
        code: KeyCode::Char('q'),
        shift: true,
        command: Command::QuitAll,
        mirrors: "ctrl+shift+q",
        label: "quit all",
    },
    Safe {
        code: KeyCode::Up,
        shift: true,
        command: Command::AddCursorAbove,
        mirrors: "ctrl+shift+up",
        label: "cursor up",
    },
    Safe {
        code: KeyCode::Down,
        shift: true,
        command: Command::AddCursorBelow,
        mirrors: "ctrl+shift+down",
        label: "cursor down",
    },
    Safe {
        code: KeyCode::Left,
        shift: false,
        command: Command::FocusPaneLeft,
        mirrors: "ctrl+alt+left",
        label: "pane left",
    },
    Safe {
        code: KeyCode::Right,
        shift: false,
        command: Command::FocusPaneRight,
        mirrors: "ctrl+alt+right",
        label: "pane right",
    },
];

/// The chord that arms the layer.
pub fn prefix() -> KeyChord {
    KeyChord::new(KeyCode::Char('k'), KeyModifiers::CONTROL)
}

/// The command `key` names in the layer, if any.
///
/// A key with `Ctrl` or `Alt` held is a different chord naming a different
/// command, and is never swallowed here: `Ctrl+K` then `Ctrl+F` is Find.
pub fn resolve(key: &KeyEvent) -> Option<Command> {
    let chord = KeyChord::from_event(key);
    if chord
        .mods
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return None;
    }
    let shift = chord.mods.contains(KeyModifiers::SHIFT);
    let find = |shift: bool| {
        LAYER
            .iter()
            .find(|s| s.code == chord.code && s.shift == shift)
            .map(|s| s.command)
    };
    // The exact key first, then the other case — see the module note on shift.
    find(shift).or_else(|| find(!shift))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Keymap;

    fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn the_prefix_is_not_a_default_binding() {
        assert_eq!(Keymap::defaults().lookup(&prefix()), None);
    }

    #[test]
    fn every_mirror_is_bound_to_its_command() {
        let map = Keymap::defaults();
        for entry in LAYER {
            let mirrored = KeyChord::parse(entry.mirrors).unwrap();
            assert_eq!(
                map.lookup(&mirrored),
                Some(entry.command),
                "{} says it stands in for {}",
                entry.label,
                entry.mirrors
            );
        }
    }

    #[test]
    fn every_alt_binding_has_a_safe_key_but_one() {
        // Stated as "exactly this one is uncovered" rather than as a skip
        // list, so a new `Alt` binding with no safe key fails the test instead
        // of joining an exemption nobody re-reads.
        let mut uncovered: Vec<Command> = Vec::new();
        for (chord, command) in Keymap::defaults().all() {
            if !chord.mods.contains(KeyModifiers::ALT) {
                continue;
            }
            if !LAYER.iter().any(|s| s.command == command) && !uncovered.contains(&command) {
                uncovered.push(command);
            }
        }
        assert_eq!(
            uncovered,
            vec![Command::ClosePane],
            "Close Pane is the one Alt binding without a slot: `w` is word wrap's"
        );
    }

    #[test]
    fn no_two_entries_claim_the_same_key() {
        for (i, a) in LAYER.iter().enumerate() {
            for b in &LAYER[i + 1..] {
                assert!(
                    !(a.code == b.code && a.shift == b.shift),
                    "{} and {} both want {:?}",
                    a.label,
                    b.label,
                    a.code
                );
            }
        }
    }

    #[test]
    fn shift_is_a_tie_breaker_not_a_requirement() {
        // Two commands claim `f`, so the case decides between them.
        assert_eq!(
            resolve(&key(KeyCode::Char('f'), KeyModifiers::NONE)),
            Some(Command::ToggleFold)
        );
        assert_eq!(
            resolve(&key(KeyCode::Char('F'), KeyModifiers::SHIFT)),
            Some(Command::FoldAll)
        );
        // Only one claims `e`, so either case reaches it.
        assert_eq!(
            resolve(&key(KeyCode::Char('e'), KeyModifiers::NONE)),
            Some(Command::UnfoldAll)
        );
        assert_eq!(
            resolve(&key(KeyCode::Char('E'), KeyModifiers::SHIFT)),
            Some(Command::UnfoldAll)
        );
    }

    #[test]
    fn terminals_that_report_the_shift_either_way_agree() {
        // With the extended protocol, Shift+f arrives as `F` plus SHIFT; on a
        // legacy one it arrives as `F` and nothing else. Both are fold-all.
        for event in [
            key(KeyCode::Char('F'), KeyModifiers::SHIFT),
            key(KeyCode::Char('F'), KeyModifiers::NONE),
        ] {
            assert_eq!(resolve(&event), Some(Command::FoldAll));
        }
    }

    #[test]
    fn modified_keys_are_not_layer_keys() {
        assert_eq!(
            resolve(&key(KeyCode::Char('f'), KeyModifiers::CONTROL)),
            None,
            "Ctrl+F is Find, not fold"
        );
        assert_eq!(
            resolve(&key(KeyCode::Char('f'), KeyModifiers::ALT)),
            None,
            "Alt+F is fold's own binding, not the layer's"
        );
    }

    #[test]
    fn arrows_resolve_by_direction_and_shift() {
        assert_eq!(
            resolve(&key(KeyCode::Up, KeyModifiers::NONE)),
            Some(Command::MoveLineUp)
        );
        assert_eq!(
            resolve(&key(KeyCode::Up, KeyModifiers::SHIFT)),
            Some(Command::AddCursorAbove)
        );
        assert_eq!(
            resolve(&key(KeyCode::Left, KeyModifiers::NONE)),
            Some(Command::FocusPaneLeft)
        );
        assert_eq!(
            resolve(&key(KeyCode::Right, KeyModifiers::NONE)),
            Some(Command::FocusPaneRight)
        );
    }

    #[test]
    fn a_key_outside_the_layer_resolves_to_nothing() {
        assert_eq!(resolve(&key(KeyCode::Char('x'), KeyModifiers::NONE)), None);
        assert_eq!(resolve(&key(KeyCode::Enter, KeyModifiers::NONE)), None);
    }
}
