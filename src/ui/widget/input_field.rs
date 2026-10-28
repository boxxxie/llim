use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::theme::theme;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TextEditor {
    pub text: String,
    pub cursor: usize, // byte offset in `text`, always at a valid char boundary
}

impl TextEditor {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
        }
    }

    pub fn from_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self { text, cursor }
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.len();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    fn clamp_cursor(&mut self) {
        self.cursor = self.cursor.min(self.text.len());
        while self.cursor > 0 && !self.text.is_char_boundary(self.cursor) {
            self.cursor -= 1;
        }
    }

    fn prev_char_boundary(&self, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }
        self.text[..pos]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    fn next_char_boundary(&self, pos: usize) -> usize {
        if pos >= self.text.len() {
            return self.text.len();
        }
        match self.text[pos..].chars().next() {
            Some(c) => pos + c.len_utf8(),
            None => self.text.len(),
        }
    }

    fn line_start(&self, pos: usize) -> usize {
        self.text[..pos]
            .rfind('\n')
            .map(|idx| idx + 1)
            .unwrap_or(0)
    }

    fn line_end(&self, pos: usize) -> usize {
        self.text[pos..]
            .find('\n')
            .map(|idx| pos + idx)
            .unwrap_or(self.text.len())
    }

    fn prev_word_boundary(&self, pos: usize) -> usize {
        let slice = &self.text[..pos];
        let mut chars: Vec<(usize, char)> = slice.char_indices().collect();
        // Skip trailing whitespace / punctuation
        while let Some(&(_, ch)) = chars.last() {
            if !ch.is_alphanumeric() && ch != '_' {
                chars.pop();
            } else {
                break;
            }
        }
        // Skip word characters
        while let Some(&(_, ch)) = chars.last() {
            if ch.is_alphanumeric() || ch == '_' {
                chars.pop();
            } else {
                break;
            }
        }
        match chars.last() {
            Some(&(i, c)) => i + c.len_utf8(),
            None => 0,
        }
    }

    fn next_word_boundary(&self, pos: usize) -> usize {
        let slice = &self.text[pos..];
        let mut iter = slice.char_indices().peekable();
        // Skip non-word chars
        while let Some(&(_, ch)) = iter.peek() {
            if !ch.is_alphanumeric() && ch != '_' {
                iter.next();
            } else {
                break;
            }
        }
        // Advance through word chars
        while let Some(&(_, ch)) = iter.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                iter.next();
            } else {
                break;
            }
        }
        match iter.peek() {
            Some(&(i, _)) => pos + i,
            None => self.text.len(),
        }
    }

    pub fn move_left(&mut self) {
        self.clamp_cursor();
        self.cursor = self.prev_char_boundary(self.cursor);
    }

    pub fn move_right(&mut self) {
        self.clamp_cursor();
        self.cursor = self.next_char_boundary(self.cursor);
    }

    pub fn move_word_left(&mut self) {
        self.clamp_cursor();
        self.cursor = self.prev_word_boundary(self.cursor);
    }

    pub fn move_word_right(&mut self) {
        self.clamp_cursor();
        self.cursor = self.next_word_boundary(self.cursor);
    }

    pub fn move_line_start(&mut self) {
        self.clamp_cursor();
        self.cursor = self.line_start(self.cursor);
    }

    pub fn move_line_end(&mut self) {
        self.clamp_cursor();
        self.cursor = self.line_end(self.cursor);
    }

    pub fn move_up(&mut self) -> bool {
        self.clamp_cursor();
        let cur_line_start = self.line_start(self.cursor);
        if cur_line_start == 0 {
            return false;
        }
        let col_chars = self.text[cur_line_start..self.cursor].chars().count();
        let prev_line_end = cur_line_start - 1;
        let prev_line_start = self.line_start(prev_line_end);
        let prev_line = &self.text[prev_line_start..prev_line_end];
        let byte_offset: usize = prev_line
            .chars()
            .take(col_chars)
            .map(char::len_utf8)
            .sum();
        self.cursor = prev_line_start + byte_offset;
        true
    }

    pub fn move_down(&mut self) -> bool {
        self.clamp_cursor();
        let cur_line_start = self.line_start(self.cursor);
        let cur_line_end = self.line_end(self.cursor);
        if cur_line_end >= self.text.len() {
            return false;
        }
        let col_chars = self.text[cur_line_start..self.cursor].chars().count();
        let next_line_start = cur_line_end + 1;
        let next_line_end = self.line_end(next_line_start);
        let next_line = &self.text[next_line_start..next_line_end];
        let byte_offset: usize = next_line
            .chars()
            .take(col_chars)
            .map(char::len_utf8)
            .sum();
        self.cursor = next_line_start + byte_offset;
        true
    }

    pub fn insert_char(&mut self, ch: char) {
        self.clamp_cursor();
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn insert_str(&mut self, s: &str) {
        self.clamp_cursor();
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    pub fn backspace(&mut self) {
        self.clamp_cursor();
        if self.cursor > 0 {
            let prev = self.prev_char_boundary(self.cursor);
            self.text.replace_range(prev..self.cursor, "");
            self.cursor = prev;
        }
    }

    pub fn delete_forward(&mut self) {
        self.clamp_cursor();
        if self.cursor < self.text.len() {
            let next = self.next_char_boundary(self.cursor);
            self.text.replace_range(self.cursor..next, "");
        }
    }

    pub fn kill_to_line_start(&mut self) {
        self.clamp_cursor();
        let start = self.line_start(self.cursor);
        if start < self.cursor {
            self.text.replace_range(start..self.cursor, "");
            self.cursor = start;
        }
    }

    pub fn kill_to_line_end(&mut self) {
        self.clamp_cursor();
        let end = self.line_end(self.cursor);
        if self.cursor < end {
            self.text.replace_range(self.cursor..end, "");
        } else if self.cursor < self.text.len() {
            // At end of line, delete the newline character like Emacs C-k
            let next = self.next_char_boundary(self.cursor);
            self.text.replace_range(self.cursor..next, "");
        }
    }

    pub fn kill_word_back(&mut self) {
        self.clamp_cursor();
        let prev = self.prev_word_boundary(self.cursor);
        if prev < self.cursor {
            self.text.replace_range(prev..self.cursor, "");
            self.cursor = prev;
        }
    }

    pub fn kill_word_forward(&mut self) {
        self.clamp_cursor();
        let next = self.next_word_boundary(self.cursor);
        if self.cursor < next {
            self.text.replace_range(self.cursor..next, "");
        }
    }

    /// Handles common Emacs / Readline / terminal keybindings (`Ctrl+a`, `Ctrl+e`, `Ctrl+b`,
    /// `Ctrl+f`, `Ctrl+d`, `Ctrl+h`, `Ctrl+k`, `Ctrl+u`, `Ctrl+w`, `Alt+b`, `Alt+f`, `Alt+d`,
    /// `Alt+Backspace`, `Home`, `End`, `Left`, `Right`, `Backspace`, `Delete`, and in multiline
    /// mode `Ctrl+j`, `Ctrl+m`, `Shift+Enter`, `Enter`, `Ctrl+p`, `Ctrl+n`, `Up`, `Down`).
    /// Returns `true` if the key event was handled by the editor.
    pub fn handle_key(&mut self, e: &KeyEvent, multiline: bool) -> bool {
        let ctrl = e.modifiers.contains(KeyModifiers::CONTROL);
        let alt = e.modifiers.contains(KeyModifiers::ALT);

        if ctrl {
            match e.code {
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    self.move_line_start();
                    return true;
                }
                KeyCode::Char('e') | KeyCode::Char('E') => {
                    self.move_line_end();
                    return true;
                }
                KeyCode::Char('b') | KeyCode::Char('B') => {
                    self.move_left();
                    return true;
                }
                KeyCode::Char('f') | KeyCode::Char('F') => {
                    self.move_right();
                    return true;
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    self.delete_forward();
                    return true;
                }
                KeyCode::Char('h') | KeyCode::Char('H') => {
                    self.backspace();
                    return true;
                }
                KeyCode::Char('k') | KeyCode::Char('K') => {
                    self.kill_to_line_end();
                    return true;
                }
                KeyCode::Char('u') | KeyCode::Char('U') => {
                    self.kill_to_line_start();
                    return true;
                }
                KeyCode::Char('w') | KeyCode::Char('W') | KeyCode::Backspace => {
                    self.kill_word_back();
                    return true;
                }
                KeyCode::Left => {
                    self.move_word_left();
                    return true;
                }
                KeyCode::Right => {
                    self.move_word_right();
                    return true;
                }
                KeyCode::Char('j') | KeyCode::Char('J') | KeyCode::Char('m') | KeyCode::Char('M')
                    if multiline =>
                {
                    self.insert_char('\n');
                    return true;
                }
                KeyCode::Char('o') | KeyCode::Char('O') if multiline => {
                    self.insert_char('\n');
                    self.move_left();
                    return true;
                }
                KeyCode::Char('p') | KeyCode::Char('P') if multiline => {
                    self.move_up();
                    return true;
                }
                KeyCode::Char('n') | KeyCode::Char('N') if multiline => {
                    self.move_down();
                    return true;
                }
                _ => return false,
            }
        }

        if alt {
            match e.code {
                KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Left => {
                    self.move_word_left();
                    return true;
                }
                KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::Right => {
                    self.move_word_right();
                    return true;
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    self.kill_word_forward();
                    return true;
                }
                KeyCode::Backspace => {
                    self.kill_word_back();
                    return true;
                }
                KeyCode::Enter if multiline => {
                    self.insert_char('\n');
                    return true;
                }
                _ => return false,
            }
        }

        match e.code {
            KeyCode::Left => {
                self.move_left();
                true
            }
            KeyCode::Right => {
                self.move_right();
                true
            }
            KeyCode::Home => {
                self.move_line_start();
                true
            }
            KeyCode::End => {
                self.move_line_end();
                true
            }
            KeyCode::Backspace => {
                self.backspace();
                true
            }
            KeyCode::Delete => {
                self.delete_forward();
                true
            }
            KeyCode::Up if multiline => self.move_up(),
            KeyCode::Down if multiline => self.move_down(),
            KeyCode::Enter if multiline => {
                // Covers plain Enter as well as Shift+Enter
                self.insert_char('\n');
                true
            }
            KeyCode::Char(c) => {
                self.insert_char(c);
                true
            }
            _ => false,
        }
    }

    pub fn text_with_cursor(&self) -> String {
        let pos = self.cursor.min(self.text.len());
        let mut out = String::with_capacity(self.text.len() + 4);
        out.push_str(&self.text[..pos]);
        out.push('█');
        out.push_str(&self.text[pos..]);
        out
    }

    pub fn render_lines(&self, style: Style) -> Vec<Line<'static>> {
        let with_cursor = self.text_with_cursor();
        with_cursor
            .split('\n')
            .map(|l| Line::from(Span::styled(l.to_string(), style)))
            .collect()
    }
}

pub struct FilterInputBar {
    pub query_with_cursor: String,
    pub active: bool,
}

impl FilterInputBar {
    pub fn new(query: &str, active: bool) -> Self {
        let query_with_cursor = if active {
            format!("{query}█")
        } else {
            query.to_string()
        };
        Self {
            query_with_cursor,
            active,
        }
    }

    pub fn from_editor(editor: &TextEditor, active: bool) -> Self {
        let query_with_cursor = if active {
            editor.text_with_cursor()
        } else {
            editor.text.clone()
        };
        Self {
            query_with_cursor,
            active,
        }
    }
}

impl Widget for FilterInputBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let line = Line::from(vec![
            Span::styled(" / Filter: ", theme().input_label),
            Span::styled(self.query_with_cursor, theme().input_selected),
            Span::styled("  (Enter to keep, Esc to clear)", theme().input_description),
        ]);
        Paragraph::new(line).render(area, buf);
    }
}
