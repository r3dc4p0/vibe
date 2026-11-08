use crossterm::event::{KeyEvent, KeyModifiers};

use crate::buffer::Document;
use crate::input::{self, Action};
use unicode_width::UnicodeWidthChar;

use super::App;

impl App {
    pub(crate) fn toggle_bookmark_panel(&mut self) {
        self.ui.bookmarks.open = !self.ui.bookmarks.open;
        self.ui.bookmarks.focused = false;
        self.ui.bookmarks.area = ratatui::layout::Rect::default();
        self.ui.bookmarks.hits.clear();
    }

    pub(crate) fn focus_bookmark_panel(&mut self) {
        if self.ui.bookmarks.open {
            self.ui.bookmarks.focused = !self.ui.bookmarks.focused;
            self.sync_bookmark_selection();
        }
    }

    pub(crate) fn toggle_active_bookmark(&mut self, line: usize) {
        let Some(index) = self.active_index() else {
            return;
        };
        if self.buffers[index].bookmarks().contains_key(&line) {
            self.buffers[index].toggle_bookmark(line);
        } else {
            let next = self
                .buffers
                .iter()
                .map(|doc| doc.bookmarks().len())
                .sum::<usize>()
                + 1;
            if self.buffers[index].toggle_bookmark(line) {
                self.buffers[index].set_bookmark_caption(line, format!("bookmark {next}"));
                self.ui.bookmarks.selected = Some((self.buffers[index].id(), line));
                self.ui.bookmarks.caption_cursor = 0;
                self.ui.bookmarks.replace_default_caption_on_type = true;
                self.ui.bookmarks.open = true;
            }
        }
        self.sync_bookmark_selection();
    }

    pub(crate) fn click_bookmark(&mut self, document_id: usize, line: usize, column: u16) {
        let Some(doc) = self.buffers.iter().find(|doc| doc.id() == document_id) else {
            return;
        };
        let caption_start = self
            .ui
            .bookmarks
            .hits
            .iter()
            .find(|hit| hit.document_id == document_id && hit.line == line)
            .map_or(column, |hit| hit.caption_x);
        let cursor = if column >= caption_start {
            let display_column = column.saturating_sub(caption_start) as usize;
            char_index_at_display_column(
                doc.bookmark_caption(line).unwrap_or_default(),
                display_column,
            )
        } else {
            0
        };
        self.ui.bookmarks.selected = Some((document_id, line));
        self.ui.bookmarks.caption_cursor = cursor;
        self.ui.bookmarks.replace_default_caption_on_type = false;
        self.ui.bookmarks.focused = true;
    }

    pub(crate) fn handle_bookmark_key(&mut self, key: &KeyEvent) {
        self.sync_bookmark_selection();
        let action = input::translate(key, &self.keymap);
        match action {
            Action::Command(command) => self.execute(command),
            Action::Escape => self.ui.bookmarks.focused = false,
            Action::Move { motion, .. } => match motion {
                crate::buffer::Motion::Up => self.step_bookmark(-1),
                crate::buffer::Motion::Down => self.step_bookmark(1),
                crate::buffer::Motion::Left => {
                    self.ui.bookmarks.replace_default_caption_on_type = false;
                    self.move_caption_cursor(-1);
                }
                crate::buffer::Motion::Right => {
                    self.ui.bookmarks.replace_default_caption_on_type = false;
                    self.move_caption_cursor(1);
                }
                crate::buffer::Motion::LineStart | crate::buffer::Motion::LineStartSmart => {
                    self.ui.bookmarks.replace_default_caption_on_type = false;
                    self.ui.bookmarks.caption_cursor = 0;
                }
                crate::buffer::Motion::LineEnd => {
                    self.ui.bookmarks.replace_default_caption_on_type = false;
                    self.caption_end();
                }
                _ => {}
            },
            Action::InsertText(text) => self.insert_bookmark_caption(&text),
            Action::Backspace => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.remove_selected_bookmark();
                } else {
                    self.delete_caption_char(true);
                }
            }
            Action::DeleteForward => self.remove_selected_bookmark(),
            Action::InsertNewline => self.navigate_to_selected_bookmark(),
            Action::Ignored | Action::InsertTab | Action::Dedent => {}
        }
    }

    fn bookmark_refs(&self) -> Vec<(usize, usize)> {
        self.buffers
            .iter()
            .flat_map(|doc| doc.bookmarks().keys().map(move |line| (doc.id(), *line)))
            .collect()
    }

    pub(crate) fn sync_bookmark_selection(&mut self) {
        let refs = self.bookmark_refs();
        if !self
            .ui
            .bookmarks
            .selected
            .is_some_and(|selected| refs.contains(&selected))
        {
            self.ui.bookmarks.replace_default_caption_on_type = false;
            self.ui.bookmarks.selected = refs.first().copied();
            self.caption_end();
        }
    }

    fn step_bookmark(&mut self, delta: isize) {
        let refs = self.bookmark_refs();
        if refs.is_empty() {
            self.ui.bookmarks.selected = None;
            return;
        }
        let at = self
            .ui
            .bookmarks
            .selected
            .and_then(|selected| refs.iter().position(|item| *item == selected))
            .unwrap_or(0);
        let next = (at as isize + delta).clamp(0, refs.len() as isize - 1) as usize;
        self.ui.bookmarks.selected = Some(refs[next]);
        self.ui.bookmarks.replace_default_caption_on_type = false;
        self.caption_end();
    }

    fn selected_bookmark(&self) -> Option<(usize, usize, usize)> {
        let (document_id, line) = self.ui.bookmarks.selected?;
        let index = self
            .buffers
            .iter()
            .position(|doc| doc.id() == document_id)?;
        self.buffers[index]
            .bookmarks()
            .contains_key(&line)
            .then_some((index, document_id, line))
    }

    fn caption_end(&mut self) {
        let length = self
            .selected_bookmark()
            .and_then(|(index, _, line)| self.buffers[index].bookmark_caption(line))
            .map_or(0, |caption| caption.chars().count());
        self.ui.bookmarks.caption_cursor = length;
    }

    fn move_caption_cursor(&mut self, delta: isize) {
        let length = self
            .selected_bookmark()
            .and_then(|(index, _, line)| self.buffers[index].bookmark_caption(line))
            .map_or(0, |caption| caption.chars().count());
        self.ui.bookmarks.caption_cursor =
            (self.ui.bookmarks.caption_cursor as isize + delta).clamp(0, length as isize) as usize;
    }

    pub(crate) fn insert_bookmark_caption(&mut self, text: &str) {
        let Some((index, _, line)) = self.selected_bookmark() else {
            return;
        };
        if self.ui.bookmarks.replace_default_caption_on_type {
            self.buffers[index].set_bookmark_caption(line, text.to_string());
            self.ui.bookmarks.caption_cursor = text.chars().count();
            self.ui.bookmarks.replace_default_caption_on_type = false;
            return;
        }
        let mut chars: Vec<char> = self.buffers[index]
            .bookmark_caption(line)
            .unwrap_or_default()
            .chars()
            .collect();
        let at = self.ui.bookmarks.caption_cursor.min(chars.len());
        let inserted: Vec<char> = text.chars().collect();
        chars.splice(at..at, inserted.iter().copied());
        self.buffers[index].set_bookmark_caption(line, chars.iter().collect());
        self.ui.bookmarks.caption_cursor = at + inserted.len();
    }

    fn delete_caption_char(&mut self, backwards: bool) {
        let Some((index, _, line)) = self.selected_bookmark() else {
            return;
        };
        let mut chars: Vec<char> = self.buffers[index]
            .bookmark_caption(line)
            .unwrap_or_default()
            .chars()
            .collect();
        let cursor = self.ui.bookmarks.caption_cursor.min(chars.len());
        if backwards && cursor > 0 {
            chars.remove(cursor - 1);
            self.ui.bookmarks.caption_cursor -= 1;
        } else if !backwards && cursor < chars.len() {
            chars.remove(cursor);
        } else if chars.is_empty() {
            self.remove_selected_bookmark();
            return;
        }
        self.buffers[index].set_bookmark_caption(line, chars.iter().collect());
    }

    fn remove_selected_bookmark(&mut self) {
        let Some((index, _, line)) = self.selected_bookmark() else {
            return;
        };
        self.buffers[index].toggle_bookmark(line);
        self.sync_bookmark_selection();
    }

    fn navigate_to_selected_bookmark(&mut self) {
        let Some((index, document_id, line)) = self.selected_bookmark() else {
            return;
        };
        if let Some(pane) = self.panes.iter().position(|pane| {
            self.buffers
                .get(pane.buffer)
                .is_some_and(|doc| doc.id() == document_id)
        }) {
            self.focus_pane(pane);
        } else {
            self.set_active_buffer(index);
        }
        if let Some(doc) = self.active_doc_mut() {
            doc.goto_line_col(line, 0);
        }
        self.ui.bookmarks.focused = false;
    }
}

fn char_index_at_display_column(text: &str, column: usize) -> usize {
    let mut display_column = 0;
    for (index, ch) in text.chars().enumerate() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(1);
        if column < display_column + width {
            return index;
        }
        display_column += width;
    }
    text.chars().count()
}

impl Document {
    pub(crate) fn editor_row_for_display_row(
        &self,
        config: &crate::config::Config,
        text_width: usize,
        visual_row: usize,
    ) -> Option<crate::ui::editor::EditorRow> {
        let wrap_width = config
            .editor
            .word_wrap
            .then_some(text_width)
            .filter(|w| *w > 0);
        let mut cache = crate::buffer::LayoutCache::new(wrap_width, config.tab_width());
        let rows = crate::ui::editor::rows_for_document(self, &mut cache, visual_row + 1);
        rows.get(visual_row).cloned()
    }

    pub(crate) fn line_for_bookmark_row(
        &self,
        config: &crate::config::Config,
        text_width: usize,
        visual_row: usize,
    ) -> Option<usize> {
        self.editor_row_for_display_row(config, text_width, visual_row)
            .and_then(|row| row.line())
    }
}
