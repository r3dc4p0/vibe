//! Which shortcuts the host keeps for itself, and which ones the wire merges.
//!
//! # What cannot be detected
//!
//! A key the terminal, the multiplexer or the window manager handles never
//! reaches this process. There is no event, no error, nothing to observe — so
//! Vibe cannot *detect* that `Ctrl+T` opened a browser tab. It can only know
//! that this host is known to take that key, and show what did arrive for the
//! keys that survive.
//!
//! # What can
//!
//! Two things, and both are here.
//!
//! * **The host's identity**, read from the environment. That says which of its
//!   own shortcuts are in the way — `Ctrl+Shift+P` in the VS Code terminal,
//!   `Ctrl+B` under tmux, `Ctrl+A` under screen.
//! * **The wire.** Without the extended keyboard protocol, a terminal sends one
//!   byte for a whole family of chords: `Ctrl+Shift+S` and `Ctrl+S` are both
//!   `0x13`, and `Ctrl+I`, `Ctrl+M`, `Ctrl+H` and `Ctrl+[` are Tab, Enter,
//!   Backspace and Esc. Vibe already asks for the protocol when it believes the
//!   terminal speaks it ([`crate::terminal::should_enable_enhancement`]); this
//!   module reports what is left when it cannot.
//!
//! The point of both is the same: a shortcut that cannot work should say so,
//! rather than fail silently.

use crossterm::event::{KeyCode, KeyModifiers};

use crate::commands::Command;
use crate::keymap::{KeyChord, Keymap};

/// A shortcut the host consumes before the application is asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Taken {
    /// Chord in `ctrl+shift+p` notation.
    pub chord: &'static str,
    /// What takes it.
    pub by: &'static str,
    /// What the user can do about it.
    pub fix: &'static str,
}

/// The environment Vibe is running inside.
///
/// Several can be true at once — tmux inside iTerm2 is the common one — so
/// detection returns a list, innermost first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    Tmux,
    Screen,
    Zellij,
    Vscode,
    Iterm,
    AppleTerminal,
    Kitty,
    Wezterm,
    Ghostty,
    Alacritty,
    Other,
}

impl Host {
    pub fn name(self) -> &'static str {
        match self {
            Host::Tmux => "tmux",
            Host::Screen => "GNU screen",
            Host::Zellij => "Zellij",
            Host::Vscode => "the VS Code terminal",
            Host::Iterm => "iTerm2",
            Host::AppleTerminal => "Terminal.app",
            Host::Kitty => "kitty",
            Host::Wezterm => "WezTerm",
            Host::Ghostty => "Ghostty",
            Host::Alacritty => "Alacritty",
            Host::Other => "an unrecognised terminal",
        }
    }

    /// Shortcuts this host takes by default.
    ///
    /// Only defaults: a terminal with a custom keymap can take anything, which
    /// is why the overlay also shows what actually arrives.
    pub fn takes(self) -> &'static [Taken] {
        match self {
            Host::Vscode => VSCODE,
            Host::Tmux => TMUX,
            Host::Screen => SCREEN,
            Host::Zellij => ZELLIJ,
            _ => &[],
        }
    }
}

const VSCODE: &[Taken] = &[
    Taken {
        chord: "ctrl+shift+p",
        by: "it opens the editor's command palette",
        fix: "add showCommands to terminal.integrated.commandsToSkipShell",
    },
    Taken {
        chord: "ctrl+p",
        by: "it opens the editor's quick open",
        fix: "add workbench.action.quickOpen to terminal.integrated.commandsToSkipShell",
    },
];

const TMUX: &[Taken] = &[Taken {
    chord: "ctrl+b",
    by: "it is the tmux prefix",
    fix: "tap it twice to send it through, or set -g prefix C-a",
}];

const SCREEN: &[Taken] = &[Taken {
    chord: "ctrl+a",
    by: "it is the screen command key",
    fix: "tap it twice to send it through, or set a different escape",
}];

const ZELLIJ: &[Taken] = &[
    Taken {
        chord: "ctrl+p",
        by: "it opens Zellij's pane mode",
        fix: "rebind it in the Zellij config",
    },
    Taken {
        chord: "ctrl+n",
        by: "it opens Zellij's resize mode",
        fix: "rebind it in the Zellij config",
    },
    Taken {
        chord: "ctrl+t",
        by: "it opens Zellij's tab mode",
        fix: "rebind it in the Zellij config",
    },
    Taken {
        chord: "ctrl+s",
        by: "it opens Zellij's scroll mode",
        fix: "rebind it in the Zellij config",
    },
    Taken {
        chord: "ctrl+o",
        by: "it opens Zellij's session mode",
        fix: "rebind it in the Zellij config",
    },
    Taken {
        chord: "ctrl+q",
        by: "it quits Zellij",
        fix: "rebind it in the Zellij config",
    },
];

/// Detect the hosts from the environment.
pub fn detect() -> Vec<Host> {
    detect_with(&|name| std::env::var(name).ok())
}

/// The same, from an injected environment, so it can be tested.
pub fn detect_with(env: &dyn Fn(&str) -> Option<String>) -> Vec<Host> {
    let get = |name: &str| env(name).unwrap_or_default().to_ascii_lowercase();
    let term = get("TERM");
    let program = get("TERM_PROGRAM");
    let mut hosts = Vec::new();

    // Innermost first: the multiplexer is the last thing between us and the
    // terminal, so it is the first thing a key can be lost to.
    if env("TMUX").is_some() {
        hosts.push(Host::Tmux);
    } else if env("STY").is_some() || term.starts_with("screen") {
        hosts.push(Host::Screen);
    }
    if env("ZELLIJ").is_some() {
        hosts.push(Host::Zellij);
    }

    let outer = if program == "vscode" {
        Host::Vscode
    } else if program == "iterm.app" {
        Host::Iterm
    } else if program == "apple_terminal" {
        Host::AppleTerminal
    } else if program == "wezterm" || term.contains("wezterm") {
        Host::Wezterm
    } else if program == "ghostty" || term.contains("ghostty") {
        Host::Ghostty
    } else if program == "kitty" || term.contains("kitty") || env("KITTY_WINDOW_ID").is_some() {
        Host::Kitty
    } else if term.contains("alacritty") {
        Host::Alacritty
    } else {
        Host::Other
    };
    if !hosts.contains(&outer) {
        hosts.push(outer);
    }
    hosts
}

/// The single byte a legacy terminal sends for this chord.
///
/// `None` means "no claim": Alt is an escape prefix rather than a byte,
/// function keys have their own sequences, and a chord with a modifier the
/// terminal encodes differently (Shift+Tab, Ctrl+Tab) is not modelled here.
pub fn legacy_byte(chord: &KeyChord) -> Option<u8> {
    if chord.mods.contains(KeyModifiers::ALT) || chord.mods.contains(KeyModifiers::SUPER) {
        return None;
    }
    let ctrl = chord.mods.contains(KeyModifiers::CONTROL);
    match chord.code {
        KeyCode::Char(c) if ctrl => control_byte(c),
        KeyCode::Char(c) => c.is_ascii().then_some(c as u8),
        // Only unmodified: Shift+Tab is `CSI Z` and Ctrl+Tab is not a byte.
        KeyCode::Tab if chord.mods.is_empty() => Some(0x09),
        KeyCode::Enter if chord.mods.is_empty() => Some(0x0D),
        KeyCode::Esc if chord.mods.is_empty() => Some(0x1B),
        KeyCode::Backspace if chord.mods.is_empty() => Some(0x7F),
        _ => None,
    }
}

/// The control byte for `Ctrl+<c>`.
///
/// The NUL family (`Ctrl+@`, `Ctrl+Space`, `Ctrl+2`) is deliberately absent:
/// terminals disagree about whether it even produces an event, and none of the
/// defaults use it.
fn control_byte(c: char) -> Option<u8> {
    let c = c.to_ascii_lowercase();
    Some(match c {
        'a'..='z' => c as u8 - b'a' + 1,
        '[' => 0x1B,
        '\\' => 0x1C,
        ']' => 0x1D,
        '^' => 0x1E,
        '_' | '/' => 0x1F,
        '?' => 0x7F,
        _ => return None,
    })
}

/// What the editor sees when a legacy terminal sends `byte`.
///
/// This mirrors crossterm's decoder, which is where the merging actually
/// happens: `0x09` is Tab however it was produced, and `0x13` is `Ctrl+S`
/// whether or not Shift was held.
pub fn arrives_as(byte: u8) -> KeyChord {
    let none = KeyModifiers::NONE;
    match byte {
        0x08 | 0x7F => KeyChord::new(KeyCode::Backspace, none),
        0x09 => KeyChord::new(KeyCode::Tab, none),
        0x0A | 0x0D => KeyChord::new(KeyCode::Enter, none),
        0x1B => KeyChord::new(KeyCode::Esc, none),
        0x1C..=0x1F => {
            let c = match byte {
                0x1C => '\\',
                0x1D => ']',
                0x1E => '^',
                _ => '_',
            };
            KeyChord::new(KeyCode::Char(c), KeyModifiers::CONTROL)
        }
        0x01..=0x1A => KeyChord::new(
            KeyCode::Char((b'a' + byte - 1) as char),
            KeyModifiers::CONTROL,
        ),
        other => KeyChord::new(KeyCode::Char(other as char), none),
    }
}

/// Why a binding will not do what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// The host takes the key before the application is asked.
    Taken {
        host: &'static str,
        by: &'static str,
        fix: &'static str,
    },
    /// The wire turns the chord into a different one.
    Rewritten {
        byte: u8,
        arrives: KeyChord,
        /// What the arriving chord runs, if anything is bound to it.
        runs: Option<Command>,
    },
}

/// One binding that cannot work as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub chord: KeyChord,
    pub command: Command,
    pub kind: Kind,
}

/// Every binding in `map` that the host or the wire will not deliver.
///
/// `enhanced` is whether the extended keyboard protocol is active: with it,
/// `Ctrl+Shift+S` is its own event and nothing is merged, so only the host's
/// own shortcuts are left.
pub fn problems(map: &Keymap, hosts: &[Host], enhanced: bool) -> Vec<Problem> {
    let mut out: Vec<Problem> = Vec::new();
    for (chord, command) in map.all() {
        for host in hosts {
            for taken in host.takes() {
                if KeyChord::parse(taken.chord).is_some_and(|t| t == chord) {
                    out.push(Problem {
                        chord,
                        command,
                        kind: Kind::Taken {
                            host: host.name(),
                            by: taken.by,
                            fix: taken.fix,
                        },
                    });
                }
            }
        }
        if enhanced {
            continue;
        }
        let Some(byte) = legacy_byte(&chord) else {
            continue;
        };
        let arrives = arrives_as(byte);
        // A chord that merges into a sibling bound to the *same* command loses
        // nothing: `Ctrl+Shift+C` exists only as an escape hatch for terminals
        // that turn `Ctrl+C` into SIGINT, and with the protocol off it lands on
        // `Ctrl+C`, which copies. Reporting that would be noise.
        if arrives != chord && map.lookup(&arrives) != Some(command) {
            out.push(Problem {
                chord,
                command,
                kind: Kind::Rewritten {
                    byte,
                    arrives,
                    runs: map.lookup(&arrives),
                },
            });
        }
    }
    out
}

/// The report, as lines for the help-style overlay or `--key-report`.
///
/// Lines indented by four spaces are body text; everything else is a heading.
pub fn report(map: &Keymap, hosts: &[Host], enhanced: bool) -> Vec<String> {
    let mut out = Vec::new();
    out.push(format!(
        "Vibe {} — {} — {}",
        crate::VERSION,
        crate::platform::os_name(),
        crate::platform::term_name()
    ));

    let names: Vec<&str> = hosts.iter().map(|h| h.name()).collect();
    out.push(format!(
        "Host: {}",
        if names.is_empty() {
            "unknown".to_string()
        } else {
            names.join(" inside ")
        }
    ));
    out.push(format!(
        "Extended keys: {}",
        if enhanced {
            "on — Ctrl+Shift+X is its own key"
        } else {
            "off — Ctrl+Shift+X arrives as Ctrl+X"
        }
    ));

    let found = problems(map, hosts, enhanced);
    if found.is_empty() {
        out.push(String::new());
        out.push("Nothing in the way.".to_string());
        out.push("    Every binding in this keymap can be delivered here.".to_string());
        return out;
    }

    let taken: Vec<&Problem> = found
        .iter()
        .filter(|p| matches!(p.kind, Kind::Taken { .. }))
        .collect();
    let merged: Vec<&Problem> = found
        .iter()
        .filter(|p| matches!(p.kind, Kind::Rewritten { .. }))
        .collect();

    if !taken.is_empty() {
        out.push(String::new());
        out.push("Taken by the host".to_string());
        // One row per binding, one line per host that takes it: Zellij inside
        // the VS Code terminal loses `Ctrl+P` twice, and the two fixes are
        // different. `problems` pushes a chord's hosts consecutively, so this
        // is a walk rather than a lookup.
        let mut i = 0;
        while i < taken.len() {
            let first = taken[i];
            out.push(format!(
                "  {:<16} {}",
                first.chord.label(),
                first.command.title()
            ));
            while i < taken.len()
                && taken[i].chord == first.chord
                && taken[i].command == first.command
            {
                if let Kind::Taken { host, by, fix } = &taken[i].kind {
                    out.push(format!("      {host}: {by}."));
                    out.push(format!("      {fix}."));
                }
                i += 1;
            }
        }
    }

    if !merged.is_empty() {
        out.push(String::new());
        out.push("Merged on the wire".to_string());
        for p in merged {
            let Kind::Rewritten { arrives, runs, .. } = &p.kind else {
                continue;
            };
            out.push(format!("  {:<16} {}", p.chord.label(), p.command.title()));
            out.push(format!(
                "      arrives as {}, which {}",
                arrives.label(),
                match runs {
                    Some(cmd) => format!("runs {}", cmd.title()),
                    None => "is not a shortcut".to_string(),
                }
            ));
        }
        out.push(String::new());
        out.push("    Turn on [terminal] keyboard_enhancement if your terminal".to_string());
        out.push("    speaks the extended protocol, or rebind these to Alt or F-keys.".to_string());
    }

    out
}
