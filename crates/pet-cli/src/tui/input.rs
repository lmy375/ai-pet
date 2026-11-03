//! Text input state: the draft, its cursor, multi-line navigation and history.

#[derive(Default)]
pub struct InputState {
    pub text: String,
    /// Cursor as a char index into `text`.
    pub cursor: usize,
    history: Vec<String>,
    hist_idx: Option<usize>,
    draft: String,
}

impl InputState {
    fn byte_at(&self, char_idx: usize) -> usize {
        self.text.char_indices().nth(char_idx).map(|(i, _)| i).unwrap_or(self.text.len())
    }
    pub fn insert(&mut self, c: char) {
        let at = self.byte_at(self.cursor);
        self.text.insert(at, c);
        self.cursor += 1;
        self.hist_idx = None;
    }
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let at = self.byte_at(self.cursor - 1);
            self.text.remove(at);
            self.cursor -= 1;
        }
    }
    pub fn delete(&mut self) {
        if self.cursor < self.text.chars().count() {
            let at = self.byte_at(self.cursor);
            self.text.remove(at);
        }
    }
    pub fn set(&mut self, s: String) {
        self.cursor = s.chars().count();
        self.text = s;
    }
    pub fn take(&mut self) -> String {
        self.cursor = 0;
        self.hist_idx = None;
        std::mem::take(&mut self.text)
    }
    pub fn remember(&mut self, line: &str) {
        if !line.trim().is_empty() && self.history.last().map(|s| s.as_str()) != Some(line) {
            self.history.push(line.to_string());
        }
    }
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = match self.hist_idx {
            None => {
                self.draft = self.text.clone();
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.hist_idx = Some(idx);
        let entry = self.history[idx].clone();
        self.set(entry);
        self.hist_idx = Some(idx);
    }
    pub fn history_next(&mut self) {
        let Some(i) = self.hist_idx else { return };
        if i + 1 < self.history.len() {
            let entry = self.history[i + 1].clone();
            self.set(entry);
            self.hist_idx = Some(i + 1);
        } else {
            let draft = std::mem::take(&mut self.draft);
            self.set(draft);
            self.hist_idx = None;
        }
    }

    /// Char index where each logical line (split on `\n`) starts.
    fn line_starts(&self) -> Vec<usize> {
        let mut starts = vec![0usize];
        for (i, c) in self.text.chars().enumerate() {
            if c == '\n' {
                starts.push(i + 1);
            }
        }
        starts
    }

    /// Cursor as `(row, column)` in chars over the logical lines.
    pub fn cursor_rc(&self) -> (usize, usize) {
        let starts = self.line_starts();
        let row = starts.iter().rposition(|&s| s <= self.cursor).unwrap_or(0);
        (row, self.cursor - starts[row])
    }

    /// Move the cursor one logical line up/down, keeping the column where it
    /// fits. `false` = already on the first/last line, so the caller can fall
    /// through to history navigation.
    fn move_row(&mut self, up: bool) -> bool {
        let starts = self.line_starts();
        let (row, col) = self.cursor_rc();
        let target = if up {
            if row == 0 {
                return false;
            }
            row - 1
        } else {
            if row + 1 >= starts.len() {
                return false;
            }
            row + 1
        };
        let total = self.text.chars().count();
        let end = starts.get(target + 1).map(|s| s - 1).unwrap_or(total);
        self.cursor = (starts[target] + col).min(end);
        true
    }

    pub fn move_up(&mut self) -> bool {
        self.move_row(true)
    }

    pub fn move_down(&mut self) -> bool {
        self.move_row(false)
    }

    /// Home/Ctrl+A — start of the current logical line.
    pub fn line_home(&mut self) {
        let (_, col) = self.cursor_rc();
        self.cursor -= col;
    }

    /// End/Ctrl+E — end of the current logical line.
    pub fn line_end(&mut self) {
        let starts = self.line_starts();
        let (row, _) = self.cursor_rc();
        let total = self.text.chars().count();
        self.cursor = starts.get(row + 1).map(|s| s - 1).unwrap_or(total);
    }
}

#[cfg(test)]
mod tests {
    use crate::event::AppEvent;
    use crate::tui::TuiApp;
    use ratatui::crossterm::event::{Event as TermEvent, KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn typed(a: &mut TuiApp, s: &str) {
        for c in s.chars() {
            a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Char(c)))));
        }
    }

    fn app() -> TuiApp {
        let mut a = TuiApp::new();
        a.entries.clear();
        a.line_cache.clear();
        a
    }

    #[test]
    fn arrows_walk_a_multiline_draft_before_reaching_history() {
        let mut a = app();
        a.input.remember("旧消息");
        typed(&mut a, "ab");
        a.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
        typed(&mut a, "cdef");
        assert_eq!(a.input.cursor_rc(), (1, 4));

        // Up stays inside the draft (column preserved), then falls through to
        // history once there's no line above.
        a.on_key(key(KeyCode::Up));
        assert_eq!(a.input.cursor_rc(), (0, 2), "column clamps to the shorter line");
        a.on_key(key(KeyCode::Down));
        assert_eq!(a.input.cursor_rc(), (1, 2));
        a.on_key(key(KeyCode::Up));
        a.on_key(key(KeyCode::Up));
        assert_eq!(a.input.text, "旧消息");
    }

    #[test]
    fn home_and_end_stay_on_the_cursors_line() {
        let mut a = app();
        typed(&mut a, "ab");
        a.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
        typed(&mut a, "cdef");
        a.on_key(key(KeyCode::Home));
        assert_eq!(a.input.cursor_rc(), (1, 0));
        a.on_key(key(KeyCode::End));
        assert_eq!(a.input.cursor_rc(), (1, 4));
    }
}