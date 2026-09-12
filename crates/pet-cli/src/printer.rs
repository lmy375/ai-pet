//! Plain streaming printer for `-p` mode (no TUI): answer text as-is,
//! reasoning dim, tools as one-liners. Display only — the turn runner persists
//! the session, so nothing is accumulated here.

use std::io::Write;

use pet_core::chat::StreamEvent;

use crate::ui;

#[derive(Default)]
pub struct OneshotPrinter {
    /// True while the last printed text was dim reasoning.
    in_reasoning: bool,
}

impl OneshotPrinter {
    pub fn new() -> Self {
        Self::default()
    }

    fn flush() {
        let _ = std::io::stdout().flush();
    }

    fn end_reasoning(&mut self) {
        if self.in_reasoning {
            println!("{}", ui::RESET);
            self.in_reasoning = false;
        }
    }

    pub fn apply(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::Chunk { text } => {
                self.end_reasoning();
                print!("{text}");
            }
            StreamEvent::Reasoning { text } => {
                if !self.in_reasoning {
                    print!("{}", ui::DIM);
                    self.in_reasoning = true;
                }
                print!("{text}");
            }
            StreamEvent::ToolStart { name, arguments } => {
                self.end_reasoning();
                println!("\n{}⚙ {}({}){}", ui::YELLOW, name, ui::one_line(arguments, 120), ui::RESET);
            }
            StreamEvent::ToolResult { result, .. } => {
                println!("{}  ↳ {}{}", ui::DIM, ui::one_line(result, 160), ui::RESET);
            }
            StreamEvent::Image { data_url } => {
                println!("{}[图片 · {} bytes]{}", ui::DIM, data_url.len(), ui::RESET);
            }
            StreamEvent::Usage { .. } => {}
            StreamEvent::Done {} => {
                self.end_reasoning();
                println!();
            }
            StreamEvent::Error { message } => {
                self.end_reasoning();
                println!("{}✗ {}{}", ui::RED, message, ui::RESET);
            }
        }
        Self::flush();
    }
}
