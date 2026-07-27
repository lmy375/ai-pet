//! Transcript entries and their rendering to styled (unwrapped) lines.
//!
//! Tool calls and reasoning render collapsed by default and can be toggled
//! open (↑/↓ selects a toggleable entry when the input is empty; Enter
//! toggles). Assistant/group message bodies render as markdown.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::markdown::render_markdown;
use crate::ui::one_line;

pub fn agent_color(idx: usize) -> Color {
    const COLORS: [Color; 6] =
        [Color::Cyan, Color::Magenta, Color::Green, Color::Yellow, Color::Blue, Color::Red];
    COLORS[idx % COLORS.len()]
}

pub struct ToolCall {
    pub name: String,
    pub args: String,
    pub result: Option<String>,
}

pub enum Entry {
    User {
        text: String,
    },
    Assistant {
        name: String,
        text: String,
        reasoning: String,
        streaming: bool,
        reasoning_expanded: bool,
    },
    Tool {
        /// `(name, color)` of the group agent this belongs to; `None` in chat.
        owner: Option<(String, Color)>,
        calls: Vec<ToolCall>,
        expanded: bool,
        /// Still accepting calls from the current round.
        open: bool,
    },
    GroupMsg {
        name: String,
        color: Color,
        text: String,
    },
    Notice {
        text: String,
    },
    Error {
        text: String,
    },
}

const DIM: Style = Style::new().add_modifier(Modifier::DIM);

impl Entry {
    /// Entries the selection cursor can land on (Enter toggles them).
    pub fn toggleable(&self) -> bool {
        match self {
            Entry::Tool { .. } => true,
            Entry::Assistant { reasoning, streaming, .. } => {
                !reasoning.is_empty() && !streaming
            }
            _ => false,
        }
    }

    pub fn toggle(&mut self) {
        match self {
            Entry::Tool { expanded, .. } => *expanded = !*expanded,
            Entry::Assistant { reasoning_expanded, .. } => {
                *reasoning_expanded = !*reasoning_expanded
            }
            _ => {}
        }
    }

    /// Unwrapped styled lines. `selected` highlights the header row.
    pub fn lines(&self, selected: bool) -> Vec<Line<'static>> {
        let mut out = match self {
            Entry::User { text } => {
                let mut lines = Vec::new();
                for (i, part) in text.lines().enumerate() {
                    let prefix = if i == 0 { "❯ " } else { "  " };
                    lines.push(Line::from(vec![
                        Span::styled(
                            prefix,
                            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(part.to_string(), Style::default().add_modifier(Modifier::BOLD)),
                    ]));
                }
                if lines.is_empty() {
                    lines.push(Line::from(Span::styled("❯ ", Style::default().fg(Color::Green))));
                }
                lines
            }

            Entry::Assistant { name, text, reasoning, streaming, reasoning_expanded } => {
                let mut lines = vec![Line::from(Span::styled(
                    format!("● {name}"),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ))];
                if !reasoning.is_empty() {
                    if *streaming || *reasoning_expanded {
                        let marker = if *streaming { "▾ 思考中…" } else { "▾ 思考过程" };
                        lines.push(Line::from(Span::styled(marker.to_string(), DIM)));
                        for part in reasoning.lines() {
                            lines.push(Line::from(Span::styled(format!("│ {part}"), DIM)));
                        }
                    } else {
                        lines.push(Line::from(Span::styled(
                            format!("▸ 思考过程（{} 字）", reasoning.chars().count()),
                            DIM,
                        )));
                    }
                }
                if text.is_empty() && *streaming {
                    lines.push(Line::from(Span::styled("…", DIM)));
                } else {
                    lines.extend(render_markdown(text));
                }
                lines
            }

            Entry::Tool { owner, calls, expanded, .. } => {
                let arrow = if *expanded { "▾" } else { "▸" };
                let done = calls.iter().filter(|c| c.result.is_some()).count();
                let status = if done == calls.len() { String::new() } else { format!(" · 运行中") };
                let mut header = vec![Span::styled(
                    format!("{arrow} 工具调用 ×{}{status}", calls.len()),
                    Style::default().fg(Color::Yellow),
                )];
                if let Some((name, color)) = owner {
                    header.insert(
                        0,
                        Span::styled(
                            format!("{name} "),
                            Style::default().fg(*color).add_modifier(Modifier::BOLD),
                        ),
                    );
                }
                let mut lines = vec![Line::from(header)];

                for call in calls {
                    let mark = if call.result.is_some() { "✓" } else { "…" };
                    if *expanded {
                        lines.push(Line::from(vec![
                            Span::styled("  ⚙ ", Style::default().fg(Color::Yellow)),
                            Span::styled(
                                call.name.clone(),
                                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(format!(" {mark}"), DIM),
                        ]));
                        for part in clipped(&call.args, 8) {
                            lines.push(Line::from(Span::styled(format!("    {part}"), DIM)));
                        }
                        if let Some(result) = &call.result {
                            lines.push(Line::from(Span::styled("    ── 结果 ──".to_string(), DIM)));
                            for part in clipped(result, 20) {
                                lines.push(Line::from(Span::styled(format!("    {part}"), DIM)));
                            }
                        }
                    } else {
                        lines.push(Line::from(vec![
                            Span::styled(format!("  ⚙ {}(", call.name), DIM),
                            Span::styled(one_line(&call.args, 60), DIM),
                            Span::styled(format!(") {mark}"), DIM),
                        ]));
                    }
                }
                lines
            }

            Entry::GroupMsg { name, color, text } => {
                let mut lines = vec![Line::from(Span::styled(
                    format!("● {name}"),
                    Style::default().fg(*color).add_modifier(Modifier::BOLD),
                ))];
                lines.extend(render_markdown(text));
                lines
            }

            Entry::Notice { text } => {
                text.lines().map(|p| Line::from(Span::styled(p.to_string(), DIM))).collect()
            }

            Entry::Error { text } => vec![Line::from(Span::styled(
                format!("✗ {text}"),
                Style::default().fg(Color::Red),
            ))],
        };

        if selected {
            if let Some(first) = out.first_mut() {
                for span in &mut first.spans {
                    span.style = span.style.add_modifier(Modifier::REVERSED);
                }
            }
        }
        out
    }
}

/// First `max` lines of `s`, with a dim "(+n 行)" tail when clipped.
fn clipped(s: &str, max: usize) -> Vec<String> {
    let total = s.lines().count();
    let mut out: Vec<String> = s.lines().take(max).map(|l| l.to_string()).collect();
    if total > max {
        out.push(format!("… (+{} 行)", total - max));
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &[Line]) -> Vec<String> {
        lines.iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect()).collect()
    }

    fn call(result: Option<&str>) -> ToolCall {
        ToolCall {
            name: "bash".into(),
            args: "{\"command\":\"ls -la\"}".into(),
            result: result.map(|s| s.to_string()),
        }
    }

    #[test]
    fn tool_entry_collapses_to_one_line_per_call_and_expands_to_details() {
        let mut e = Entry::Tool {
            owner: None,
            calls: vec![call(Some("total 0\nfile_a\nfile_b"))],
            expanded: false,
            open: false,
        };
        let collapsed = texts(&e.lines(false));
        // Header + one summary line; the result body is hidden.
        assert_eq!(collapsed.len(), 2);
        assert!(collapsed[1].contains("bash("));
        assert!(!collapsed.iter().any(|t| t.contains("file_a")));

        e.toggle();
        let expanded = texts(&e.lines(false));
        assert!(expanded.iter().any(|t| t.contains("file_a")));
        assert!(expanded.iter().any(|t| t.contains("── 结果 ──")));
    }

    #[test]
    fn reasoning_collapses_after_streaming_and_toggles_open() {
        let mut e = Entry::Assistant {
            name: "宠".into(),
            text: "答案".into(),
            reasoning: "第一步\n第二步".into(),
            streaming: false,
            reasoning_expanded: false,
        };
        assert!(e.toggleable());
        let collapsed = texts(&e.lines(false));
        assert!(collapsed.iter().any(|t| t.contains("▸ 思考过程")));
        assert!(!collapsed.iter().any(|t| t.contains("第一步")));

        e.toggle();
        let expanded = texts(&e.lines(false));
        assert!(expanded.iter().any(|t| t.contains("第一步")));
    }

    #[test]
    fn selected_entry_reverses_its_header_row() {
        let e = Entry::Tool { owner: None, calls: vec![call(None)], expanded: false, open: true };
        let lines = e.lines(true);
        assert!(lines[0].spans.iter().all(|s| s.style.add_modifier.contains(Modifier::REVERSED)));
        assert!(lines[1].spans.iter().all(|s| !s.style.add_modifier.contains(Modifier::REVERSED)));
    }

    #[test]
    fn assistant_body_renders_markdown() {
        let e = Entry::Assistant {
            name: "宠".into(),
            text: "**加粗** 正常".into(),
            reasoning: String::new(),
            streaming: false,
            reasoning_expanded: false,
        };
        let lines = e.lines(false);
        let body = &lines[1];
        let bold = body.spans.iter().find(|s| s.content == "加粗").unwrap();
        assert!(bold.style.add_modifier.contains(Modifier::BOLD));
    }
}
