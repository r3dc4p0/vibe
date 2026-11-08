//! Rendering and UI state.
//!
//! The UI owns presentation state (which overlay is open, prompt contents,
//! transient messages) and reads application state. It never mutates a document
//! except through the documented `scroll` fields of the active buffer, which are
//! pure view state.

pub mod chrome;
pub mod diff;
pub mod editor;
pub mod input;
pub mod loading;
pub mod overlays;

use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::Frame;

use crate::app::App;
use crate::commands::Command;
use crate::theme::Theme;

pub use chrome::OverlayKind;
pub use input::TextInput;

/// Severity of a transient message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Success,
    Warn,
    Error,
}

/// A transient status message.
#[derive(Debug, Clone)]
pub struct Message {
    pub text: String,
    pub level: Level,
    pub born: Instant,
}

impl Message {
    pub fn new(text: impl Into<String>, level: Level) -> Self {
        Self {
            text: text.into(),
            level,
            born: Instant::now(),
        }
    }
}

/// Which picker a [`Palette`] is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteMode {
    Commands,
    Files,
    /// Pick a file to compare with the buffer in front of you.
    ///
    /// The same picker as `Files`, because "which file" is the same question;
    /// only what happens to the answer differs.
    Compare,
}

/// What selecting a palette entry does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteAction {
    Run(Command),
    OpenFile(std::path::PathBuf),
    CompareFile(std::path::PathBuf),
}

/// One row in the palette.
///
/// A group heading is a row of its own rather than a flag on an entry, so the
/// type says what the code means: a heading has no action, and nothing can run
/// one by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteItem {
    /// Something to run or open.
    Entry {
        label: String,
        detail: String,
        positions: Vec<usize>,
        action: PaletteAction,
    },
    /// A separator with a group's name in the middle of it.
    Group(String),
}

impl PaletteItem {
    pub fn entry(
        label: impl Into<String>,
        detail: impl Into<String>,
        positions: Vec<usize>,
        action: PaletteAction,
    ) -> Self {
        PaletteItem::Entry {
            label: label.into(),
            detail: detail.into(),
            positions,
            action,
        }
    }

    /// True for a heading, which is drawn but never selected.
    pub fn is_group(&self) -> bool {
        matches!(self, PaletteItem::Group(_))
    }

    /// What selecting this row runs.
    pub fn action(&self) -> Option<&PaletteAction> {
        match self {
            PaletteItem::Entry { action, .. } => Some(action),
            PaletteItem::Group(_) => None,
        }
    }
}

/// Command palette / file finder state.
#[derive(Debug, Clone)]
pub struct Palette {
    pub mode: PaletteMode,
    pub input: TextInput,
    pub items: Vec<PaletteItem>,
    pub selected: usize,
    pub scroll: usize,
}

impl Palette {
    /// Put the selection on the first row that can be chosen.
    ///
    /// A list that starts with a heading would otherwise open with a separator
    /// highlighted, which cannot be run and looks like a bug.
    pub fn select_first_entry(&mut self) {
        self.selected = self
            .items
            .iter()
            .position(|item| !item.is_group())
            .unwrap_or(0);
        self.scroll = 0;
    }

    /// Move the selection by `delta` rows, stepping over headings.
    pub fn step(&mut self, delta: isize) {
        if self.items.is_empty() {
            return;
        }
        let mut at = self.selected as isize;
        loop {
            at += delta;
            if at < 0 || at as usize >= self.items.len() {
                return;
            }
            if !self.items[at as usize].is_group() {
                self.selected = at as usize;
                return;
            }
        }
    }

    /// The first entry of every group, in order.
    ///
    /// An entry that comes before any heading belongs to no group, so it is not
    /// one of these: there is nothing to jump to.
    pub fn group_starts(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut after_heading = false;
        for (index, item) in self.items.iter().enumerate() {
            match item {
                PaletteItem::Group(_) => after_heading = true,
                PaletteItem::Entry { .. } => {
                    if after_heading {
                        out.push(index);
                        after_heading = false;
                    }
                }
            }
        }
        out
    }

    /// Jump to the first entry of the next or previous group.
    ///
    /// Both directions leave the group the selection is in, even when it is
    /// already at that group's first entry: Left from the middle of Edit goes
    /// to the top of File, not to the top of Edit. That keeps the two keys
    /// symmetric — each crosses exactly one heading — and leaves "the start of
    /// this section" to `Home`, if it is ever wanted.
    ///
    /// Stops at the ends rather than wrapping: the last group is the last
    /// group, and a Right that silently returned to the top of the menu would
    /// make "am I at the end?" unanswerable.
    pub fn jump_group(&mut self, forward: bool) {
        let starts = self.group_starts();
        let target = if forward {
            starts.iter().copied().find(|&start| start > self.selected)
        } else {
            // The group the selection is in, then the one before it.
            starts
                .iter()
                .rposition(|&start| start <= self.selected)
                .and_then(|here| here.checked_sub(1))
                .map(|before| starts[before])
        };
        if let Some(target) = target {
            self.selected = target;
        }
    }

    /// Rows that can be chosen, and how many there are.
    pub fn entry_position(&self) -> (usize, usize) {
        let total = self.items.iter().filter(|i| !i.is_group()).count();
        let here = self
            .items
            .iter()
            .take(self.selected + 1)
            .filter(|i| !i.is_group())
            .count();
        (here, total)
    }

    pub fn new(mode: PaletteMode) -> Self {
        Self {
            mode,
            input: TextInput::new(),
            items: Vec::new(),
            selected: 0,
            scroll: 0,
        }
    }
}

/// Incremental find bar.
#[derive(Debug, Clone)]
pub struct FindBar {
    pub input: TextInput,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

impl FindBar {
    pub fn new() -> Self {
        Self {
            input: TextInput::new(),
            case_sensitive: false,
            whole_word: false,
            regex: false,
        }
    }
}

impl Default for FindBar {
    fn default() -> Self {
        Self::new()
    }
}

/// Find and replace bar.
#[derive(Debug, Clone)]
pub struct ReplaceBar {
    pub input: TextInput,
    pub replace: TextInput,
    pub focus_replace: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

impl ReplaceBar {
    pub fn new() -> Self {
        Self {
            input: TextInput::new(),
            replace: TextInput::new(),
            focus_replace: false,
            case_sensitive: false,
            whole_word: false,
            regex: false,
        }
    }
}

impl Default for ReplaceBar {
    fn default() -> Self {
        Self::new()
    }
}

/// Path prompt with filesystem completion.
#[derive(Debug, Clone)]
pub struct PathPrompt {
    pub input: TextInput,
    pub completion: Vec<String>,
    pub selected: usize,
}

impl PathPrompt {
    pub fn new(initial: impl Into<String>) -> Self {
        Self {
            input: TextInput::with(initial),
            completion: Vec::new(),
            selected: 0,
        }
    }
}

/// What confirming a dialog will do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    Quit,
    CloseBuffer(usize),
    Reload,
    OverwriteExternal,
    /// Pick an indentation from a menu; the choice is the item index.
    SetIndent,
}

/// A modal yes/no/… dialog.
#[derive(Debug, Clone)]
pub struct Confirm {
    /// Panel title: what this dialog is about.
    pub title: String,
    pub prompt: String,
    pub items: Vec<String>,
    pub selected: usize,
    pub action: ConfirmAction,
}

/// Help overlay scroll state.
#[derive(Debug, Clone, Default)]
pub struct HelpState {
    pub scroll: usize,
}

/// Key report state: where the list is scrolled, and the last key that arrived.
///
/// The last key is the whole point. Everything else in the report is inference
/// from the environment; this is the one measurement — press the shortcut that
/// does nothing and read what the terminal actually sent, or see that it sent
/// nothing at all.
#[derive(Debug, Clone, Default)]
pub struct KeyReportState {
    pub scroll: usize,
    pub last: Option<crossterm::event::KeyEvent>,
    /// What the last key does, filled in as keys arrive.
    pub note: String,
    /// The report, built once when the panel opens.
    ///
    /// It depends on the keymap, the environment and the keyboard protocol —
    /// none of which change while the panel is up — so drawing reads it rather
    /// than rebuilding it sixty times a second.
    pub lines: Vec<String>,
}

/// The single overlay slot: at most one modal UI is open at a time.
#[derive(Debug, Clone)]
pub enum Overlay {
    None,
    Palette(Palette),
    Find(FindBar),
    Replace(ReplaceBar),
    GotoLine(TextInput),
    Indent(TextInput),
    Open(PathPrompt),
    SaveAs(PathPrompt),
    Confirm(Confirm),
    Help(HelpState),
    KeyReport(KeyReportState),
}

impl Overlay {
    pub fn kind(&self) -> OverlayKind {
        match self {
            Overlay::None => OverlayKind::None,
            Overlay::Palette(_) => OverlayKind::Palette,
            Overlay::Find(_) => OverlayKind::Find,
            Overlay::Replace(_) => OverlayKind::Replace,
            Overlay::GotoLine(_) => OverlayKind::GotoLine,
            Overlay::Indent(_) => OverlayKind::Indent,
            Overlay::Open(_) => OverlayKind::Open,
            Overlay::SaveAs(_) => OverlayKind::SaveAs,
            Overlay::Confirm(_) => OverlayKind::Confirm,
            Overlay::Help(_) => OverlayKind::Help,
            Overlay::KeyReport(_) => OverlayKind::KeyReport,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Overlay::None)
    }

    /// True when the editor must not receive text input.
    pub fn captures_input(&self) -> bool {
        !self.is_none()
    }
}

/// A clickable stretch of the hint bar.
///
/// The bar is drawn as a row of shortcuts, and each one is a button: the draw
/// pass publishes where it put them, and the mouse handler looks the click up
/// here. Publishing from the draw pass rather than computing the layout twice
/// is the same rule the tab strip and the panes follow — one layout, one source
/// of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HintHit {
    pub row: u16,
    /// First column, inclusive.
    pub start: u16,
    /// Last column, exclusive.
    pub end: u16,
    pub command: Command,
}

impl HintHit {
    pub fn contains(&self, column: u16, row: u16) -> bool {
        row == self.row && column >= self.start && column < self.end
    }
}

/// All presentation state.
#[derive(Debug)]
pub struct UiState {
    pub overlay: Overlay,
    pub bookmarks: BookmarkPanel,
    pub messages: Vec<Message>,
    /// Last message shown in the status bar (kept after the toast expires).
    pub status: Option<Message>,
    /// Where the hint bar's buttons ended up in the last frame.
    ///
    /// Empty when the bar is hidden, so a click on the row it would have
    /// occupied cannot press a button that is not there.
    pub hint_hits: Vec<HintHit>,
    /// The hint under the pointer, so the bar reads as something pressable.
    pub hovered_hint: Option<usize>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            overlay: Overlay::None,
            bookmarks: BookmarkPanel::default(),
            messages: Vec::new(),
            status: None,
            hint_hits: Vec::new(),
            hovered_hint: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct BookmarkPanel {
    pub open: bool,
    pub focused: bool,
    pub selected: Option<(usize, usize)>,
    pub caption_cursor: usize,
    pub scroll: usize,
    pub replace_default_caption_on_type: bool,
    pub area: Rect,
    pub hits: Vec<BookmarkHit>,
}

#[derive(Debug, Clone, Copy)]
pub struct BookmarkHit {
    pub area: Rect,
    pub document_id: usize,
    pub line: usize,
    pub caption_x: u16,
}

impl UiState {
    /// Messages that are still within their display window.
    pub fn visible_messages(&self) -> Vec<&Message> {
        let now = Instant::now();
        self.messages
            .iter()
            .filter(|m| now.duration_since(m.born).as_millis() < 4_000)
            .collect()
    }

    /// Drop expired messages and cap the queue.
    pub fn prune(&mut self) {
        let now = Instant::now();
        self.messages
            .retain(|m| now.duration_since(m.born).as_millis() < 8_000);
        while self.messages.len() > 16 {
            self.messages.remove(0);
        }
    }
}

/// Compute the row rectangles for the whole window.
struct Layout {
    topbar: Option<Rect>,
    /// The whole editing area, panes and dividers together.
    editor: Rect,
    tabs: Option<Rect>,
    /// One rect per pane, in pane order. Always `app.pane_count()` long, so an
    /// index into it is an index into `app.panes`; a pane too narrow to show
    /// gets a zero-width rect rather than disappearing from the list.
    panes: Vec<Rect>,
    bookmarks: Option<Rect>,
    status: Option<Rect>,
    hints: Option<Rect>,
}

/// Divide the editing area into `count` columns, one column of divider between
/// each pair.
///
/// The remainder of the division is handed out one column at a time to the
/// leftmost panes, so two panes of an odd width differ by one column instead of
/// leaving a gap at the right edge.
fn split_columns(area: Rect, count: usize) -> Vec<Rect> {
    if count <= 1 {
        return vec![area];
    }
    let dividers = count as u16 - 1;
    let usable = area.width.saturating_sub(dividers);
    let base = usable / count as u16;
    let mut extra = usable % count as u16;
    let mut x = area.x;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mut width = base;
        if extra > 0 {
            width += 1;
            extra -= 1;
        }
        out.push(Rect::new(x, area.y, width, area.height));
        x += width + 1;
    }
    out
}

fn layout(area: Rect, app: &App) -> Layout {
    let want_top = app.config.ui.show_topbar;
    let want_tabs = app.config.ui.show_tabs && app.buffers.len() > 1;
    let want_hints = app.config.ui.show_hints;

    // Fixed priority: the editing area always keeps at least one row, then the
    // status bar, then the top bar, then tabs, then hints. Every step is only
    // taken when a spare row remains, so the sum can never exceed the height
    // (which would draw outside the buffer and panic).
    let mut budget = area.height as usize;
    let mut status = 0usize;
    let mut top = 0usize;
    let mut tabs = 0usize;
    let mut hints = 0usize;
    if budget >= 2 {
        status = 1;
        budget -= 1;
    }
    if want_top && budget >= 2 {
        top = 1;
        budget -= 1;
    }
    if want_tabs && budget >= 2 {
        tabs = 1;
        budget -= 1;
    }
    if want_hints && budget >= 2 {
        hints = 1;
    }

    let editor_h = area
        .height
        .saturating_sub((top + tabs + hints + status) as u16);

    let mut y = area.y;
    let topbar = if top > 0 {
        let r = Rect::new(area.x, y, area.width, 1);
        y += 1;
        Some(r)
    } else {
        None
    };
    let tabs_rect = if tabs > 0 {
        let r = Rect::new(area.x, y, area.width, 1);
        y += 1;
        Some(r)
    } else {
        None
    };
    let full_editor = Rect::new(area.x, y, area.width, editor_h);
    let (editor, bookmarks) = if app.ui.bookmarks.open && area.width > 2 {
        let panel_width = (area.width / 3).clamp(8, 32).min(area.width - 2);
        let editor_width = area.width - panel_width - 1;
        (
            Rect::new(area.x, y, editor_width, editor_h),
            Some(Rect::new(
                area.x + editor_width + 1,
                y,
                panel_width,
                editor_h,
            )),
        )
    } else {
        (full_editor, None)
    };
    let panes = split_columns(editor, app.pane_count().max(1));
    y += editor_h;
    let status_rect = if status > 0 {
        let r = Rect::new(area.x, y, area.width, 1);
        y += 1;
        Some(r)
    } else {
        None
    };
    let hints_rect = if hints > 0 {
        Some(Rect::new(area.x, y, area.width, 1))
    } else {
        None
    };

    Layout {
        topbar,
        editor,
        tabs: tabs_rect,
        panes,
        bookmarks,
        status: status_rect,
        hints: hints_rect,
    }
}

/// Render one frame.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    app.sync_bookmark_selection();
    let theme = app.theme.clone();
    frame
        .buffer_mut()
        .set_style(area, Style::default().bg(theme.bg).fg(theme.fg));

    let l = layout(area, app);
    // Mouse hit-testing and the page size both need the rect the editor was
    // actually given; only the draw pass knows it. Same for the tab strip,
    // which is clickable. Every pane remembers its own rect, and the active
    // one's is what the motions and the pointer mapping work against.
    for (i, rect) in l.panes.iter().enumerate() {
        if let Some(pane) = app.panes.get_mut(i) {
            pane.rect = *rect;
        }
    }
    app.set_editor_area(l.panes.get(app.active_pane).copied().unwrap_or_default());
    app.set_tabs_area(l.tabs.unwrap_or_default());
    app.ui.bookmarks.area = l.bookmarks.unwrap_or_default();
    app.ui.bookmarks.hits.clear();

    if let Some(r) = l.topbar {
        chrome::draw_topbar(frame.buffer_mut(), r, app);
    }
    if let Some(r) = l.tabs {
        chrome::draw_tabs(frame.buffer_mut(), r, app);
    }

    // The active pane is drawn first, and without its stored viewport being
    // installed over the document: between frames the document is the fresher
    // of the two, because that is where motions and the wheel put the scroll.
    // The other panes are the opposite — their document may be showing another
    // pane's position, or another buffer's — so they hand theirs over first.
    if app.diff.is_some() {
        // A comparison takes the whole editing area: two columns inside one
        // view, not two panes, because the columns share a scroll offset and a
        // pane owns its own.
        diff::draw(frame.buffer_mut(), l.editor, app);
    } else {
        let active = app.active_pane.min(l.panes.len().saturating_sub(1));
        if !l.panes.is_empty() {
            draw_pane(frame, l.panes[active], app, active, false);
        }
        for pane in 0..l.panes.len() {
            if pane != active {
                draw_pane(frame, l.panes[pane], app, pane, true);
            }
        }
        chrome::draw_pane_dividers(frame.buffer_mut(), &l.panes, app);
    }
    if let Some(panel) = l.bookmarks {
        draw_bookmark_panel(frame.buffer_mut(), panel, app);
    }
    // Leave the active pane's viewport installed in its document: everything
    // after the draw — the status bar, the next keystroke — reads it from
    // there.
    app.install_active_view();

    if let Some(r) = l.status {
        chrome::draw_status(frame.buffer_mut(), r, app);
    }
    if let Some(r) = l.hints {
        chrome::draw_hints(frame.buffer_mut(), r, app);
    } else {
        // No bar, no buttons: a stale hit list would make the row it used to
        // occupy press something invisible.
        app.ui.hint_hits.clear();
        app.ui.hovered_hint = None;
    }

    let span = app.editor_span();

    // Notifications float over the bottom of the editing area, never over the
    // status bar or the hint bar: those carry information the user is reading.
    chrome::draw_messages(frame.buffer_mut(), span, app);

    // A file still being opened gets the middle of the editing area, under any
    // overlay the user has opened since.
    loading::draw(frame.buffer_mut(), span, app);

    if let Some((x, y)) = overlays::draw(frame, app) {
        frame.set_cursor_position((x, y));
    } else if app.ui.bookmarks.focused {
        if let Some(hit) = app
            .ui
            .bookmarks
            .hits
            .iter()
            .find(|hit| Some((hit.document_id, hit.line)) == app.ui.bookmarks.selected)
        {
            let caption = app
                .buffers
                .iter()
                .find(|doc| doc.id() == hit.document_id)
                .and_then(|doc| doc.bookmark_caption(hit.line))
                .unwrap_or_default();
            let cursor_column: usize = caption
                .chars()
                .take(app.ui.bookmarks.caption_cursor)
                .map(|ch| unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1))
                .sum();
            let x = hit
                .caption_x
                .saturating_add(cursor_column as u16)
                .min(hit.area.x + hit.area.width - 1);
            frame.set_cursor_position((x, hit.area.y));
        }
    }
}

fn draw_bookmark_panel(buf: &mut Buffer, area: Rect, app: &mut App) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let theme = app.theme.clone();
    buf.set_style(area, Style::default().fg(theme.fg).bg(theme.bg));
    let divider_x = area.x.saturating_sub(1);
    for y in area.y..area.y + area.height {
        buf[(divider_x, y)]
            .set_symbol("│")
            .set_style(Style::default().fg(theme.line).bg(theme.bg));
    }
    buf.set_string(
        area.x + 1,
        area.y,
        "Bookmarks",
        Style::default()
            .fg(theme.mark)
            .bg(theme.bg)
            .add_modifier(Modifier::BOLD),
    );

    let mut entries = Vec::new();
    for doc in &app.buffers {
        if doc.bookmarks().is_empty() {
            continue;
        }
        entries.push((None, doc.display_name(), None));
        for (line, caption) in doc.bookmarks() {
            let number_width = editor::decimal_digits(line + 1).max(4);
            entries.push((
                Some((doc.id(), *line)),
                format!(
                    "{number:>width$} {caption}",
                    number = line + 1,
                    width = number_width
                ),
                Some(number_width),
            ));
        }
    }
    if entries.is_empty() {
        if area.height > 1 {
            buf.set_string(
                area.x + 1,
                area.y + 1,
                "No bookmarks",
                Style::default().fg(theme.dim).bg(theme.bg),
            );
        }
        return;
    }
    let selected_row = entries
        .iter()
        .position(|(item, _, _)| *item == app.ui.bookmarks.selected);
    let visible_rows = area.height.saturating_sub(1) as usize;
    if let Some(selected) = selected_row {
        if selected < app.ui.bookmarks.scroll {
            app.ui.bookmarks.scroll = selected;
        } else if selected >= app.ui.bookmarks.scroll + visible_rows {
            app.ui.bookmarks.scroll = selected + 1 - visible_rows;
        }
    }
    app.ui.bookmarks.scroll = app.ui.bookmarks.scroll.min(entries.len().saturating_sub(1));
    for (offset, (item, label, number_width)) in entries
        .iter()
        .skip(app.ui.bookmarks.scroll)
        .take(visible_rows)
        .enumerate()
    {
        let y = area.y + 1 + offset as u16;
        let Some((document_id, line)) = item else {
            buf.set_string(
                area.x + 1,
                y,
                label,
                Style::default().fg(theme.dim).bg(theme.bg),
            );
            continue;
        };
        let selected = app.ui.bookmarks.selected == Some((*document_id, *line));
        let style = if selected && app.ui.bookmarks.focused {
            Style::default()
                .fg(theme.overlay_sel_fg)
                .bg(theme.overlay_sel_bg)
        } else if selected {
            Style::default().fg(theme.mark).bg(theme.gutter_bg)
        } else {
            Style::default().fg(theme.fg).bg(theme.bg)
        };
        let row = Rect::new(area.x, y, area.width, 1);
        buf.set_style(row, style);
        buf.set_string(area.x + 1, y, label, style);
        let caption_x = area.x + number_width.unwrap_or(4) as u16 + 2;
        app.ui.bookmarks.hits.push(BookmarkHit {
            area: row,
            document_id: *document_id,
            line: *line,
            caption_x,
        });
        if selected && app.ui.bookmarks.focused {
            let caption = app
                .buffers
                .iter()
                .find(|doc| doc.id() == *document_id)
                .and_then(|doc| doc.bookmark_caption(*line))
                .unwrap_or_default();
            let cursor_offset: usize = caption
                .chars()
                .take(app.ui.bookmarks.caption_cursor)
                .map(|ch| unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1))
                .sum();
            let cursor_x = caption_x + cursor_offset as u16;
            if cursor_x < area.x + area.width {
                buf[(cursor_x, y)].set_style(Style::default().fg(theme.bg).bg(theme.mark));
            }
        }
    }
}

/// Render one pane.
///
/// The viewport is handed over and taken back around the call: the renderer
/// works by adjusting the document's scroll fields, and two panes on one buffer
/// must not share them. `install` is false for the active pane, whose document
/// already holds the live viewport.
fn draw_pane(frame: &mut Frame, area: Rect, app: &mut App, pane: usize, install: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let Some(entry) = app.panes.get(pane).copied() else {
        return;
    };
    let idx = entry.buffer;
    if idx >= app.buffers.len() {
        return;
    }

    if install {
        let doc = &mut app.buffers[idx];
        doc.set_view_top(entry.top, entry.row);
        doc.set_scroll_left(entry.left);
        if entry.detached {
            doc.detach_view();
        } else {
            doc.attach_view();
        }
    }

    let brackets = app.brackets_for(idx);
    let doc_id = app.buffers[idx].id();
    let cache = app.syntax_cache.entry(doc_id).or_default();

    let mut renderer = editor::EditorRenderer {
        doc: &mut app.buffers[idx],
        theme: &app.theme,
        cfg: &app.config,
        search: &app.search,
        brackets,
        cache,
        focused: pane == app.active_pane && app.ui.overlay.is_none() && !app.ui.bookmarks.focused,
        bookmark_gutter_highlight: !app.config.editor.bookmark_gutter_highlight_requires_panel
            || app.ui.bookmarks.open,
        follows_cursor: pane == app.active_pane,
    };
    renderer.render(frame, area);

    app.harvest_view(pane);
}

/// Convenience for tests: render an app into a test backend and return the
/// buffer as plain text rows.
///
/// Exposed (rather than `#[cfg(test)]`) so the integration tests in `tests/`
/// can assert on what the UI actually paints.
#[doc(hidden)]
pub fn render_to_lines(app: &mut App, width: u16, height: u16) -> Vec<String> {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| draw(f, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

/// The theme used by the UI, exposed so tests can assert on colours.
pub fn theme_of(app: &App) -> &Theme {
    &app.theme
}
