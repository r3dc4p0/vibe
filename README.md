# Vibe

A modern, fast terminal text editor with the keyboard shortcuts you already know.

Vibe is modal-free: `Ctrl+S` saves, `Ctrl+F` finds, `Ctrl+Z` undoes, `Ctrl+P`
jumps to a file, `Ctrl+Shift+P` opens a command palette. It opens instantly,
handles large files without stalling, and never loses your work.

[Website](https://r3dc4p0.github.io/vibe/) · [Commands](docs/commands.md) ·
[Key bindings](docs/keybindings.md) · [Architecture](docs/architecture.md) ·
[Roadmap](docs/roadmap.md) · **v0.12.0**

```
┌──────────────────────────────────────────────────────────────────────┐
│ VIBE · src/buffer/mod.rs ●                        2 buffers  v0.12.0 │
│ ▏ storage.rs ▏ mod.rs ●                                              │
│   1 ▏ use std::ops::Range;                                           │
│   2 ▏                                                                │
│   3 ▏ /// A text buffer backed by a rope.                            │
│┌────▏bookmark_2                                                      │
│ └ 4 ▏ pub struct Document {                                          │
│   5 ▏     storage: Box<dyn TextStorage>,                             │
│   6 ▏     cursor: usize,                                             │
│   7 ▏     history: History,                                          │
│   8 ▏ }                                                              │
│     ▏ ~                                                              │
│     ▏ ~                                                              │
│ ● Rust  LF  UTF-8  Spaces: 4                      Ln 4, Col 12   12% │
│ Ctrl+s save   ·   Ctrl+f find   ·   Ctrl+p open file   ·   F1 help   │
└──────────────────────────────────────────────────────────────────────┘
```

## Features

**Editing** — motions, shift-selection, clipboard, undo/redo with coalescing and
save points, duplicate/delete/move lines, comment toggling, bracket matching.

**Indentation** — each file's indentation is read when it opens, so Tab and
Enter match the file rather than a global setting; `Ctrl+Shift+I` changes it for
the buffer.

**Typing** — brackets and quotes complete themselves, a closer is stepped over,
backspace removes both halves of an empty pair, Enter between `{}` opens a
block, and a bracket typed over a selection wraps it. Quotes pair only where a
string can start, so `don't` is left alone.

**Multi-cursor** — stack carets with `Ctrl+Shift+Up`/`Down`, add one per
occurrence with `Ctrl+D`, or place them with `Alt+Click`; one undo step covers
all of them.

**Search** — incremental, with case, whole-word and regex toggles, and
replace-all as one undo step. The first pass answers each keystroke and the rest
of the file is scanned in the background, so every match is reachable in a file
too large to scan per keystroke.

**Files** — tabs; atomic saves that keep permissions and symlinks; UTF-8/UTF-16
with BOM; LF/CRLF/CR written back unchanged; binary files refused. A
two-gigabyte file loads on a worker thread behind a progress panel.

**Nothing gets lost** — unsaved buffers are snapshotted a second after they
change, so a crash does not take them; the next start brings them back, dirty,
at the cursor they had.

**Live changes** — every open file is watched: a clean buffer follows another
program's write and keeps its caret and scroll, while a dirty one is never
overwritten but takes a `⚠ changed on disk` and a save that asks first.

**Clipboard** — the OS pasteboard where there is one, otherwise OSC 52, the only
route that reaches your machine over SSH or tmux; the editor keeps its own copy,
and the message says which happened.

**Panes and comparison** — `Ctrl+\` splits the area over one document, scrolling
independently; `Alt+D` compares two files line by line, with `]`/`[` stepping
through the changes.

**Bookmarks and folds** — double-click a gutter row to name a line, and
`Ctrl+Shift+B` lists them across buffers; `Alt+F` collapses the indentation block
under the cursor, or exactly the lines you selected, keeping its header and
closing brace; `Ctrl+Shift+F` collapses every block at once and `Ctrl+Shift+E`
opens them again. A folded block is one row: arrows step over it, its header
carries a `▸` and a `⋯ n` count, and clicking either opens it. Folds are view
state, never text — not saved, not an undo step, never marking the file
modified — but they follow the text as it is edited.

**Navigation and look** — go to line, fuzzy finder, a palette grouped by
category, help built from the live keymap; hairline chrome, 17 languages, dark
and light themes, indent guides, and word wrap that moves by visual rows.

## Install

```sh
cargo install --path .
```

Requires Rust 1.82 or newer. The editor is a single static binary with no
runtime dependencies.

## Usage

```sh
vibe                    # start with an empty buffer
vibe notes.md           # open a file (created on first save if missing)
vibe a.rs b.rs          # open several files
vibe --print-config     # print the effective configuration as TOML
vibe --list-keys        # print every command and its bindings
vibe --key-report       # explain which shortcuts this terminal takes or merges
```

| Flag | Meaning |
| --- | --- |
| `-c`, `--config <FILE>` | Use a specific configuration file |
| `--ui <pixel\|cell>` | How the chrome is drawn: hairlines or block glyphs |
| `--print-config` | Print the effective configuration and exit |
| `--list-keys` | Print the key bindings and exit |
| `--key-report` | Explain which shortcuts this terminal takes or merges, and exit |
| `--smoke` | Start, draw one frame and exit (startup check for CI) |
| `--no-log` | Do not write a log file |
| `-v`, `--verbose` | Verbose logging |
| `-h`, `--help` | Usage |
| `-V`, `--version` | Version |

## Commands

Every binding is a command with a stable id, remappable in `config.toml` and
reachable from the palette even when unbound. **[docs/commands.md](docs/commands.md)
lists all 69** with their ids and default keys; `vibe --list-keys` prints the
same for your own configuration.

The essentials:

| Key | Action |
| --- | --- |
| `Ctrl+S` / `Ctrl+Shift+S` | Save / Save As |
| `Ctrl+O` / `Ctrl+N` / `Ctrl+W` | Open / New / Close buffer |
| `Ctrl+P` / `Ctrl+Shift+P` | Fuzzy file finder / Command palette |
| `Ctrl+F` / `Ctrl+H` | Find / Replace |
| `F3` / `Shift+F3` | Find next / previous |
| `Ctrl+Z` / `Ctrl+Y` | Undo / Redo |
| `Ctrl+X` / `Ctrl+C` / `Ctrl+V` | Cut / Copy / Paste |
| `Ctrl+D` | Add a cursor at the next occurrence |
| `Ctrl+G` / `Ctrl+Tab` | Go to line / Next buffer |
| `Alt+D` / `Alt+W` / `Alt+F` | Compare file / Word wrap / Fold block |
| `Ctrl+Shift+B` | Bookmark panel |
| `F1` / `F2` | Help / Which keys reach Vibe |

Terminals are not all the same: many deliver `Ctrl+C` as `SIGINT` before the
application sees it, and `Ctrl+Shift+X` / `Ctrl+Tab` need the Kitty keyboard
protocol. Vibe enables the protocol where it can and falls back to unambiguous
alternatives otherwise. When a shortcut does nothing, `F2` says why — it names
the host, the chords that host takes, the ones the wire merges, and echoes each
key as it arrives. See [docs/keybindings.md](docs/keybindings.md).

## Configuration

Configuration lives at `$XDG_CONFIG_HOME/vibe/config.toml`
(`~/.config/vibe/config.toml`), or `~/Library/Application Support/vibe/config.toml`
on macOS. Every key is optional; see [`config.example.toml`](config.example.toml)
for the full set.

```toml
[editor]
tab_width = 4
use_spaces = true
auto_indent = true
syntax_highlighting = true
relative_numbers = false
show_whitespace = false
word_wrap = false

[ui]
theme = "vibe-dark"        # or "vibe-light"
style = "pixel"            # or "cell" — block borders instead of hairlines
show_hints = true
mouse = true

[keys]
"ctrl+shift+p" = "help.commandPalette"
"f5" = "file.reload"
```

Invalid values never abort startup: they are clamped or ignored and reported in
the notification area.

## Performance

Measured on the development machine (Apple silicon, macOS 27, release build,
`cargo bench`). See [docs/performance.md](docs/performance.md) for methodology.

| Operation | Target | Measured |
| --- | --- | --- |
| Cold start (process → first frame) | < 100 ms | **7 ms** |
| Open 1 MiB | < 50 ms | **1.3 ms** |
| Open 100 MiB | < 1 s | **136 ms** |
| Literal search, full scan of 100 MiB | < 500 ms | **23 ms** |
| Typing latency (1 MiB buffer) | imperceptible | **0.3 µs/keystroke** |
| Full frame, 120×24 | < 16 ms | **70 µs** |

## Architecture

```
terminal event → input::translate → Action → App::execute → Document
                                                   ↓
                                              ui::draw (read-only)
```

`App` is the only place where an intent becomes a document mutation, and the
renderer never mutates anything. Text lives in a rope behind a `TextStorage`
trait, so the buffer implementation can be replaced without touching the rest of
the editor. See [docs/architecture.md](docs/architecture.md).

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo bench
```

See [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/extending.md](docs/extending.md)
for how to add commands, key bindings, UI components and buffer backends.

## Status and limitations

Vibe is a working editor, not a finished product. Known limitations are listed
in [docs/roadmap.md](docs/roadmap.md); the important ones:

- Line-oriented commands (duplicate/delete/move line, comment toggle, indent,
  trim whitespace) act on the primary cursor and collapse the others; typing,
  deleting and newlines do apply to every cursor.
- Syntax highlighting is a fast line-oriented lexer, not a full parser: it can
  mis-colour unusual constructs (raw strings, nested comments) but never affects
  the text itself.
- Regex search is not incremental on very large files (it rescans on demand).
- No language server and no git: no diagnostics, completion or go-to-definition,
  and nothing in the gutter says which lines you have changed since the last
  commit. Both want an out-of-process client, which the job channel is ready
  for.

## License

MIT. See [LICENSE](LICENSE).
