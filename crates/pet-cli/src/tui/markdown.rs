//! Markdown → styled ratatui lines (headings, bold/italic, inline code, fenced
//! code blocks, lists, quotes, links, rules). Wrapping happens later in
//! `wrap.rs`; this module only decides content and styles.
//!
//! Start/End tags are tracked with a plain stack — markdown events are
//! balanced, so every `End` pops whatever was last opened. That keeps the
//! renderer independent of the exact `TagEnd` shapes across pulldown-cmark
//! versions.

use pulldown_cmark::{Event as MdEvent, Options, Parser, Tag};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

#[derive(Clone, Copy, PartialEq)]
enum Block {
    Paragraph,
    Heading,
    CodeBlock,
    List,
    Item,
    Quote,
    Strong,
    Emphasis,
    Link,
    Other,
}

#[derive(Default)]
struct Renderer {
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    stack: Vec<Block>,
    /// Bullet/number counters per nesting level; `None` = unordered.
    lists: Vec<Option<u64>>,
    /// Pending list-item marker for the next text of this item.
    item_marker: Option<String>,
    link_url: Option<String>,
}

pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    let mut r = Renderer::default();
    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    for ev in Parser::new_ext(text, opts) {
        r.event(ev);
    }
    r.flush();
    while r.lines.last().is_some_and(|l| l.spans.is_empty()) {
        r.lines.pop();
    }
    r.lines
}

impl Renderer {
    fn in_block(&self, b: Block) -> bool {
        self.stack.contains(&b)
    }

    fn style(&self) -> Style {
        let mut s = Style::default();
        if self.in_block(Block::Heading) {
            s = s.fg(Color::Cyan).add_modifier(Modifier::BOLD);
        }
        if self.in_block(Block::CodeBlock) {
            s = s.fg(Color::Green);
        }
        if self.in_block(Block::Quote) {
            s = s.add_modifier(Modifier::DIM);
        }
        if self.in_block(Block::Strong) {
            s = s.add_modifier(Modifier::BOLD);
        }
        if self.in_block(Block::Emphasis) {
            s = s.add_modifier(Modifier::ITALIC);
        }
        if self.in_block(Block::Link) {
            s = s.add_modifier(Modifier::UNDERLINED);
        }
        s
    }

    fn flush(&mut self) {
        if !self.spans.is_empty() {
            self.lines.push(Line::from(std::mem::take(&mut self.spans)));
        }
    }

    /// Blank separator before a new top-level block (never two in a row).
    fn blank_before_block(&mut self) {
        self.flush();
        if self.lines.last().is_some_and(|l| !l.spans.is_empty()) {
            self.lines.push(Line::from(""));
        }
    }

    /// Prefixes owed at the start of a fresh visual line (quote bar, list marker).
    fn line_prefix(&mut self) {
        if !self.spans.is_empty() {
            return;
        }
        if self.in_block(Block::Quote) {
            self.spans.push(Span::styled("│ ", Style::default().add_modifier(Modifier::DIM)));
        }
        if let Some(marker) = self.item_marker.take() {
            self.spans.push(Span::styled(marker, Style::default().fg(Color::Cyan)));
        } else if self.in_block(Block::Item) {
            // Continuation content inside an item aligns under the marker.
            let depth = self.lists.len().max(1);
            self.spans.push(Span::raw("  ".repeat(depth)));
        }
    }

    fn push_text(&mut self, text: &str, style: Style) {
        for (i, part) in text.split('\n').enumerate() {
            if i > 0 {
                self.flush();
            }
            if part.is_empty() {
                continue;
            }
            self.line_prefix();
            self.spans.push(Span::styled(part.to_string(), style));
        }
    }

    fn event(&mut self, ev: MdEvent) {
        match ev {
            MdEvent::Start(tag) => {
                let block = match tag {
                    Tag::Paragraph => {
                        if !self.in_block(Block::Item) {
                            self.blank_before_block();
                        }
                        Block::Paragraph
                    }
                    Tag::Heading { .. } => {
                        self.blank_before_block();
                        Block::Heading
                    }
                    Tag::CodeBlock(_) => {
                        self.blank_before_block();
                        Block::CodeBlock
                    }
                    Tag::List(start) => {
                        if self.lists.is_empty() {
                            self.blank_before_block();
                        } else {
                            self.flush();
                        }
                        self.lists.push(start);
                        Block::List
                    }
                    Tag::Item => {
                        self.flush();
                        let depth = self.lists.len();
                        let indent = "  ".repeat(depth.saturating_sub(1));
                        let marker = match self.lists.last_mut() {
                            Some(Some(n)) => {
                                let m = format!("{indent}{n}. ");
                                *n += 1;
                                m
                            }
                            _ => format!("{indent}• "),
                        };
                        self.item_marker = Some(marker);
                        Block::Item
                    }
                    Tag::BlockQuote(_) => {
                        self.blank_before_block();
                        Block::Quote
                    }
                    Tag::Strong => Block::Strong,
                    Tag::Emphasis => Block::Emphasis,
                    Tag::Link { dest_url, .. } => {
                        self.link_url = Some(dest_url.to_string());
                        Block::Link
                    }
                    _ => Block::Other,
                };
                self.stack.push(block);
            }
            MdEvent::End(_) => {
                match self.stack.pop() {
                    Some(Block::Link) => {
                        if let Some(url) = self.link_url.take() {
                            self.spans.push(Span::styled(
                                format!(" ({url})"),
                                Style::default().add_modifier(Modifier::DIM),
                            ));
                        }
                    }
                    Some(Block::List) => {
                        self.lists.pop();
                        self.flush();
                    }
                    Some(Block::Paragraph | Block::Heading | Block::CodeBlock | Block::Item | Block::Quote) => {
                        self.flush();
                    }
                    _ => {}
                }
            }
            MdEvent::Text(t) => {
                let style = self.style();
                if self.in_block(Block::CodeBlock) {
                    // Code renders verbatim (no markers), indented two cells.
                    for part in t.split('\n') {
                        if part.is_empty() && t.ends_with('\n') {
                            // trailing newline of the block — handled by split
                        }
                        self.line_prefix();
                        self.spans.push(Span::styled(format!("  {part}"), style));
                        self.flush();
                    }
                    // `split` yields a final "" for trailing newline; drop the
                    // empty line it produced.
                    if t.ends_with('\n') && self.lines.last().is_some_and(|l| {
                        l.spans.len() == 1 && l.spans[0].content.trim().is_empty()
                    }) {
                        self.lines.pop();
                    }
                } else {
                    self.push_text(&t, style);
                }
            }
            MdEvent::Code(t) => {
                self.line_prefix();
                self.spans.push(Span::styled(t.to_string(), Style::default().fg(Color::Yellow)));
            }
            MdEvent::SoftBreak | MdEvent::HardBreak => self.flush(),
            MdEvent::Rule => {
                self.blank_before_block();
                self.lines.push(Line::from(Span::styled(
                    "─".repeat(24),
                    Style::default().add_modifier(Modifier::DIM),
                )));
            }
            MdEvent::TaskListMarker(done) => {
                self.line_prefix();
                self.spans.push(Span::raw(if done { "[x] " } else { "[ ] " }));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;

    fn text_of(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn heading_is_bold_and_body_separated_by_blank() {
        let lines = render_markdown("# 标题\n\n正文");
        assert_eq!(text_of(&lines[0]), "标题");
        assert!(lines[0].spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(text_of(&lines[1]), "");
        assert_eq!(text_of(&lines[2]), "正文");
    }

    #[test]
    fn fenced_code_block_renders_verbatim_lines() {
        let lines = render_markdown("说明\n```rust\nlet a = 1;\nlet b = 2;\n```");
        let texts: Vec<String> = lines.iter().map(text_of).collect();
        assert!(texts.contains(&"  let a = 1;".to_string()));
        assert!(texts.contains(&"  let b = 2;".to_string()));
        // The fence markers themselves never appear.
        assert!(!texts.iter().any(|t| t.contains("```")));
    }

    #[test]
    fn inline_styles_apply_to_their_spans_only() {
        let lines = render_markdown("a **b** `c`");
        let l = &lines[0];
        let bold = l.spans.iter().find(|s| s.content == "b").unwrap();
        assert!(bold.style.add_modifier.contains(Modifier::BOLD));
        let code = l.spans.iter().find(|s| s.content == "c").unwrap();
        assert_eq!(code.style.fg, Some(Color::Yellow));
        let plain = l.spans.iter().find(|s| s.content.contains('a')).unwrap();
        assert!(!plain.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn ordered_list_numbers_increment() {
        let lines = render_markdown("1. one\n2. two");
        let texts: Vec<String> = lines.iter().map(text_of).collect();
        assert!(texts.iter().any(|t| t.starts_with("1. one")));
        assert!(texts.iter().any(|t| t.starts_with("2. two")));
    }
}
