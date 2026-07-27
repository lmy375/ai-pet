//! Width-aware wrapping of styled lines. ratatui's own `Wrap` hides how many
//! rows a paragraph occupies, which the scrollback needs to know exactly — so
//! the transcript pre-wraps every line itself: word-aware for latin text,
//! width-2-aware for CJK, hard-splitting tokens wider than the viewport.

use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

fn char_width(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}

pub fn str_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

/// A wrappable unit: a whitespace char, one wide (CJK) char, or a latin word.
fn tokens(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut word_start: Option<usize> = None;
    for (i, c) in s.char_indices() {
        if c.is_whitespace() || char_width(c) > 1 {
            if let Some(ws) = word_start.take() {
                out.push(&s[ws..i]);
            }
            out.push(&s[i..i + c.len_utf8()]);
        } else if word_start.is_none() {
            word_start = Some(i);
        }
    }
    if let Some(ws) = word_start {
        out.push(&s[ws..]);
    }
    out
}

pub fn wrap_lines(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    lines.into_iter().flat_map(|l| wrap_line(l, width)).collect()
}

fn wrap_line(line: Line<'static>, width: usize) -> Vec<Line<'static>> {
    let width = width.max(4);
    if line.spans.iter().map(|s| str_width(&s.content)).sum::<usize>() <= width {
        return vec![line];
    }

    let mut out: Vec<Line<'static>> = Vec::new();
    let mut cur: Vec<Span<'static>> = Vec::new();
    let mut cur_w = 0usize;

    let mut flush = |cur: &mut Vec<Span<'static>>, cur_w: &mut usize| {
        out.push(Line::from(std::mem::take(cur)));
        *cur_w = 0;
    };

    for span in line.spans {
        let style = span.style;
        let mut buf = String::new();
        let push_buf = |buf: &mut String, cur: &mut Vec<Span<'static>>| {
            if !buf.is_empty() {
                cur.push(Span::styled(std::mem::take(buf), style));
            }
        };

        for tok in tokens(&span.content) {
            let tw = str_width(tok);
            if cur_w + tw > width && cur_w > 0 {
                // Doesn't fit on this row: wrap, trimming the row's trailing
                // whitespace and dropping the leading space of the next one.
                while buf.ends_with(char::is_whitespace) {
                    buf.pop();
                }
                push_buf(&mut buf, &mut cur);
                flush(&mut cur, &mut cur_w);
                if tok.chars().all(|c| c.is_whitespace()) {
                    continue;
                }
            }
            if tw > width {
                // A single token wider than the viewport: hard-split by char.
                for c in tok.chars() {
                    let cw = char_width(c);
                    if cur_w + cw > width {
                        push_buf(&mut buf, &mut cur);
                        flush(&mut cur, &mut cur_w);
                    }
                    buf.push(c);
                    cur_w += cw;
                }
            } else {
                buf.push_str(tok);
                cur_w += tw;
            }
        }
        push_buf(&mut buf, &mut cur);
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(Line::from(cur));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Style};

    fn text_of(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn cjk_wraps_by_display_width_not_char_count() {
        // 6 CJK chars = display width 12; at width 8 only 4 fit per row.
        let lines = wrap_line(Line::from("六个中文字符"), 8);
        assert_eq!(lines.iter().map(text_of).collect::<Vec<_>>(), vec!["六个中文", "字符"]);
    }

    #[test]
    fn latin_wraps_at_word_boundaries() {
        let lines = wrap_line(Line::from("hello brave new world"), 12);
        assert_eq!(
            lines.iter().map(text_of).collect::<Vec<_>>(),
            vec!["hello brave", "new world"]
        );
    }

    #[test]
    fn oversized_token_hard_splits_and_style_survives() {
        let style = Style::default().fg(Color::Yellow);
        let lines = wrap_line(Line::from(ratatui::text::Span::styled("abcdefghij", style)), 4);
        assert_eq!(lines.iter().map(text_of).collect::<Vec<_>>(), vec!["abcd", "efgh", "ij"]);
        assert!(lines.iter().all(|l| l.spans.iter().all(|s| s.style == style)));
    }
}
