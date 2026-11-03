//! The TUI state: transcript, input line, palette, selection and scroll.

use std::collections::HashMap;

use pet_core::skills::Skill;
use ratatui::style::Color;
use ratatui::text::Line;

use crate::Mode;
use super::entries::Entry;
use super::InputState;
use super::palette::{palette_items, PaletteItem};
use super::picker::Picker;

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
    pub(super) palette_dismissed: bool,
    /// Skills backing the `/skill:<slug>` palette entries — which double as the
    /// only skills listing the CLI has. Re-scanned each time the palette opens
    /// (see `refresh_palette`), so a newly added skill is completable without
    /// restarting — one `read_dir`, not one per keystroke.
    pub(super) skills: Vec<Skill>,

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
    pub(super) line_cache: Vec<Vec<Line<'static>>>,
    pub(super) cache_width: usize,

    /// Terminal reports Shift+Enter distinctly (kitty keyboard protocol pushed
    /// in main.rs). Only affects which newline chord the status bar advertises —
    /// Ctrl+J always works.
    pub shift_enter: bool,

    // Group: agent_id → index of its currently-open Tool entry.
    pub(super) group_tools: HashMap<String, usize>,
    /// The configured agents (id, name, colour), cached by `refresh_header` so
    /// group tool events don't re-read settings per event.
    pub(super) agents: Vec<(String, String, Color)>,
    /// The session this view shows (the index's active id, cached by
    /// `refresh_header` so per-token turn notices don't hit the disk).
    pub session_id: String,
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
            shift_enter: false,
            group_tools: HashMap::new(),
            agents: Vec::new(),
            session_id: String::new(),
        };
        app.refresh_header();
        app.replay_session(8);
        app.push(Entry::Notice {
            text: "输入 / 弹出命令面板；/help 查看用法。与桌面 GUI 共享配置与会话。".to_string(),
        });
        app
    }

    // --- entry plumbing ---

    pub(super) fn push(&mut self, e: Entry) {
        self.entries.push(e);
        self.line_cache.push(Vec::new());
    }

    pub(super) fn touch(&mut self, idx: usize) {
        if let Some(c) = self.line_cache.get_mut(idx) {
            c.clear();
        }
    }

    pub(super) fn touch_last(&mut self) {
        if !self.entries.is_empty() {
            let i = self.entries.len() - 1;
            self.touch(i);
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
}