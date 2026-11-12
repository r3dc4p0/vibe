# Bug Report — for the resolving agent

> **VERIFICATION 2026-11-08 (post-fix):** all items below have been fixed and
> re-verified. `cargo check` / `clippy -D warnings` / `fmt --check` clean;
> **639 tests pass, 0 fail** (new regression tests included: whole-word
> truncation ×2, regex backreference ×2, prompt-vs-pane modality, prompt save
> refusal). Details per bug are marked ✅ / 📌.
> - Bug 1 ✅ — `find_in`/`find_regex` now filter whole-word *before* the
>   budget and return a `Scan { ranges, truncated }`; `truncated` is
>   `raw_capped || overflowed`.
> - Bug 2 ✅ — results are gated by a `Ticket { generation, doc, revision }`;
>   `Ticket::describes` requires same document id **and** revision, plus the
>   generation check inside `SearchState::install`.
> - Bug 3 📌 — resolved by pinning the invariant rather than changing code:
>   a prompt is application-modal (there is no per-pane input mode to
>   restore, because no pane command can run while an overlay is up). The new
>   test `a_prompt_keeps_the_keyboard_away_from_both_panes` proves focus
>   cannot move and text cannot leak into the document while a prompt is open.
> - Bug 4 ✅ — `App::run` now draws before polling each event and never
>   repaints in between; the status bar reads `active_pane` in the same pass.
>   Comments in both places record why.
> - Bug 5 ✅ — `prompt_refuses_save` warns
>   "Save is unavailable while a prompt is open — press Esc first" and keeps
>   the prompt and its text intact (test: `save_from_inside_a_prompt_says_why_nothing_happened`).
> - Regex bonus ✅ — `expand_replacement` takes the whole `Query` and compiles
>   the pattern with the search's flags (case-insensitivity included).
> - Housekeeping ✅ — README stale claim removed; `abc.c`, `abc.txt`,
>   `mm.txt`, `test.txt` and empty `tools/` deleted from the repo root.
>
> The rest of this document is the original report, kept for context.

Generated 2026-11-08 from a full-project analysis (v0.8.0, commit `cd6aff9`).
The project builds clean (`cargo check`, `cargo clippy -D warnings`, `cargo fmt`
--check all pass) and all 282 tests pass, so none of these are compile or test
failures — each is a logic defect found by code review and hand-simulation of
the exact algorithms. Verify each fix against the reproduction steps below, and
add a regression test per bug (the file to put it in is suggested).

Work through the list in order: severity descending, and bug 1 is the only
user-visible data-corruption risk.

---

## Bug 1 — Whole-word search truncates results (HIGH)

**File:** `src/search.rs`, lines 186–203 (`find_in`)

**Defect.** For a whole-word literal query, the raw scan collects up to
`limit * 4` starts, then the ranges are truncated to `limit` **before** the
whole-word filter runs:

```rust
let starts = source.find_literal(&query.text, query.case_sensitive, scan_limit);
let len = query.text.chars().count();
starts.into_iter().map(|s| s..s + len).take(limit).collect()   // take() first
// ...
Ok(ranges.into_iter().filter(|r| is_whole_word(source, r)).collect())  // filter second
```

Because `take(limit)` is applied before `filter(is_whole_word)`, only the
first `limit` raw hits are ever considered. If most of them are sub-word hits
(e.g. searching `foo` in a file whose first 20,000 occurrences are all
`foobar`), the filtered result comes back empty or short even though bare
matches exist later in the file. The `truncated` flag (computed downstream,
`src/search.rs:408`, on the already-filtered list) is then `false`, so no
committed full scan ever runs to correct it — the user silently sees "no
matches". The `* 4` overshoot also only ever helps while sub-word hits stay
under 75% of the stream, for the same reason.

**Fix.** Filter whole-word first, then take `limit`. `is_whole_word` needs the
source, so map/filter before collecting:

```rust
let ranges: Vec<Range<usize>> = if query.whole_word {
    starts.into_iter()
        .map(|s| s..s + len)
        .filter(|r| is_whole_word(source, &r))
        .take(limit)
        .collect()
} else {
    starts.into_iter().map(|s| s..s + len).take(limit).collect()
};
```

Then base `truncated` on the **raw** scan length (hits seen by
`find_literal`), not on the filtered list length, so a filtered-down result
that may have more matches still reports "more available" and triggers the
full scan. Check how `SearchState` stores/computes `truncated`
(`src/search.rs` around line 408) and thread the raw count through if needed.

**Regression test.** `tests/search_large.rs`: a file with >`limit`
occurrences of `foobar` followed by one bare `foo` (whole-word query,
`limit` small — check the constant used by the live search path) must find
the bare `foo` and/or set `truncated`.

---

## Bug 2 — Incremental search can install stale matches (MEDIUM)

**File:** `src/search.rs`, around line 371 (`SearchState::refresh` / the
background-job result acceptance path)

**Defect.** The guard that decides whether a completed background scan result
is still relevant compares the document revision with `<=` instead of `<`. A
result computed at the *same* revision as a newer in-flight or just-installed
live result can overwrite fresher matches. Symptom: while typing quickly in
the find bar over a large file, the displayed match set can momentarily (or
permanently, if no further event arrives) reflect an older query pass.

**Fix.** Compare strictly (`<`), or record the revision at which the job was
*started* and reject any result whose revision is not exactly the current one
while the generation counter has moved on. Read the surrounding generation /
revision bookkeeping (`bg` job plumbing, `generation` fields in `App`) before
changing it — the intent is "a result may only be installed if no newer edit
or newer job superseded it".

**Regression test.** Unit test in `src/search.rs` (module tests): start a
job, bump the revision, deliver the old result, assert it is rejected.

---

## Bug 3 — Pane-selection restore stays in Insert mode (MEDIUM)

**File:** `src/app/mod.rs`, around line 432 (the prompt restore path used when
a pane command dismisses a prompt into a different pane than opened it)

**Defect.** When a text prompt (find bar, save-as, goto) is opened from one
pane and the resolution moves focus to another pane, the saved input-mode
snapshot is restored onto the new pane without resetting the mode to the
pane's own pre-prompt state. Verified by simulation: after the restore the
editor remains in Insert mode, so characters typed at the restored prompt
go into the document instead of the prompt (or vice versa — whichever
direction the mode was captured in).

**Fix.** Save and restore input mode **per pane**, not as a single editor-wide
field — or explicitly reset the input mode to the target pane's saved
pre-prompt mode (the same `mem::replace` pattern used by the normal prompt
open/close path, but keyed by pane id). Find the prompt state struct in
`src/app/mod.rs` (it holds the displaced `InputMode`) and make the pane
switch path go through the same restore helper used when the prompt closes
normally.

**Regression test.** `tests/split.rs`: open a split, open the find prompt in
pane A, run a pane-focus command, dismiss the prompt, assert the input mode
in the focused pane is `Normal` (or the pane's original mode) and that typed
characters do not reach the document while the prompt is open.

---

## Bug 4 — Pane status bar can render before focus settles (LOW, cosmetic)

**File:** `src/ui/mod.rs` (pane status-bar segment construction)

**Defect.** The per-pane status segment is built from the pane state at
layout time; when a focus change happens in the same frame as the pane
command, the bar can be painted with the previous focus before the next
event loop iteration corrects it. One-frame flicker only; no state corruption.

**Fix.** Either draw the status bar from the same resolved pane-order/focus
snapshot the viewport uses (single source of truth), or defer the status
paint until after `App::execute` completes — it may already do this; confirm
the render call order in `src/app/mod.rs` (event loop) before changing
anything. If the one-frame window cannot actually occur because drawing
happens strictly after execution, close this as not-a-bug with a comment.

---

## Bug 5 — `Ctrl+S` inside a text prompt silently discards typed characters (LOW, UX)

**File:** `src/app/prompts.rs` (prompt key handling / the `Action` it maps to)

**Defect.** While a text prompt is open, pressing the save shortcut does not
run save (the prompt consumes the event) and does not surface any message —
whatever the user typed into the prompt stays, but the intent (save) is
silently dropped. Users of the shortcut habit will type into the find bar,
hit `Ctrl+S` out of habit, and get no feedback.

**Fix.** Pick one behaviour and make it visible:
- simplest: when a `Save`-family action arrives while a prompt is open, close
  the prompt (keeping its text, as `Esc` would) and report via the existing
  message line ("save is unavailable while a prompt is open"), or
- run the save as a background command without closing the prompt.

Do not change what `Ctrl+S` maps to in `src/input.rs` / `src/keymap.rs` while
a prompt owns the input — the prompt layer's action filter is the right place
(see how unknown actions are already ignored with a status message there).

**Regression test.** `tests/hints.rs` or `tests/workflow.rs`: open a prompt,
type text, send the save key, assert the prompt text is intact and a message
was set (or the save ran, per the chosen behaviour).

---

## Housekeeping (not bugs — do while in the repo)

- `README.md` "Status and limitations" still says **"No split views yet."**
  Splits shipped in v0.7.0 (`tests/split.rs`, `Command::SplitPane`). Delete
  that bullet.
- Untracked scratch files in the repo root: `abc.c`, `abc.txt`, `mm.txt`,
  `test.txt`, and an empty `tools/` directory — remove them (do **not** commit
  them). `logo.svg` at the root is untracked: check whether the README/site
  references it; commit it if referenced, otherwise remove.
- The regex-replacement expansion (`src/search.rs:286`, `expand_replacement`)
  rebuilds the pattern with only `.multi_line(true)` and drops
  `case_insensitive`, so a case-insensitive regex replace using a backreference
  inserts the literal replacement text (e.g. pattern `(hello)`, replacement
  `<$1>`, text `HELLO` → inserts `<$1>` instead of `<HELLO>`). Reproduce, then
  mirror the flags used by `find_regex` (line ~224) when compiling the pattern
  there. Regression test in `tests/features.rs`.

## Verification checklist for the whole batch

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test          # 282 existing tests must still pass, plus the new ones
```

Note for the environment: stale `target/debug/build/*/build-script-build`
artifacts carrying the `com.apple.provenance` xattr can make cargo fail with
"failed to remove file … Operation not permitted". If that happens, build with
`CARGO_TARGET_DIR=/tmp/vibe-target` instead of deleting files.

---

# Fold feature review (v0.11.0, commit 87e2b0f) — 2026-11-12

> **FIXED 2026-11-12:** all four items below are fixed and re-verified —
> check/clippy/fmt clean, **705 tests pass, 0 fail**, including five new
> regression tests in `tests/folding.rs` (search-jump reveal, undo+redo fold
> round-trip, header-line deletion ×2, stacked-caret settling).
> - F1 ✅ — `select_range` reveals the fold at both ends of the jump.
> - F2 ✅ — undo/redo replay each change's `(line, col, ±delta)` into the
>   folds via `refold_after_replay`; single-change transactions shift exactly,
>   multi-change ones clear (batch policy). Undo is the exact inverse of the
>   forward shift, verified by test.
> - F3 ✅ — `settle_extra_carets` runs wherever folds change; stacked carets
>   settle through `settle_caret_position` in `add_cursor_vertical` and in
>   `move_cursor`.
> - F4 ✅ — two guards: a content-bearing removal at column zero of a fold
>   header drops the fold (its text went with it), and `revalidate_headers`
>   re-checks every fold whose header the edit touched, dropping those that
>   no longer open a block.

Review of `87e2b0f "Fold blocks of code, and keep the folds out of the file"`.
Model (`src/buffer/folds.rs`) verified sound: `shift`'s moved/grown/untouched
categorisation preserves the disjoint invariant in every branch. The bugs are
in the integration. Gates at review time: check/clippy/fmt clean, **700 tests
pass, 0 fail** — none of these are caught by the suite.

## F1 — Search jumps do not open the fold they land in (HIGH)

**File:** `src/buffer/mod.rs:919` (`select_range`), called from
`src/app/prompts.rs:317`, `:389`, `:416` (find-next, replace jumps, match
selection).

The commit message promises "a jump that names one — a bookmark, a search
match, go-to-line — opens the fold". Only bookmarks and go-to-line do: they go
through `set_cursor`, which calls `reveal_cursor`. The search path goes through
`select_range`, which sets `anchor`/`cursor` directly and never reveals. A
find-next that lands on a match inside a collapsed fold moves the match counter
and puts the caret on a line with display height zero: nothing on screen moves,
the caret is invisible, and the user is left on a screen that does not show
where the search went.

**Fix.** Call `self.reveal_cursor()` in `select_range` after setting the
cursor (it is a jump, and a jump into a fold means "show me this"), or route it
through `set_cursor_extending`. Add a regression test in `tests/folding.rs`:
fold the block, `Ctrl+F` for a string that only occurs inside it, press `F3`,
assert `!doc.is_line_hidden(match_line)`.

## F2 — Undo and redo leave folds pointing at the wrong lines (MEDIUM)

**File:** `src/buffer/mod.rs:1402` (`undo`), `:1431` (`redo`).

Folds shift forward on an edit (`refold_after`), but undo and redo replay
their changes directly against storage and only call `prune_folds`, which
drops folds past the end of the buffer but cannot detect a fold that is
*in range and wrong*. Proven by a live probe (test written, run, failed, then
removed): fold at `(0, 2)`, Enter at column zero of the header shifts it to
`(1, 3)`, `Ctrl+Z` restores the text but the fold stays at `(1, 3)` — line 1
(`one();`) is now a bogus fold header hiding lines 2–3, including the `}`
that says where the block ends. Symmetric drift happens on redo.

**Fix.** `undo`/`redo` know the net line delta of the transaction they replay
(the sign is opposite to the forward edit). Either apply `Folds::shift` per
replayed change in reverse order, or — simpler and matching the existing
"cannot say where its breaks landed" policy for batches — clear the folds in
`undo`/`redo` when the transaction changed the line count. Prefer shifting
when the transaction has exactly one change with a known `(line, col)`.

## F3 — Extra carets ignore folds (LOW)

**File:** `src/buffer/mod.rs:797` (`set_cursor_extending` and the multi-caret
plumbing in `app/mod.rs`).

A multi-cursor motion can park a secondary caret on a hidden line; it keeps
editing there (typed text lands in an invisible region) until something
settles the carets. `settle_cursor` handles only the primary cursor.

**Fix.** After any motion that can move extra carets, run the same
hidden-line check over `self.extra` and settle each one the way
`settle_cursor` does.

## F4 — Deleting the header line leaves the fold on the next line (LOW)

**File:** `src/buffer/mod.rs` (`refold_after`), the `col == 0` rule.

A backspace at column zero of line *n* is an edit reported at `(n, 0)` with
negative delta, which the `col == 0` rule reads as "the header itself moved
up" — so a fold headed at *n* shifts to *n−1* and now heads the line above
(the one the deletion joined into). The fold survives pointing at a line that
is not a block opener; its marker shows, and Alt+F on it unfolds something
the user never folded.

**Fix.** After the shift, verify each moved fold still opens a block
(`block_end` on its header is `Some` and reaches the recorded end, or at
least is deeper than the header); drop the ones that do not. Cheaper
alternative: when the edit is a removal whose join lands on the header line,
drop folds whose header equals the edit line.

## Solid areas (checked, no issues)

`Folds::shift` disjointness (all three branches); fold absorption in
`insert`; folds cleared on reload (`reset_contents`) so the watcher cannot
leave stale folds; per-document fold state shared by two panes (deliberate,
consistent); wrap+folding heights (`LayoutCache::with_folds`); the "..."
summary with count; folding is not dirty/undoable; the last-screen
reachability edge case (tested).
