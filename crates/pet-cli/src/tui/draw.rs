//! Rendering: header / transcript / status / input, plus the command palette
//! popup anchored above the input box.

use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

use super::wrap::str_width;
use super::TuiApp;
use crate::Mode;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const DIM: Style = Style::new().add_modifier(Modifier::DIM);

pub fn draw(f: &mut Frame, app: &mut TuiApp) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(3),
    ])
    .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_transcript(f, app, chunks[1]);
    draw_status(f, app, chunks[2]);
    draw_input(f, app, chunks[3]);
    if app.picker.is_some() {
        draw_picker(f, app, chunks[3]);
    } else if app.palette_visible() {
        draw_palette(f, app, chunks[3]);
    }
}

/// Modal list overlay (/agents, /sessions, /tasks, /members), anchored above
/// the input box like the palette.
fn draw_picker(f: &mut Frame, app: &TuiApp, input_area: Rect) {
    const MAX_ROWS: usize = 10;
    let Some(picker) = &app.picker else { return };
    let rows = picker.items.len().clamp(1, MAX_ROWS);
    let height = rows as u16 + 2;
    if input_area.y < height {
        return;
    }
    let area = Rect { x: input_area.x, y: input_area.y - height, width: input_area.width, height };
    f.render_widget(Clear, area);

    let first = picker.sel.saturating_sub(rows.saturating_sub(1));
    let label_w = picker.items.iter().map(|i| str_width(&i.label)).max().unwrap_or(0).min(40);

    let mut lines: Vec<Line> = Vec::with_capacity(rows);
    for (i, item) in picker.items.iter().enumerate().skip(first).take(rows) {
        let mark = match item.checked {
            Some(true) => "● ",
            Some(false) => "○ ",
            None => " ",
        };
        let pad = label_w.saturating_sub(str_width(&item.label));
        let mut spans = vec![
            Span::styled(
                format!("{mark}{}{} ", item.label, " ".repeat(pad)),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(item.desc.clone(), DIM),
        ];
        if i == picker.sel {
            for s in &mut spans {
                s.style = s.style.add_modifier(Modifier::REVERSED).remove_modifier(Modifier::DIM);
            }
        }
        lines.push(Line::from(spans));
    }

    let block = Block::bordered()
        .title(picker.title.clone())
        .border_style(Style::default().fg(Color::Yellow));
    f.render_widget(Paragraph::new(Text::from(lines)).block(block), area);
}

fn draw_header(f: &mut Frame, app: &TuiApp, area: Rect) {
    let style = Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED);
    f.render_widget(
        Paragraph::new(format!("{:<width$}", app.header, width = area.width as usize)).style(style),
        area,
    );
}

fn draw_transcript(f: &mut Frame, app: &mut TuiApp, area: Rect) {
    let inner = area.inner(ratatui::layout::Margin { horizontal: 1, vertical: 0 });
    let lines = app.visible_lines(inner.width as usize, inner.height as usize);
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Compact token count: 812 → "812", 43210 → "43.2k".
fn fmt_tokens(n: u64) -> String {
    if n < 1000 {
        n.to_string()
    } else {
        format!("{:.1}k", n as f64 / 1000.0)
    }
}

fn draw_status(f: &mut Frame, app: &TuiApp, area: Rect) {
    let mut left: Vec<Span> = Vec::new();
    if app.busy {
        left.push(Span::styled(
            format!(" {} 处理中…", SPINNER[app.spin % SPINNER.len()]),
            Style::default().fg(Color::Yellow),
        ));
    } else {
        let mode = match app.mode {
            Mode::Chat => " 单聊",
            Mode::Group => " 群聊",
        };
        left.push(Span::styled(mode, Style::default().fg(Color::Cyan)));
    }

    // Agent · model · context occupancy · tool count.
    left.push(Span::styled(
        format!(" · {}", app.agent_name_cached),
        Style::default().add_modifier(Modifier::BOLD),
    ));
    left.push(Span::styled(format!(" · {}", app.model), Style::default().fg(Color::Cyan)));
    match app.context_usage {
        Some((used, total)) if total > 0 => {
            let pct = (used as f64 / total as f64 * 100.0).round() as u64;
            left.push(Span::raw(format!(
                " · ctx {}/{} ({pct}%)",
                fmt_tokens(used),
                fmt_tokens(total)
            )));
        }
        _ => left.push(Span::styled(" · ctx –", DIM)),
    }
    match app.tools_count {
        Some(n) => left.push(Span::raw(format!(" · {n} tools"))),
        None => left.push(Span::styled(" · … tools", DIM)),
    }
    if let Some(i) = app.sel_entry {
        left.push(Span::styled(format!("  · 已选中 #{i}（Enter 展开/收起）"), DIM));
    }

    let right_text = "/ 命令 · Ctrl+C 退出 ";
    let right_w = str_width(right_text) as u16;
    let cols = Layout::horizontal([Constraint::Min(0), Constraint::Length(right_w.min(area.width))])
        .split(area);
    f.render_widget(Paragraph::new(Line::from(left)), cols[0]);
    f.render_widget(Paragraph::new(right_text).style(DIM), cols[1]);
}

fn draw_input(f: &mut Frame, app: &TuiApp, area: Rect) {
    let border_style = if app.busy {
        Style::default().add_modifier(Modifier::DIM)
    } else {
        Style::default().fg(Color::Cyan)
    };
    let block = Block::bordered().border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let prompt = match app.mode {
        Mode::Chat => "❯ ",
        Mode::Group => "群 ❯ ",
    };
    let prompt_w = str_width(prompt);
    let avail = (inner.width as usize).saturating_sub(prompt_w + 1);

    // Horizontal window so the cursor stays visible on long input.
    let before_cursor: String = app.input.text.chars().take(app.input.cursor).collect();
    let mut start_char = 0usize;
    while str_width(&before_cursor[byte_of(&before_cursor, start_char)..]) > avail {
        start_char += 1;
    }
    let visible: String = app.input.text.chars().skip(start_char).collect();

    let line = Line::from(vec![
        Span::styled(prompt, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::raw(visible),
    ]);
    f.render_widget(Paragraph::new(line), inner);

    let cursor_x = prompt_w
        + str_width(&app.input.text.chars().skip(start_char).take(app.input.cursor - start_char).collect::<String>());
    f.set_cursor_position(Position::new(inner.x + cursor_x as u16, inner.y));
}

fn byte_of(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(i, _)| i).unwrap_or(s.len())
}

fn draw_palette(f: &mut Frame, app: &TuiApp, input_area: Rect) {
    const MAX_ROWS: usize = 8;
    let rows = app.palette.len().min(MAX_ROWS);
    let height = rows as u16 + 2;
    if input_area.y < height {
        return;
    }
    let area = Rect {
        x: input_area.x,
        y: input_area.y - height,
        width: input_area.width,
        height,
    };
    f.render_widget(Clear, area);

    // Keep the selected row inside the window.
    let first = app.palette_sel.saturating_sub(rows.saturating_sub(1));
    let label_w = app
        .palette
        .iter()
        .map(|i| str_width(&i.label))
        .max()
        .unwrap_or(0)
        .min(40);

    let mut lines: Vec<Line> = Vec::with_capacity(rows);
    for (i, item) in app.palette.iter().enumerate().skip(first).take(rows) {
        let pad = label_w.saturating_sub(str_width(&item.label));
        let mut spans = vec![
            Span::styled(
                format!(" {}{} ", item.label, " ".repeat(pad)),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(item.desc.clone(), DIM),
        ];
        if i == app.palette_sel {
            for s in &mut spans {
                s.style = s.style.add_modifier(Modifier::REVERSED).remove_modifier(Modifier::DIM);
            }
        }
        lines.push(Line::from(spans));
    }

    let block = Block::bordered()
        .title(" 命令（↑↓ 选择 · Enter/Tab 确认 · Esc 关闭）")
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(Paragraph::new(Text::from(lines)).block(block), area);
}
