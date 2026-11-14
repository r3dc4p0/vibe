# Commands

Every binding is a *command*, and every command has a stable id — the same id
in the command palette (`Ctrl+Shift+P`), in `config.toml`, and in `--list-keys`.
A command you never bind is still reachable: open the palette and type its name.

This page lists all 70 commands in the editor, grouped the way the palette
groups them, with the keys each one has by default.

```sh
vibe --list-keys     # the table for your own configuration
```

Bindings are shown as Vibe writes them. Terminals differ in which chords they
can deliver at all — see [Terminal limitations](keybindings.md#terminal-limitations-read-this)
before concluding that a key does nothing.

| Section | Commands |
| --- | --- |
| [Files](#files) | 13 |
| [Editing](#editing) | 20 |
| [Navigation](#navigation) | 3 |
| [Search](#search) | 7 |
| [View and appearance](#view-and-appearance) | 24 |
| [Help](#help) | 3 |

## Files

Buffers, saving and comparing. Saves are atomic and keep the file's mode and symlinks.

| Command | id | Default keys |
| --- | --- | --- |
| New Buffer | `file.new` | `Ctrl+N` |
| Open File… | `file.open` | `Ctrl+O` |
| Quick Open File… | `file.quickOpen` | `Ctrl+P` |
| Save | `file.save` | `Ctrl+S` |
| Save As… | `file.saveAs` | `Ctrl+Shift+S` |
| Save All | `file.saveAll` | — |
| Revert File from Disk | `file.reload` | `Ctrl+Shift+R` |
| Close Buffer | `file.close` | `Ctrl+W` |
| Next Buffer | `buffer.next` | `Ctrl+Tab` |
| Previous Buffer | `buffer.prev` | `Ctrl+Shift+Tab` |
| Quit | `app.quit` | `Ctrl+Q` |
| Quit All | `app.quitAll` | `Ctrl+Shift+Q` |
| Compare with File… | `file.compareWith` | `Alt+D` |

## Editing

Everything that changes text, and the carets that change it.

| Command | id | Default keys |
| --- | --- | --- |
| Undo | `edit.undo` | `Ctrl+Z` |
| Redo | `edit.redo` | `Ctrl+Y`, `Ctrl+Shift+Z` |
| Cut | `edit.cut` | `Ctrl+X` |
| Copy | `edit.copy` | `Ctrl+C`, `Ctrl+Shift+C` |
| Paste | `edit.paste` | `Ctrl+V` |
| Select All | `edit.selectAll` | `Ctrl+A` |
| Duplicate Line | `edit.duplicateLine` | `Ctrl+Shift+D` |
| Delete Line | `edit.deleteLine` | `Ctrl+Shift+K` |
| Move Line Up | `edit.moveLineUp` | `Alt+Up` |
| Move Line Down | `edit.moveLineDown` | `Alt+Down` |
| Toggle Comment | `edit.toggleComment` | `Ctrl+/` |
| Indent | `edit.indent` | — |
| Outdent | `edit.dedent` | — |
| Trim Trailing Whitespace | `edit.trimTrailingWhitespace` | — |
| Add Cursor Above | `edit.addCursorAbove` | `Ctrl+Shift+Up`, `Ctrl+Alt+Up` |
| Add Cursor Below | `edit.addCursorBelow` | `Ctrl+Shift+Down`, `Ctrl+Alt+Down` |
| Add Cursor at Next Occurrence | `edit.addCursorToNext` | `Ctrl+D` |
| Collapse to One Cursor | `edit.collapseCursors` | — |
| Set Indentation… | `edit.setIndentation` | `Ctrl+Shift+I` |
| Toggle Auto-Close Pairs | `edit.toggleAutoPairs` | `Ctrl+Shift+A` |

## Navigation

Jumps. A jump that names a line inside a collapsed fold opens the fold.

| Command | id | Default keys |
| --- | --- | --- |
| Go to Line… | `nav.gotoLine` | `Ctrl+G` |
| Go to Start of Buffer | `nav.bufferStart` | `Ctrl+Home` |
| Go to End of Buffer | `nav.bufferEnd` | `Ctrl+End` |

## Search

Find and replace. Every jump a search makes opens the fold it lands in.

| Command | id | Default keys |
| --- | --- | --- |
| Find… | `search.find` | `Ctrl+F` |
| Find and Replace… | `search.replace` | `Ctrl+H` |
| Find Next | `search.findNext` | `F3` |
| Find Previous | `search.findPrev` | `Shift+F3` |
| Replace | `search.replaceOne` | — |
| Replace All | `search.replaceAll` | — |
| Clear Search Highlights | `search.clear` | — |

## View and appearance

What is on screen rather than what is in the file — none of these mark a buffer modified.

| Command | id | Default keys |
| --- | --- | --- |
| Close Diff View | `view.closeDiff` | — |
| Next Change | `view.nextChange` | — |
| Previous Change | `view.prevChange` | — |
| Split Pane Right | `view.splitPane` | `Ctrl+\` |
| Close Pane | `view.closePane` | `Ctrl+Alt+W` |
| Focus Pane Left | `view.focusPaneLeft` | `Ctrl+Alt+Left` |
| Focus Pane Right | `view.focusPaneRight` | `Ctrl+Alt+Right` |
| Toggle Dark/Light Theme | `view.toggleTheme` | `Ctrl+Shift+T` |
| Toggle Pixel/Cell UI Style | `view.toggleUiStyle` | `Ctrl+Shift+U` |
| Toggle Line Numbers | `view.toggleLineNumbers` | — |
| Toggle Relative Line Numbers | `view.toggleRelativeNumbers` | — |
| Toggle Visible Whitespace | `view.toggleWhitespace` | `Alt+Z` |
| Toggle Indent Guides | `view.toggleIndentGuides` | `Ctrl+Shift+G` |
| Toggle Syntax Highlighting | `view.toggleSyntax` | — |
| Toggle Shortcut Hints | `view.toggleHints` | — |
| Toggle Cursor Line Highlight | `view.toggleCursorLine` | — |
| Toggle Word Wrap | `view.toggleWordWrap` | `Alt+W` |
| Toggle Fold | `view.toggleFold` | `Alt+F` |
| Fold All | `view.foldAll` | `Ctrl+Shift+F` |
| Unfold All | `view.unfoldAll` | `Ctrl+Shift+E` |
| Toggle Bookmark Panel | `view.toggleBookmarkPanel` | `Ctrl+Shift+B` |
| Focus Bookmark Panel | `view.focusBookmarkPanel` | `Ctrl+Alt+B` |
| Word Count | `view.wordCount` | `Ctrl+Shift+W` |
| Document Statistics | `view.stats` | — |

## Help

Generated from the live keymap, so they are never stale.

| Command | id | Default keys |
| --- | --- | --- |
| Keyboard Shortcuts | `help.show` | `F1` |
| Which Keys Reach Vibe | `help.keyReport` | `F2` |
| Command Palette… | `help.commandPalette` | `Ctrl+Shift+P` |

## Commands with no default binding

A command marked `—` has no entry in the keymap; it is not unreachable. Several
of the 17 are already reached from where they make sense — `Tab` and
`Shift+Tab` indent and outdent without a chord, Replace and Replace All live in
the replace bar, and `]`/`[` step through a diff only while the diff is open,
because binding them globally would take away two characters you type. The rest
are simply rare. Any of them can be given a key with the id from the table
above.

## Rebinding

Put the command id on the left and the chord on the right, in `[keys]`:

```toml
[keys]
"ctrl+shift+p" = "help.commandPalette"   # an id from this page
"f5" = "file.reload"
"ctrl+alt+t" = "view.toggleTheme"
```

A chord may be bound to one command only; binding it again moves it. Chords are
written lowercase with `+`, and `F1`–`F12` as `f1`–`f12`. An id that names no
command is reported in the notification area at startup rather than silently
ignored. Run `vibe --list-keys` to see the result.

