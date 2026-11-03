//! Viewport geometry: wrapped-line cache, scrolling and entry selection.

use ratatui::text::Line;

use super::wrap::wrap_lines;

impl super::TuiApp {
    fn total_lines(&self) -> usize {
        self.line_cache.iter().map(|c| c.len()).sum()
    }

    /// Re-wrap dirty entries for `width`; a width change rebuilds everything.
    pub fn ensure_cache(&mut self, width: usize) {
        if width != self.cache_width {
            self.cache_width = width;
            for c in &mut self.line_cache {
                c.clear();
            }
        }
        for (i, entry) in self.entries.iter().enumerate() {
            if self.line_cache[i].is_empty() {
                let mut lines = wrap_lines(entry.lines(), width);
                lines.push(Line::from("")); // separator
                self.line_cache[i] = lines;
            }
        }
    }

    /// The visible window of wrapped lines, with the selected entry's header
    /// row highlighted.
    pub fn visible_lines(&mut self, width: usize, height: usize) -> Vec<Line<'static>> {
        self.ensure_cache(width);
        self.last_height = height;
        let total = self.total_lines();
        let max_scroll = total.saturating_sub(height);
        if self.stick_bottom {
            self.scroll = max_scroll;
        } else {
            self.scroll = self.scroll.min(max_scroll);
        }
        let mut out = Vec::with_capacity(height);
        let mut skipped = 0usize;
        'outer: for (idx, cache) in self.line_cache.iter().enumerate() {
            for (j, line) in cache.iter().enumerate() {
                if skipped < self.scroll {
                    skipped += 1;
                    continue;
                }
                let mut line = line.clone();
                if j == 0 && self.sel_entry == Some(idx) {
                    for span in &mut line.spans {
                        span.style = span.style.add_modifier(
                            ratatui::style::Modifier::REVERSED,
                        );
                    }
                }
                out.push(line);
                if out.len() >= height {
                    break 'outer;
                }
            }
        }
        out
    }

    fn entry_start_line(&self, idx: usize) -> usize {
        self.line_cache[..idx].iter().map(|c| c.len()).sum()
    }

    pub(super) fn scroll_by(&mut self, delta: isize) {
        let total = self.total_lines();
        let max_scroll = total.saturating_sub(self.last_height);
        let next = self.scroll as isize + delta;
        self.scroll = next.clamp(0, max_scroll as isize) as usize;
        self.stick_bottom = self.scroll >= max_scroll;
    }

    fn ensure_entry_visible(&mut self, idx: usize) {
        let start = self.entry_start_line(idx);
        if start < self.scroll || start >= self.scroll + self.last_height {
            self.scroll = start;
        }
        self.stick_bottom = false;
    }

    pub(super) fn move_selection(&mut self, up: bool) {
        let toggleables: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.toggleable())
            .map(|(i, _)| i)
            .collect();
        if toggleables.is_empty() {
            return;
        }
        let next = match (self.sel_entry, up) {
            (None, true) => Some(*toggleables.last().unwrap()),
            (None, false) => None,
            (Some(cur), true) => {
                toggleables.iter().rev().find(|&&i| i < cur).copied().or(Some(cur))
            }
            (Some(cur), false) => toggleables.iter().find(|&&i| i > cur).copied(),
        };
        self.sel_entry = next;
        match next {
            Some(i) => self.ensure_entry_visible(i),
            None => self.stick_bottom = true, // walked below the last one — back to live tail
        }
    }

    pub(super) fn toggle_selected(&mut self) {
        if let Some(i) = self.sel_entry {
            if let Some(e) = self.entries.get_mut(i) {
                e.toggle();
            }
            self.touch(i);
            self.ensure_entry_visible(i);
        }
    }

    pub(super) fn toggle_last(&mut self) {
        if let Some(i) = self.entries.iter().rposition(|e| e.toggleable()) {
            self.entries[i].toggle();
            self.touch(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::entries::Entry;
    use super::super::TuiApp;

    fn app() -> TuiApp {
        let mut a = TuiApp::new();
        a.entries.clear();
        a.line_cache.clear();
        a
    }

    #[test]
    fn visible_lines_reverses_the_selected_entry_header_only() {
        let mut a = app();
        a.push(Entry::Notice { text: "第一行".into() });
        a.push(Entry::Notice { text: "第二行".into() });
        a.sel_entry = Some(1);
        let lines = a.visible_lines(80, 20);
        // Each entry caches its lines followed by one blank separator, so index
        // 2 is the second (selected) entry's header row.
        assert_eq!(lines.len(), 4);
        let reversed = ratatui::style::Modifier::REVERSED;
        assert!(lines[0].spans.iter().all(|s| !s.style.add_modifier.contains(reversed)));
        assert!(lines[2].spans.iter().all(|s| s.style.add_modifier.contains(reversed)));
    }
}