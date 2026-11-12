# Roadmap

Ordered by what a user notices first, not by what is easiest.

## Done

- Buffers with tabs, dirty tracking, atomic saves, save-as, save-all, reload
- Character/word/line/document motions, shift-selection, clipboard, undo/redo
- Find, find-next/prev, replace, replace-all, regex, whole word, case sensitivity
- Goto line/column, fuzzy file finder, command palette, help screen
- Line numbers (absolute and relative), syntax highlighting (17 languages),
  dark/light themes, visible whitespace, indent guides, bracket matching
- Auto-indent, comment toggling, duplicate/delete/move lines, trim whitespace
- Multi-cursor: stacked carets, select-next-occurrence, Alt+Click, one undo step
- Split panes: two columns sharing the buffer list, each with its own viewport
- Mouse: click, drag-selection with edge autoscroll, double/triple-click,
  free wheel scrolling with an off-screen cursor indicator, tab switching
- Word wrap, toggled at runtime: rows are the unit of scrolling, motion and
  hit-testing, and a single line can fill the window
- Recent files, session restore, configuration with validation, logging
- Terminal restoration on panic and on `SIGTERM`/`SIGINT`/`SIGHUP`/`SIGQUIT`
- Encoding detection (UTF-8, UTF-16 with BOM), LF/CRLF/CR round-tripping
- External-modification detection at save time
- Crash recovery: unsaved buffers are snapshotted to the data directory and come
  back after a crash, with the cursor; a clean exit discards the snapshot
- Auto-closing brackets and quotes: pairs complete themselves, a closer under
  the caret is stepped over, backspace removes an empty pair whole, and Enter
  opens a bracket pair onto three lines
- Indent detection: each file's own indentation (tabs or spaces, and how many)
  is read at load and used by Tab, Shift+Tab, Enter and the pair expansion
- Live external-change detection: every open file is watched through its parent
  directory, so a clean buffer follows the file and a dirty one is flagged
  instead of being overwritten
- Clipboard routing: the OS pasteboard where there is one, OSC 52 where there is
  not (SSH, tmux, containers), and an in-editor copy either way
- Code folding: an indentation-based block collapses to one row behind a `▸` in
  the gutter, with a `⋯ n` count where the text stopped. Folds are view state —
  never saved, never an undo step, never dirtying the buffer — but they follow
  the text as it is edited
- Unit, integration, UI and property tests; Criterion benchmarks

## Next

### 1. Line transforms

Sort, dedupe, join, upper/lower, and the rest of the operations that are one
keystroke in a shell and a hand-rolled regex here. Each is an `apply_batch`
call, so it is one undo step already; what is missing is the commands and a menu
to reach them from.

### 2. Incremental regex search on large files

Regex currently rescans the whole buffer on each change. Chunked scanning with a
resumable matcher would keep the same responsiveness as literal search.

## Later

- **LSP client**: diagnostics, completion, go-to-definition. Large surface; the
  architecture (typed errors, background jobs, generation counters) is ready for
  it.
- **Plugin system**: an out-of-process plugin protocol beats in-process
  scripting for crash isolation; `Command` would need a dynamic variant.
- **Git integration**: gutter markers and hunk staging, out of process.
- **Terminal image protocols**: not planned; the editor is text-first.
- **Splits in a tabbed layout, minimap**: nice, not scheduled.

## Explicitly not planned

- Modal editing (Vim/Emacs key layers). Vibe's premise is conventional shortcuts;
  a modal mode would double the input surface and dilute the design.
- A GUI front end. The terminal is the product.
- Anything that requires an embedded scripting runtime in the editor core: it
  conflicts with the startup-time and crash-isolation goals.
