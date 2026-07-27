//! Chat-turn sinks. Both build the session's display items via the shared
//! `ItemBuilder` (so what the CLI persists matches the GUI); they differ in
//! where the live stream goes: `TuiSink` forwards events into the TUI loop,
//! `OneshotSink` prints plainly for `-p` mode.

use std::io::Write;
use std::sync::Mutex;

use pet_core::chat::{ChatEventSink, ItemBuilder, StreamEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::event::AppEvent;
use crate::ui;

/// What `run_chat_turn` needs back from a sink after the stream ends.
pub trait SessionSink {
    fn take_items(&self) -> Vec<serde_json::Value>;
    fn usage(&self) -> Option<(u64, u32)>;
}

#[derive(Default)]
struct SinkInner {
    items: ItemBuilder,
    usage: Option<(u64, u32)>,
}

// --- TUI sink ---------------------------------------------------------------

pub struct TuiSink {
    inner: Mutex<SinkInner>,
    tx: UnboundedSender<AppEvent>,
}

impl TuiSink {
    pub fn new(tx: UnboundedSender<AppEvent>) -> Self {
        Self { inner: Mutex::new(SinkInner::default()), tx }
    }

    fn emit(&self, ev: StreamEvent) {
        let _ = self.tx.send(AppEvent::Stream(ev));
    }
}

impl SessionSink for TuiSink {
    fn take_items(&self) -> Vec<serde_json::Value> {
        self.inner.lock().unwrap().items.take_items()
    }
    fn usage(&self) -> Option<(u64, u32)> {
        self.inner.lock().unwrap().usage
    }
}

impl ChatEventSink for TuiSink {
    fn send_chunk(&self, text: &str) {
        self.inner.lock().unwrap().items.chunk(text);
        self.emit(StreamEvent::Chunk { text: text.to_string() });
    }
    fn send_reasoning(&self, text: &str) {
        self.inner.lock().unwrap().items.reasoning(text);
        self.emit(StreamEvent::Reasoning { text: text.to_string() });
    }
    fn send_tool_start(&self, name: &str, arguments: &str) {
        self.inner.lock().unwrap().items.tool_start(name, arguments);
        self.emit(StreamEvent::ToolStart {
            name: name.to_string(),
            arguments: arguments.to_string(),
        });
    }
    fn send_tool_result(&self, name: &str, result: &str) {
        self.inner.lock().unwrap().items.tool_result(name, result);
        self.emit(StreamEvent::ToolResult { name: name.to_string(), result: result.to_string() });
    }
    fn send_image(&self, data_url: &str) {
        self.inner.lock().unwrap().items.image(data_url);
        self.emit(StreamEvent::Image { data_url: data_url.to_string() });
    }
    fn send_usage(&self, prompt_tokens: u64, total_tokens: u64, context_window: u32) {
        self.inner.lock().unwrap().usage = Some((total_tokens, context_window));
        self.emit(StreamEvent::Usage { prompt_tokens, total_tokens, context_window });
    }
    fn send_done(&self) {
        self.inner.lock().unwrap().items.done();
        self.emit(StreamEvent::Done {});
    }
    fn send_error(&self, message: &str) {
        self.inner.lock().unwrap().items.error(message);
        self.emit(StreamEvent::Error { message: message.to_string() });
    }
}

// --- One-shot (-p) sink ------------------------------------------------------

/// Plain streaming printer for `-p` mode (no TUI): answer text as-is,
/// reasoning dim, tools as one-liners.
pub struct OneshotSink {
    inner: Mutex<SinkInner>,
    /// True while the last printed text was dim reasoning.
    in_reasoning: Mutex<bool>,
}

impl OneshotSink {
    pub fn new() -> Self {
        Self { inner: Mutex::new(SinkInner::default()), in_reasoning: Mutex::new(false) }
    }

    fn flush() {
        let _ = std::io::stdout().flush();
    }

    fn end_reasoning(&self) {
        let mut in_r = self.in_reasoning.lock().unwrap();
        if *in_r {
            print!("{}\n", ui::RESET);
            *in_r = false;
        }
    }
}

impl SessionSink for OneshotSink {
    fn take_items(&self) -> Vec<serde_json::Value> {
        self.inner.lock().unwrap().items.take_items()
    }
    fn usage(&self) -> Option<(u64, u32)> {
        self.inner.lock().unwrap().usage
    }
}

impl ChatEventSink for OneshotSink {
    fn send_chunk(&self, text: &str) {
        self.inner.lock().unwrap().items.chunk(text);
        self.end_reasoning();
        print!("{text}");
        Self::flush();
    }
    fn send_reasoning(&self, text: &str) {
        self.inner.lock().unwrap().items.reasoning(text);
        let mut in_r = self.in_reasoning.lock().unwrap();
        if !*in_r {
            print!("{}", ui::DIM);
            *in_r = true;
        }
        print!("{text}");
        Self::flush();
    }
    fn send_tool_start(&self, name: &str, arguments: &str) {
        self.inner.lock().unwrap().items.tool_start(name, arguments);
        self.end_reasoning();
        println!("\n{}⚙ {}({}){}", ui::YELLOW, name, ui::one_line(arguments, 120), ui::RESET);
        Self::flush();
    }
    fn send_tool_result(&self, name: &str, result: &str) {
        self.inner.lock().unwrap().items.tool_result(name, result);
        println!("{}  ↳ {}{}", ui::DIM, ui::one_line(result, 160), ui::RESET);
        Self::flush();
    }
    fn send_image(&self, data_url: &str) {
        self.inner.lock().unwrap().items.image(data_url);
        println!("{}[图片 · {} bytes]{}", ui::DIM, data_url.len(), ui::RESET);
    }
    fn send_usage(&self, _prompt_tokens: u64, total_tokens: u64, context_window: u32) {
        self.inner.lock().unwrap().usage = Some((total_tokens, context_window));
    }
    fn send_done(&self) {
        self.inner.lock().unwrap().items.done();
        self.end_reasoning();
        println!();
        Self::flush();
    }
    fn send_error(&self, message: &str) {
        self.inner.lock().unwrap().items.error(message);
        self.end_reasoning();
        println!("{}✗ {}{}", ui::RED, message, ui::RESET);
    }
}
