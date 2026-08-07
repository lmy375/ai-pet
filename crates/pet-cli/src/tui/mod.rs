//! TUI state machine: one struct owns the transcript, input line, palette,
//! selection and scroll state; terminal + engine events mutate it and the draw
//! module renders it. Effects (submits, turns, quit) are returned to the run
//! loop in `main.rs`, which owns the async plumbing.

pub mod draw;
pub mod entries;
pub mod markdown;
pub mod palette;
pub mod picker;
pub mod wrap;

use std::collections::HashMap;

use pet_core::chat::StreamEvent;
use pet_core::session;
use pet_core::settings::get_settings;
use pet_core::shell::TaskCompletion;
use pet_core::skills::Skill;
use ratatui::crossterm::event::{Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::Color;
use ratatui::text::Line;

use crate::app::TurnInput;
use crate::event::AppEvent;
use crate::Mode;
use entries::{agent_color, Entry, ToolCall};
use palette::{palette_items, PaletteItem};
use picker::{Picker, PickerKind};
use wrap::wrap_lines;

/// What the run loop must do after applying an event.
pub enum Action {
    /// Dispatch this line to the command/chat handler (sets busy).
    Submit(String),
    /// Run a chat turn directly (background-task resume; sets busy).
    Turn(TurnInput),
    /// Build and open the tasks picker (the run loop owns the shell store).
    OpenTasks,
    /// Fetch the model list from the API and open the model picker.
    OpenModels,
    /// Build and open the group-members picker (needs the async group state).
    OpenMembers,
    /// Apply the member set picked in the overlay (sets busy).
    SetMembers(Vec<String>),
    Quit,
}

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
}

pub struct TuiApp {
    pub mode: Mode,
    pub entries: Vec<Entry>,
    pub input: InputState,
    pub busy: bool,
    pub spin: usize,
    pub header: String,
    /// Active agent name + model, for the status bar (kept in sync by
    /// `refresh_header`).
    pub agent_name_cached: String,
    pub model: String,
    pub context_usage: Option<(u64, u64)>,
    /// Tools available to the active agent (built-in + MCP); recomputed in the
    /// background whenever `tools_dirty` is set.
    pub tools_count: Option<usize>,
    pub tools_dirty: bool,

    // Palette
    pub palette: Vec<PaletteItem>,
    pub palette_sel: usize,
    palette_dismissed: bool,
    /// Skills backing the `/skill:<slug>` palette entries — which double as the
    /// only skills listing the CLI has. Re-scanned each time the palette opens
    /// (see `refresh_palette`), so a newly added skill is completable without
    /// restarting — one `read_dir`, not one per keystroke.
    skills: Vec<Skill>,

    /// Modal list overlay (/agents, /sessions, /tasks, /members). While open
    /// it captures ↑↓/Space/Enter/Esc.
    pub picker: Option<Picker>,

    // Transcript selection (over toggleable entries; input must be empty)
    pub sel_entry: Option<usize>,

    // Scroll state
    pub scroll: usize,
    pub stick_bottom: bool,
    pub last_height: usize,

    // Wrapped-line cache, parallel to `entries` (each includes its trailing
    // separator blank line).
    line_cache: Vec<Vec<Line<'static>>>,
    cache_width: usize,

    // Group: agent_id → index of its currently-open Tool entry.
    group_tools: HashMap<String, usize>,
    // Pending background-task completions waiting for idle.
    pending_tasks: Vec<TaskCompletion>,
}

impl TuiApp {
    pub fn new() -> Self {
        let mut app = Self {
            mode: Mode::Chat,
            entries: Vec::new(),
            input: InputState::default(),
            busy: false,
            spin: 0,
            header: String::new(),
            agent_name_cached: String::new(),
            model: String::new(),
            context_usage: None,
            tools_count: None,
            tools_dirty: true,
            palette: Vec::new(),
            palette_sel: 0,
            palette_dismissed: false,
            skills: pet_core::skills::list_skills(),
            picker: None,
            sel_entry: None,
            scroll: 0,
            stick_bottom: true,
            last_height: 20,
            line_cache: Vec::new(),
            cache_width: 0,
            group_tools: HashMap::new(),
            pending_tasks: Vec::new(),
        };
        app.refresh_header();
        app.replay_session(8);
        app.push(Entry::Notice {
            text: "输入 / 弹出命令面板；/help 查看用法。与桌面 GUI 共享配置与会话。".to_string(),
        });
        app
    }

    // --- entry plumbing ---

    fn push(&mut self, e: Entry) {
        self.entries.push(e);
        self.line_cache.push(Vec::new());
    }

    fn touch(&mut self, idx: usize) {
        if let Some(c) = self.line_cache.get_mut(idx) {
            c.clear();
        }
    }

    fn touch_last(&mut self) {
        if !self.entries.is_empty() {
            let i = self.entries.len() - 1;
            self.touch(i);
        }
    }

    pub fn agent_name(&self) -> String {
        get_settings()
            .ok()
            .and_then(|s| s.active_agent_config().map(|a| a.name.clone()))
            .unwrap_or_else(|| "pet".to_string())
    }

    /// Refresh the header + status-bar facts (agent, model, persisted context
    /// usage) from disk, and schedule a tool recount. Called at startup and
    /// after every turn / command / mode change.
    pub fn refresh_header(&mut self) {
        let settings = get_settings().unwrap_or_default();
        let (name, model) = settings
            .active_agent_config()
            .map(|a| (a.name.clone(), a.model.clone()))
            .unwrap_or_default();
        let index = session::list_sessions();
        let title = index
            .sessions
            .iter()
            .find(|m| m.id == index.active_id)
            .map(|m| m.title.clone())
            .unwrap_or_else(|| "新会话".to_string());
        self.header = match self.mode {
            Mode::Chat => format!(" pet · 会话:{title}"),
            Mode::Group => " pet · 群聊".to_string(),
        };
        self.agent_name_cached = name;
        self.model = model;
        // Last persisted occupancy of the active session (live Usage events
        // overwrite this during a turn).
        if !index.active_id.is_empty() {
            if let Ok(sess) = session::load_session(index.active_id) {
                self.context_usage = sess.context_usage.map(|u| (u.used, u.total));
            }
        }
        self.tools_dirty = true;
    }

    /// Seed the transcript with the tail of the active session so reopening
    /// the CLI shows where the conversation left off.
    fn replay_session(&mut self, limit: usize) {
        let index = session::list_sessions();
        if index.active_id.is_empty() {
            return;
        }
        let Ok(sess) = session::load_session(index.active_id) else { return };
        let name = self.agent_name();
        let items = &sess.items;
        let start = items.len().saturating_sub(limit);
        for item in &items[start..] {
            match item["type"].as_str().unwrap_or("") {
                "user" => self.push(Entry::User {
                    text: item["content"].as_str().unwrap_or("").to_string(),
                }),
                "assistant" => {
                    let text = item["content"].as_str().unwrap_or("").to_string();
                    if text.is_empty() {
                        continue; // image-only bubbles
                    }
                    self.push(Entry::Assistant {
                        name: name.clone(),
                        text,
                        reasoning: item["reasoning"].as_str().unwrap_or("").to_string(),
                        streaming: false,
                        reasoning_expanded: false,
                    });
                }
                "tool" => {
                    let calls = item["toolCalls"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .map(|tc| ToolCall {
                                    name: tc["name"].as_str().unwrap_or("").to_string(),
                                    args: tc["arguments"].as_str().unwrap_or("").to_string(),
                                    result: tc["result"].as_str().map(|s| s.to_string()),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    self.push(Entry::Tool { owner: None, calls, expanded: false, open: false });
                }
                "error" => self.push(Entry::Error {
                    text: item["content"].as_str().unwrap_or("").to_string(),
                }),
                "notification" => self.push(Entry::Notice {
                    text: item["content"].as_str().unwrap_or("").to_string(),
                }),
                _ => {}
            }
        }
    }

    // --- palette ---

    pub fn refresh_palette(&mut self) {
        // Opening the palette (the first `/`) is the one moment worth paying a
        // directory scan for, so skill completions are always current.
        if self.input.text == "/" {
            self.skills = pet_core::skills::list_skills();
        }
        self.palette = palette_items(self.mode, &self.input.text, &self.skills);
        self.palette_sel = self.palette_sel.min(self.palette.len().saturating_sub(1));
    }

    pub fn palette_visible(&self) -> bool {
        !self.palette.is_empty() && !self.palette_dismissed
    }

    // --- scroll / selection ---

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
                let mut lines = wrap_lines(entry.lines(false), width);
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

    fn scroll_by(&mut self, delta: isize) {
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

    fn move_selection(&mut self, up: bool) {
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

    fn toggle_selected(&mut self) {
        if let Some(i) = self.sel_entry {
            if let Some(e) = self.entries.get_mut(i) {
                e.toggle();
            }
            self.touch(i);
            self.ensure_entry_visible(i);
        }
    }

    fn toggle_last(&mut self) {
        if let Some(i) = self.entries.iter().rposition(|e| e.toggleable()) {
            self.entries[i].toggle();
            self.touch(i);
        }
    }

    // --- chat-turn stream application ---

    fn apply_stream(&mut self, ev: StreamEvent) {
        match ev {
            StreamEvent::Chunk { text } => {
                self.close_open_tool();
                let appended = matches!(
                    self.entries.last(),
                    Some(Entry::Assistant { streaming: true, .. })
                );
                if appended {
                    if let Some(Entry::Assistant { text: t, .. }) = self.entries.last_mut() {
                        t.push_str(&text);
                    }
                } else {
                    let name = self.agent_name();
                    self.push(Entry::Assistant {
                        name,
                        text,
                        reasoning: String::new(),
                        streaming: true,
                        reasoning_expanded: false,
                    });
                }
                self.touch_last();
            }
            StreamEvent::Reasoning { text } => {
                let appended = matches!(
                    self.entries.last(),
                    Some(Entry::Assistant { streaming: true, .. })
                );
                if appended {
                    if let Some(Entry::Assistant { reasoning, .. }) = self.entries.last_mut() {
                        reasoning.push_str(&text);
                    }
                } else {
                    let name = self.agent_name();
                    self.push(Entry::Assistant {
                        name,
                        text: String::new(),
                        reasoning: text,
                        streaming: true,
                        reasoning_expanded: false,
                    });
                }
                self.touch_last();
            }
            StreamEvent::ToolStart { name, arguments } => {
                self.finish_streaming_assistant();
                let call = ToolCall { name, args: arguments, result: None };
                let appendable =
                    matches!(self.entries.last(), Some(Entry::Tool { open: true, .. }));
                if appendable {
                    if let Some(Entry::Tool { calls, .. }) = self.entries.last_mut() {
                        calls.push(call);
                    }
                } else {
                    self.push(Entry::Tool {
                        owner: None,
                        calls: vec![call],
                        expanded: false,
                        open: true,
                    });
                }
                self.touch_last();
            }
            StreamEvent::ToolResult { name, result } => {
                if let Some(Entry::Tool { calls, .. }) =
                    self.entries.iter_mut().rev().find(|e| matches!(e, Entry::Tool { owner: None, .. }))
                {
                    if let Some(c) =
                        calls.iter_mut().find(|c| c.name == name && c.result.is_none())
                    {
                        c.result = Some(result);
                    }
                }
                // The open tool entry is near the end; refreshing the last few
                // is cheap and correct even if text followed.
                let n = self.entries.len();
                for i in n.saturating_sub(3)..n {
                    self.touch(i);
                }
            }
            StreamEvent::Image { data_url } => {
                self.close_open_tool();
                self.push(Entry::Notice { text: format!("[图片 · {} bytes]", data_url.len()) });
            }
            StreamEvent::Usage { total_tokens, context_window, .. } => {
                self.context_usage = Some((total_tokens, context_window as u64));
            }
            StreamEvent::Done {} => {
                self.close_open_tool();
                self.finish_streaming_assistant();
                self.prune_empty_assistant();
            }
            StreamEvent::Error { message } => {
                self.close_open_tool();
                self.finish_streaming_assistant();
                self.prune_empty_assistant();
                self.push(Entry::Error { text: message });
            }
        }
    }

    fn close_open_tool(&mut self) {
        if let Some(Entry::Tool { open, .. }) = self.entries.last_mut() {
            *open = false;
        }
    }

    /// Commit the trailing streaming assistant entry (its text is final).
    fn finish_streaming_assistant(&mut self) {
        let committed = match self.entries.last_mut() {
            Some(Entry::Assistant { streaming, .. }) if *streaming => {
                *streaming = false;
                true
            }
            _ => false,
        };
        if committed {
            self.touch_last();
        }
    }

    /// Drop a trailing assistant entry that ended with nothing to show (e.g.
    /// the local placeholder when the turn failed before streaming).
    fn prune_empty_assistant(&mut self) {
        if matches!(
            self.entries.last(),
            Some(Entry::Assistant { text, reasoning, streaming: false, .. })
                if text.trim().is_empty() && reasoning.is_empty()
        ) {
            self.entries.pop();
            self.line_cache.pop();
        }
    }

    // --- group stream application ---

    fn group_roster(&self, agent_id: &str) -> (String, Color) {
        if let Ok(settings) = get_settings() {
            for (i, a) in settings.agents.iter().enumerate() {
                if a.id == agent_id {
                    return (a.name.clone(), agent_color(i));
                }
            }
        }
        (agent_id.to_string(), Color::Reset)
    }

    fn apply_group_stream(&mut self, agent_id: String, ev: StreamEvent) {
        if self.mode != Mode::Group {
            return; // muted outside the room (workers keep running)
        }
        match ev {
            StreamEvent::ToolStart { name, arguments } => {
                let call = ToolCall { name, args: arguments, result: None };
                let existing = self
                    .group_tools
                    .get(&agent_id)
                    .copied()
                    .filter(|&idx| matches!(self.entries.get(idx), Some(Entry::Tool { .. })));
                match existing {
                    Some(idx) => {
                        if let Some(Entry::Tool { calls, .. }) = self.entries.get_mut(idx) {
                            calls.push(call);
                        }
                        self.touch(idx);
                    }
                    None => {
                        let owner = self.group_roster(&agent_id);
                        self.push(Entry::Tool {
                            owner: Some(owner),
                            calls: vec![call],
                            expanded: false,
                            open: true,
                        });
                        self.group_tools.insert(agent_id, self.entries.len() - 1);
                    }
                }
            }
            StreamEvent::ToolResult { name, result } => {
                if let Some(&idx) = self.group_tools.get(&agent_id) {
                    if let Some(Entry::Tool { calls, .. }) = self.entries.get_mut(idx) {
                        if let Some(c) =
                            calls.iter_mut().find(|c| c.name == name && c.result.is_none())
                        {
                            c.result = Some(result);
                        }
                    }
                    self.touch(idx);
                }
            }
            StreamEvent::Error { message } => {
                let (name, _) = self.group_roster(&agent_id);
                self.push(Entry::Error { text: format!("{name}: {message}") });
            }
            _ => {}
        }
    }

    // --- event application (returns follow-up actions) ---

    pub fn apply(&mut self, ev: AppEvent) -> Vec<Action> {
        match ev {
            AppEvent::Term(TermEvent::Key(key)) => return self.on_key(key),
            AppEvent::Term(TermEvent::Resize(..)) => {}
            AppEvent::Term(_) => {}
            AppEvent::Stream(ev) => self.apply_stream(ev),
            AppEvent::TurnDone(result) => {
                self.busy = false;
                self.finish_streaming_assistant();
                self.prune_empty_assistant();
                if let Err(e) = result {
                    self.push(Entry::Error { text: e });
                }
                self.refresh_header();
                return self.drain_pending();
            }
            AppEvent::CommandDone => {
                self.busy = false;
                self.refresh_header();
                return self.drain_pending();
            }
            AppEvent::Notice(text) => self.push(Entry::Notice { text }),
            AppEvent::ErrorNotice(text) => self.push(Entry::Error { text }),
            AppEvent::SetMode(mode) => {
                self.mode = mode;
                self.sel_entry = None;
                self.refresh_palette();
                self.refresh_header();
            }
            AppEvent::GroupMsg(msg) => {
                if self.mode == Mode::Group {
                    let entry = match msg.agent_id.as_deref() {
                        Some(id) => {
                            let (name, color) = self.group_roster(id);
                            Entry::GroupMsg { name, color, text: msg.content }
                        }
                        None => Entry::GroupMsg {
                            name: msg.name,
                            color: Color::Green,
                            text: msg.content,
                        },
                    };
                    self.push(entry);
                }
            }
            AppEvent::GroupStream { agent_id, event } => self.apply_group_stream(agent_id, event),
            AppEvent::GroupAgentDone(agent_id) => {
                self.group_tools.remove(&agent_id);
            }
            AppEvent::TaskDone(c) => {
                let active = session::list_sessions().active_id;
                if !c.session_id.is_empty() && c.session_id != active {
                    let label = if c.label.is_empty() { c.kind.clone() } else { c.label.clone() };
                    self.push(Entry::Notice { text: format!("后台任务完成（其他会话）：{label}") });
                } else {
                    self.pending_tasks.push(c);
                    return self.drain_pending();
                }
            }
            AppEvent::OpenPicker(p) => self.picker = Some(p),
            AppEvent::ToolsCount(n) => self.tools_count = Some(n),
            AppEvent::Quit => return vec![Action::Quit],
        }
        vec![]
    }

    /// Start the next queued background-completion turn when idle.
    fn drain_pending(&mut self) -> Vec<Action> {
        if self.busy || self.pending_tasks.is_empty() {
            return vec![];
        }
        let c = self.pending_tasks.remove(0);
        let label = if c.label.is_empty() { c.kind.clone() } else { c.label.clone() };
        self.push(Entry::Notice { text: format!("后台任务完成：{label} — 自动继续对话") });
        self.busy = true;
        vec![Action::Turn(TurnInput::Completion(c))]
    }

    // --- key handling ---

    fn on_key(&mut self, key: KeyEvent) -> Vec<Action> {
        if key.kind != KeyEventKind::Press {
            return vec![];
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        // A modal picker captures navigation until closed.
        if let Some(p) = &mut self.picker {
            match (key.code, ctrl) {
                (KeyCode::Char('c'), true) | (KeyCode::Esc, _) => self.picker = None,
                (KeyCode::Up, _) => p.up(),
                (KeyCode::Down, _) => p.down(),
                (KeyCode::Char(' '), false) => p.toggle(),
                (KeyCode::Enter, _) => return self.confirm_picker(),
                _ => {}
            }
            return vec![];
        }

        match (key.code, ctrl) {
            (KeyCode::Char('c'), true) => {
                if self.input.text.is_empty() {
                    return vec![Action::Quit];
                }
                self.input.take();
                self.refresh_palette();
            }
            (KeyCode::Char('d'), true) => return vec![Action::Quit],
            (KeyCode::Char('a'), true) => self.input.cursor = 0,
            (KeyCode::Char('e'), true) => self.input.cursor = self.input.text.chars().count(),
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
            (KeyCode::Home, _) => self.input.cursor = 0,
            (KeyCode::End, _) => self.input.cursor = self.input.text.chars().count(),
            (KeyCode::PageUp, _) => self.scroll_by(-(self.last_height as isize - 1)),
            (KeyCode::PageDown, _) => self.scroll_by(self.last_height as isize - 1),
            (KeyCode::Up, _) => {
                if self.palette_visible() {
                    self.palette_sel = self.palette_sel.saturating_sub(1);
                } else if self.input.text.is_empty() {
                    self.move_selection(true);
                } else {
                    self.input.history_prev();
                }
            }
            (KeyCode::Down, _) => {
                if self.palette_visible() {
                    self.palette_sel = (self.palette_sel + 1).min(self.palette.len() - 1);
                } else if self.input.text.is_empty() {
                    self.move_selection(false);
                } else {
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

    /// Enter on a picker row: act on the selection.
    fn confirm_picker(&mut self) -> Vec<Action> {
        let Some(p) = self.picker.take() else { return vec![] };
        match p.kind {
            PickerKind::SwitchAgent => {
                if let Some(item) = p.items.get(p.sel) {
                    match pet_core::settings::set_active_agent(&item.id) {
                        Ok(()) => self.push(Entry::Notice {
                            text: format!("已切换到 {}", item.label),
                        }),
                        Err(e) => self.push(Entry::Error { text: e }),
                    }
                    self.refresh_header();
                }
            }
            PickerKind::SwitchSession => {
                if let Some(item) = p.items.get(p.sel) {
                    match session::set_active_session(item.id.clone()) {
                        Ok(()) => {
                            self.push(Entry::Notice {
                                text: format!("已切换到会话：{}", item.label),
                            });
                            self.replay_session(8);
                        }
                        Err(e) => self.push(Entry::Error { text: e }),
                    }
                    self.refresh_header();
                }
            }
            PickerKind::SwitchModel => {
                if let Some(item) = p.items.get(p.sel) {
                    let agent_id = get_settings()
                        .ok()
                        .and_then(|s| s.active_agent_config().map(|a| a.id.clone()));
                    match agent_id {
                        Some(id) => match pet_core::settings::set_agent_model(&id, &item.id) {
                            Ok(()) => self.push(Entry::Notice {
                                text: format!("模型已切换为 {}", item.label),
                            }),
                            Err(e) => self.push(Entry::Error { text: e }),
                        },
                        None => self.push(Entry::Error { text: "没有可用的 Agent".to_string() }),
                    }
                    self.refresh_header();
                }
            }
            PickerKind::Members => {
                self.busy = true;
                return vec![Action::SetMembers(p.checked_ids())];
            }
            PickerKind::ViewOnly => {}
        }
        vec![]
    }

    /// List commands open their picker overlay instead of dispatching (and
    /// instead of dumping output into the transcript).
    fn open_picker_for(&mut self, line: &str) -> Option<Vec<Action>> {
        match (self.mode, line) {
            (Mode::Chat, "/agents" | "/agent") => {
                match picker::agents_picker() {
                    Some(p) => self.picker = Some(p),
                    None => self.push(Entry::Notice { text: "还没有配置 Agent".to_string() }),
                }
                Some(vec![])
            }
            (Mode::Chat, "/sessions" | "/session") => {
                match picker::sessions_picker() {
                    Some(p) => self.picker = Some(p),
                    None => self.push(Entry::Notice {
                        text: "还没有会话（发送一条消息即可开始）".to_string(),
                    }),
                }
                Some(vec![])
            }
            (Mode::Chat, "/tasks") => Some(vec![Action::OpenTasks]),
            (Mode::Chat, "/models" | "/model") => Some(vec![Action::OpenModels]),
            (Mode::Group, "/members") => Some(vec![Action::OpenMembers]),
            _ => None,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
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

    #[test]
    fn stream_builds_assistant_then_tool_then_selection_toggles_it() {
        let mut a = app();
        a.apply(AppEvent::Stream(StreamEvent::Chunk { text: "让我看看".into() }));
        a.apply(AppEvent::Stream(StreamEvent::ToolStart {
            name: "bash".into(),
            arguments: "{}".into(),
        }));
        a.apply(AppEvent::Stream(StreamEvent::ToolResult {
            name: "bash".into(),
            result: "ok".into(),
        }));
        a.apply(AppEvent::Stream(StreamEvent::Done {}));
        assert_eq!(a.entries.len(), 2);
        assert!(matches!(a.entries[1], Entry::Tool { expanded: false, .. }));

        // Empty input: ↑ selects the tool entry, Enter expands it.
        a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Up))));
        assert_eq!(a.sel_entry, Some(1));
        a.apply(AppEvent::Term(TermEvent::Key(key(KeyCode::Enter))));
        assert!(matches!(a.entries[1], Entry::Tool { expanded: true, .. }));
    }

    #[test]
    fn completion_for_other_session_only_notices() {
        let mut a = app();
        let c = TaskCompletion {
            session_id: "some-other-session".into(),
            task_id: "t".into(),
            kind: "bash".into(),
            label: "sleep".into(),
            result: "done".into(),
        };
        let actions = a.apply(AppEvent::TaskDone(c));
        assert!(actions.is_empty());
        assert!(matches!(a.entries.last(), Some(Entry::Notice { .. })));
    }

    #[test]
    fn completion_queues_while_busy_and_resumes_after() {
        let mut a = app();
        a.busy = true;
        let c = TaskCompletion {
            session_id: String::new(),
            task_id: "t".into(),
            kind: "bash".into(),
            label: "build".into(),
            result: "ok".into(),
        };
        assert!(a.apply(AppEvent::TaskDone(c)).is_empty());
        // Turn ends → queued completion starts a resume turn.
        let actions = a.apply(AppEvent::TurnDone(Ok(())));
        assert!(matches!(actions.as_slice(), [Action::Turn(TurnInput::Completion(_))]));
        assert!(a.busy);
    }
}
