//! Key handling: picker navigation, multi-line input and the normal bindings.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::Mode;
use super::entries::Entry;
use super::Action;

impl super::TuiApp {
    pub(super) fn on_key(&mut self, key: KeyEvent) -> Vec<Action> {
        if key.kind != KeyEventKind::Press {
            return vec![];
        }
        // A modal picker captures navigation until closed.
        if self.picker.is_some() {
            return self.on_key_picker(key);
        }
        if is_newline_chord(&key) {
            self.insert_newline();
            return vec![];
        }
        self.on_key_normal(key)
    }

    fn on_key_picker(&mut self, key: KeyEvent) -> Vec<Action> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if let Some(p) = &mut self.picker {
            match (key.code, ctrl) {
                (KeyCode::Char('c'), true) | (KeyCode::Esc, _) => self.picker = None,
                (KeyCode::Up, _) => p.up(),
                (KeyCode::Down, _) => p.down(),
                (KeyCode::Char(' '), false) => p.toggle(),
                (KeyCode::Enter, _) => return self.confirm_picker(),
                _ => {}
            }
        }
        vec![]
    }

    /// Shift/Alt+Enter insert a newline instead of submitting. Terminals only
    /// report the modifier on Enter when they speak the kitty keyboard protocol
    /// (pushed in main.rs when supported); Ctrl+J is the fallback that works
    /// everywhere, and is what the status bar advertises when Shift+Enter isn't
    /// available.
    fn insert_newline(&mut self) {
        self.sel_entry = None;
        self.input.insert('\n');
        self.palette_dismissed = true;
        self.refresh_palette();
        self.stick_bottom = true;
    }

    fn on_key_normal(&mut self, key: KeyEvent) -> Vec<Action> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match (key.code, ctrl) {
            (KeyCode::Char('c'), true) => {
                if self.input.text.is_empty() {
                    return vec![Action::Quit];
                }
                self.input.take();
                self.refresh_palette();
            }
            (KeyCode::Char('d'), true) => return vec![Action::Quit],
            (KeyCode::Char('a'), true) => self.input.line_home(),
            (KeyCode::Char('e'), true) => self.input.line_end(),
            (KeyCode::Char('u'), true) => {
                self.input.take();
                self.refresh_palette();
            }
            (KeyCode::Char('p'), true) => self.input.history_prev(),
            (KeyCode::Char('n'), true) => self.input.history_next(),
            (KeyCode::Char('o'), true) => self.toggle_last(),
            (KeyCode::Char(c), false) => {
                self.sel_entry = None;
                self.palette_dismissed = false;
                self.input.insert(c);
                self.refresh_palette();
                self.stick_bottom = true;
            }
            (KeyCode::Backspace, _) => {
                self.palette_dismissed = false;
                self.input.backspace();
                self.refresh_palette();
            }
            (KeyCode::Delete, _) => {
                self.input.delete();
                self.refresh_palette();
            }
            (KeyCode::Left, _) => self.input.cursor = self.input.cursor.saturating_sub(1),
            (KeyCode::Right, _) => {
                self.input.cursor = (self.input.cursor + 1).min(self.input.text.chars().count())
            }
            (KeyCode::Home, _) => self.input.line_home(),
            (KeyCode::End, _) => self.input.line_end(),
            (KeyCode::PageUp, _) => self.scroll_by(-(self.last_height as isize - 1)),
            (KeyCode::PageDown, _) => self.scroll_by(self.last_height as isize - 1),
            // In a multi-line draft ↑↓ walk its lines first; history navigation
            // takes over once the cursor leaves the top/bottom line.
            (KeyCode::Up, _) => {
                if self.palette_visible() {
                    self.palette_sel = self.palette_sel.saturating_sub(1);
                } else if self.input.text.is_empty() {
                    self.move_selection(true);
                } else if !self.input.move_up() {
                    self.input.history_prev();
                }
            }
            (KeyCode::Down, _) => {
                if self.palette_visible() {
                    self.palette_sel = (self.palette_sel + 1).min(self.palette.len() - 1);
                } else if self.input.text.is_empty() {
                    self.move_selection(false);
                } else if !self.input.move_down() {
                    self.input.history_next();
                }
            }
            (KeyCode::Tab, _) => {
                if self.palette_visible() {
                    if let Some(item) = self.palette.get(self.palette_sel).cloned() {
                        self.input.set(item.insert);
                        self.palette_dismissed = true;
                        self.refresh_palette();
                    }
                }
            }
            (KeyCode::Esc, _) => {
                if self.palette_visible() {
                    self.palette_dismissed = true;
                } else if self.sel_entry.is_some() {
                    self.sel_entry = None;
                    self.stick_bottom = true;
                }
            }
            (KeyCode::Enter, _) => {
                if self.palette_visible() {
                    if let Some(item) = self.palette.get(self.palette_sel).cloned() {
                        self.input.set(item.insert.clone());
                        self.palette_dismissed = true;
                        self.refresh_palette();
                        if item.run {
                            return self.submit();
                        }
                    }
                } else if self.input.text.is_empty() {
                    if self.sel_entry.is_some() {
                        self.toggle_selected();
                    }
                } else {
                    return self.submit();
                }
            }
            _ => {}
        }
        vec![]
    }

    fn submit(&mut self) -> Vec<Action> {
        if self.busy {
            self.push(Entry::Notice { text: "上一轮仍在进行中，请稍候…".to_string() });
            return vec![];
        }
        let line = self.input.take();
        self.refresh_palette();
        if line.trim().is_empty() {
            return vec![];
        }
        self.input.remember(&line);
        self.stick_bottom = true;
        self.sel_entry = None;

        if let Some(actions) = self.open_picker_for(line.trim()) {
            return actions;
        }

        // Local echo: chat messages appear as the owner's bubble; group
        // messages come back via the transcript event, commands echo nothing.
        // `/skill:` is a command in name only — it expands into a chat turn, so
        // it needs the bubble and the streaming placeholder like any message.
        let is_chat_line = !line.trim_start().starts_with('/') || pet_core::skills::is_command(&line);
        if self.mode == Mode::Chat && is_chat_line {
            let name = self.agent_name();
            self.push(Entry::User { text: line.clone() });
            self.push(Entry::Assistant {
                name,
                text: String::new(),
                reasoning: String::new(),
                streaming: true,
                reasoning_expanded: false,
            });
        }
        self.busy = true;
        vec![Action::Submit(line)]
    }
}

fn is_newline_chord(key: &KeyEvent) -> bool {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Enter => key
            .modifiers
            .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::SUPER),
        KeyCode::Char('j') => ctrl,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::AppEvent;
    use crate::tui::entries::Entry;
    use crate::tui::{palette, picker, TuiApp};
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
    fn typing_slash_opens_palette_and_arrows_select() {
        let mut a = app();
        a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Char('/')))));
        assert!(a.palette_visible());
        // Every chat command is offered. The count isn't fixed: typing `/`
        // rescans the owner's skills dir, and each usable skill adds a
        // `/skill:` entry after the static commands.
        let labels: Vec<&str> = a.palette.iter().map(|i| i.label.as_str()).collect();
        for c in palette::CHAT_COMMANDS {
            assert!(labels.contains(&c.name), "palette missing {}", c.name);
        }
        assert!(a.palette[palette::CHAT_COMMANDS.len()..]
            .iter()
            .all(|i| i.label.starts_with(pet_core::skills::COMMAND_PREFIX)));
        let before = a.palette_sel;
        a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Down))));
        assert_eq!(a.palette_sel, before + 1);
        a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Up))));
        assert_eq!(a.palette_sel, before);
    }

    #[test]
    fn enter_on_argless_palette_item_submits_it() {
        let mut a = app();
        for c in "/help".chars() {
            a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Char(c)))));
        }
        let actions = a.on_key(key(KeyCode::Enter));
        match actions.as_slice() {
            [Action::Submit(line)] => assert_eq!(line, "/help"),
            _ => panic!("expected submit"),
        }
        assert!(a.busy);
    }

    #[test]
    fn shift_enter_and_ctrl_j_insert_newlines_that_enter_submits_as_one_message() {
        let mut a = app();
        typed(&mut a, "第一行");
        assert!(a.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)).is_empty());
        assert!(!a.busy, "Shift+Enter must not send the message");
        typed(&mut a, "第二行");
        // Fallback chord for terminals that report Shift+Enter as a bare Enter.
        a.on_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
        typed(&mut a, "第三行");
        assert_eq!(a.input.text, "第一行\n第二行\n第三行");

        match a.on_key(key(KeyCode::Enter)).as_slice() {
            [Action::Submit(line)] => assert_eq!(line, "第一行\n第二行\n第三行"),
            _ => panic!("expected submit"),
        }
    }

    #[test]
    fn agents_command_opens_picker_instead_of_dispatching() {
        let mut a = app();
        for c in "/agents".chars() {
            a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Char(c)))));
        }
        // Palette Enter accepts "/agents" (run) → submit intercepts it into a
        // picker overlay: no dispatched action, no busy, nothing in transcript.
        let actions = a.on_key(key(KeyCode::Enter));
        assert!(actions.is_empty());
        assert!(!a.busy);
        assert!(a.picker.is_some());
        assert!(a.entries.is_empty());
    }

    #[test]
    fn skill_command_dispatches_as_a_chat_turn() {
        // `/skill:<slug>` is a chat turn wearing a command's clothes: unlike
        // every other `/` line it must dispatch AND echo the owner's bubble +
        // streaming placeholder, or the UI looks frozen while the model works.
        let mut a = app();
        for c in "/skill:x hi".chars() {
            a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Char(c)))));
        }
        let actions = a.on_key(key(KeyCode::Enter));
        match actions.as_slice() {
            [Action::Submit(line)] => assert_eq!(line, "/skill:x hi"),
            other => panic!("expected submit, got {} actions", other.len()),
        }
        assert!(a.busy);
        assert!(matches!(a.entries.first(), Some(Entry::User { .. })));
        assert!(matches!(a.entries.get(1), Some(Entry::Assistant { streaming: true, .. })));
    }

    #[test]
    fn picker_captures_navigation_and_esc_closes() {
        let mut a = app();
        a.picker = Some(picker::Picker {
            title: String::new(),
            kind: picker::PickerKind::ViewOnly,
            items: (0..3)
                .map(|i| picker::PickerItem {
                    id: format!("{i}"),
                    label: format!("row{i}"),
                    desc: String::new(),
                    checked: None,
                })
                .collect(),
            sel: 0,
        });
        a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Down))));
        assert_eq!(a.picker.as_ref().unwrap().sel, 1);
        // While the picker is open, typing must not reach the input line.
        a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Char('x')))));
        assert!(a.input.text.is_empty());
        a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Esc))));
        assert!(a.picker.is_none());
    }

    #[test]
    fn members_picker_enter_applies_checked_set() {
        let mut a = app();
        a.picker = Some(picker::Picker {
            title: String::new(),
            kind: picker::PickerKind::Members,
            items: vec![
                picker::PickerItem {
                    id: "a".into(),
                    label: "甲".into(),
                    desc: String::new(),
                    checked: Some(false),
                },
                picker::PickerItem {
                    id: "b".into(),
                    label: "乙".into(),
                    desc: String::new(),
                    checked: Some(true),
                },
            ],
            sel: 0,
        });
        // Space checks the first row, Enter applies both.
        a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Char(' ')))));
        let actions = a.on_key(key(KeyCode::Enter));
        match actions.as_slice() {
            [Action::SetMembers(ids)] => assert_eq!(ids, &vec!["a".to_string(), "b".to_string()]),
            _ => panic!("expected SetMembers"),
        }
        assert!(a.busy);
        assert!(a.picker.is_none());
    }

    #[test]
    fn esc_dismisses_palette_until_input_changes() {
        let mut a = app();
        a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Char('/')))));
        assert!(a.palette_visible());
        a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Esc))));
        assert!(!a.palette_visible());
        a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Char('a')))));
        assert!(a.palette_visible());
    }
}