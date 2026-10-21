//! Streaming splitter for the group-chat `<public>…</public>` convention.
//!
//! A group agent's text stream is partly private (its own notes, internal
//! remarks) and partly public (what the room sees). Only text inside a
//! `<public>` block is posted to the shared transcript; everything else never
//! leaves the agent's private session.
//!
//! Tags can be split across arbitrary chunk boundaries — providers stream a few
//! characters at a time, so a chunk may end in `<pub` — and a block can span
//! several LLM rounds (text → tool call → more text). That rules out matching a
//! tag per chunk: this filter keeps the state, holds back any suffix that could
//! still become a tag, and emits classified deltas.

const OPEN: &str = "<public>";
const CLOSE: &str = "</public>";

/// One classified piece of the agent's text stream.
#[derive(Debug, PartialEq, Eq)]
pub enum Piece {
    /// Text outside a `<public>` block — private to this agent.
    Private(String),
    /// A delta inside a `<public>` block; the caller accumulates it.
    Public(String),
    /// The current block closed (or the stream ended while it was open). The
    /// caller posts whatever it accumulated, if anything.
    PublicEnd,
}

#[derive(Debug, Default)]
pub struct PublicFilter {
    in_public: bool,
    /// Suffix held back because it may be the beginning of the next tag.
    held: String,
}

impl PublicFilter {
    /// Feed one streamed chunk and return the pieces it completed.
    pub fn push(&mut self, chunk: &str) -> Vec<Piece> {
        if chunk.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut buf = std::mem::take(&mut self.held);
        buf.push_str(chunk);

        loop {
            let tag = if self.in_public { CLOSE } else { OPEN };
            match buf.find(tag) {
                Some(i) => {
                    self.emit_text(&buf[..i], &mut out);
                    self.in_public = !self.in_public;
                    if !self.in_public {
                        out.push(Piece::PublicEnd);
                    }
                    buf.drain(..i + tag.len());
                }
                None => {
                    // Hold back a suffix that is a prefix of the tag, so a tag
                    // split across chunks is still recognised next time.
                    let hold = partial_suffix_len(&buf, tag);
                    let cut = buf.len() - hold;
                    self.emit_text(&buf[..cut], &mut out);
                    self.held = buf[cut..].to_string();
                    return out;
                }
            }
        }
    }

    /// Flush at end of stream: release the held-back suffix and close an
    /// unterminated block. An unclosed `<public>` is still public — the model
    /// simply ran out of output.
    pub fn finish(&mut self) -> Vec<Piece> {
        let held = std::mem::take(&mut self.held);
        let mut out = Vec::new();
        self.emit_text(&held, &mut out);
        if self.in_public {
            self.in_public = false;
            out.push(Piece::PublicEnd);
        }
        out
    }

    fn emit_text(&self, text: &str, out: &mut Vec<Piece>) {
        if text.is_empty() {
            return;
        }
        out.push(if self.in_public {
            Piece::Public(text.to_string())
        } else {
            Piece::Private(text.to_string())
        });
    }
}

/// Length of the longest suffix of `buf` that is a proper prefix of `tag`.
fn partial_suffix_len(buf: &str, tag: &str) -> usize {
    let max = buf.len().min(tag.len().saturating_sub(1));
    (1..=max).rev().find(|&k| buf.ends_with(&tag[..k])).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces(filter: &mut PublicFilter, chunks: &[&str]) -> Vec<Piece> {
        let mut out = Vec::new();
        for chunk in chunks {
            out.extend(filter.push(chunk));
        }
        out
    }

    fn private(text: &str) -> Piece {
        Piece::Private(text.to_string())
    }

    fn public(text: &str) -> Piece {
        Piece::Public(text.to_string())
    }

    #[test]
    fn text_without_tags_stays_private() {
        let mut f = PublicFilter::default();
        assert_eq!(pieces(&mut f, &["hello ", "world"]), vec![private("hello "), private("world")]);
    }

    #[test]
    fn a_public_block_is_split_from_its_surroundings() {
        let mut f = PublicFilter::default();
        assert_eq!(
            pieces(&mut f, &["before <public>hi</public> after"]),
            vec![private("before "), public("hi"), Piece::PublicEnd, private(" after")]
        );
    }

    #[test]
    fn a_tag_split_across_chunks_is_still_recognised() {
        let mut f = PublicFilter::default();
        let out = pieces(
            &mut f,
            &["<pub", "lic>he", "llo</publi", "c>tail", "<publi", "c>two</public>"],
        );
        assert_eq!(
            out,
            vec![
                public("he"),
                public("llo"),
                Piece::PublicEnd,
                private("tail"),
                public("two"),
                Piece::PublicEnd,
            ]
        );
    }

    #[test]
    fn multiple_blocks_in_one_message_are_posted_separately() {
        let mut f = PublicFilter::default();
        assert_eq!(
            pieces(&mut f, &["<public>one</public> middle <public>two</public>"]),
            vec![
                public("one"),
                Piece::PublicEnd,
                private(" middle "),
                public("two"),
                Piece::PublicEnd,
            ]
        );
    }

    #[test]
    fn an_unclosed_block_is_flushed_at_finish() {
        let mut f = PublicFilter::default();
        assert_eq!(
            pieces(&mut f, &["note <public>half a sentence"]),
            vec![private("note "), public("half a sentence")]
        );
        assert_eq!(f.finish(), vec![Piece::PublicEnd]);
        // Finishing twice must not close the same block again.
        assert!(f.finish().is_empty());
    }

    #[test]
    fn lookalike_text_is_released_instead_of_being_held_forever() {
        let mut f = PublicFilter::default();
        // `<publix` can never become `<public>`; once the mismatch is known the
        // held prefix must be emitted as private text.
        assert_eq!(pieces(&mut f, &["<publix>"]), vec![private("<publix>")]);
        // A trailing `<` alone is a real partial tag and is held until the next
        // chunk tells us what it is.
        assert_eq!(pieces(&mut f, &["<"]), Vec::<Piece>::new());
        assert_eq!(pieces(&mut f, &["b>"]), vec![private("<b>")]);
    }

    #[test]
    fn an_opener_inside_a_public_block_is_literal_text() {
        let mut f = PublicFilter::default();
        assert_eq!(
            pieces(&mut f, &["<public>a <public> b</public>"]),
            vec![public("a <public> b"), Piece::PublicEnd]
        );
    }
}
