//! Collapsing the phantom tool calls one gateway reproduces in genai's
//! OpenAI streamer. See [`dedup_tool_calls`].

use genai::chat::{ChatMessage, ContentPart, MessageContent, ToolCall};

/// Collapse the phantom tool calls genai's OpenAI streamer manufactures when a
/// gateway numbers `tool_calls[].index` by Responses-API output position
/// instead of 0-based per tool call.
///
/// Reproduced against `litellm.1cobo.com` (`gpt-5.6-*` via a Responses→chat
/// bridge): whenever a reasoning or text item precedes the call, the stream's
/// first delta arrives as `{index: 1, id, name, arguments: ""}` — the index is
/// the output slot, and the reasoning item took slot 0. genai keys its
/// accumulator by that index and fills the gap with
/// `resize(index + 1, first_chunk.clone())`, so slot 0 keeps a copy of the
/// first chunk (same call_id, same name, empty arguments) while the real
/// arguments accumulate into slot 1. The turn then reads as
/// `[{id, ""}, {id, {...}}]`; every copy gets executed with empty arguments,
/// fails, and a duplicate call_id lands in the stored history.
///
/// A call_id is unique within one assistant turn, so the collapse is safe:
/// for each id keep the most complete entry — parsed JSON beats an
/// unparseable string (a partial first chunk such as `{"co` that genai could
/// not parse), and a longer string beats a shorter one — in first-seen order.
/// A genuinely argument-less call is a single entry for its id and is left
/// alone. Nothing here depends on how the gateway numbered the slots.
pub(super) fn dedup_tool_calls(calls: Vec<ToolCall>) -> Vec<ToolCall> {
    // Higher is more complete: parsed JSON, then raw string by length (a
    // placeholder is the empty string, a partial first chunk is a short one).
    let completeness = |tc: &ToolCall| match tc.fn_arguments.as_str() {
        Some(raw) => (0, raw.len()),
        None => (1, 0),
    };
    let mut out: Vec<ToolCall> = Vec::with_capacity(calls.len());
    for call in calls {
        match out.iter_mut().find(|kept| kept.call_id == call.call_id) {
            None => out.push(call),
            Some(kept) => {
                if completeness(&call) > completeness(kept) {
                    // The clone may carry the turn's thought signatures (genai
                    // attaches them to whatever sits first); keep them with the
                    // surviving call.
                    let signatures = kept.thought_signatures.take();
                    *kept = call;
                    if kept.thought_signatures.is_none() {
                        kept.thought_signatures = signatures;
                    }
                }
            }
        }
    }
    out
}

/// Apply [`dedup_tool_calls`] to the assistant turn genai captured, which is
/// what gets replayed into history. Only the `ToolCall` parts are touched;
/// thought signatures and text keep their place ahead of them (genai's own
/// ordering policy puts tool calls last).
pub(super) fn dedup_assistant_turn(turn: ChatMessage) -> ChatMessage {
    let ChatMessage { role, content, options } = turn;
    let mut parts: Vec<ContentPart> = Vec::new();
    let mut calls: Vec<ToolCall> = Vec::new();
    for part in content.into_parts() {
        match part {
            ContentPart::ToolCall(tc) => calls.push(tc),
            other => parts.push(other),
        }
    }
    parts.extend(dedup_tool_calls(calls).into_iter().map(ContentPart::ToolCall));
    ChatMessage { role, content: MessageContent::from_parts(parts), options }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(id: &str, args: serde_json::Value) -> ToolCall {
        ToolCall { call_id: id.into(), fn_name: "bash".into(), fn_arguments: args, thought_signatures: None }
    }

    /// The exact shape genai produced from a gateway stream whose first tool
    /// call carried `index: 1`: a phantom copy with empty arguments ahead of
    /// the real call, followed by a second, healthy call. Only the phantom may
    /// go — the second call must survive in order.
    #[test]
    fn gateway_index_gap_clones_collapse_to_one_call_per_id() {
        let full = serde_json::json!({"command": "pwd && ls -la"});
        let calls = dedup_tool_calls(vec![
            call("call_a", serde_json::json!("")),
            call("call_a", full.clone()),
            call("call_b", serde_json::json!({"command": "find ."})),
        ]);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].call_id, "call_a");
        assert_eq!(calls[0].fn_arguments, full);
        assert_eq!(calls[1].call_id, "call_b");

        // A call preceded by several output items (reasoning + text) shows up
        // with as many clones — round 2 of the reproducing session had six.
        let mut many: Vec<ToolCall> = (0..6).map(|_| call("call_c", serde_json::json!(""))).collect();
        many.push(call("call_c", full.clone()));
        let calls = dedup_tool_calls(many);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].fn_arguments, full);
    }

    /// A provider whose first chunk already carries part of the arguments
    /// leaves a clone holding that fragment, not an empty string. The parsed
    /// call must still win over the fragment regardless of which came first.
    #[test]
    fn a_partial_first_chunk_clone_loses_to_the_parsed_call() {
        let full = serde_json::json!({"command": "ls"});
        let calls = dedup_tool_calls(vec![
            call("call_a", serde_json::json!("{\"co")),
            call("call_a", full.clone()),
        ]);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].fn_arguments, full);
        // And the other way round: a full call first, fragment after.
        let calls = dedup_tool_calls(vec![
            call("call_a", full.clone()),
            call("call_a", serde_json::json!("{\"co")),
        ]);
        assert_eq!(calls[0].fn_arguments, full);
    }

    /// An argument-less call is a single entry for its id, and must not be
    /// mistaken for a placeholder: dropping it would swallow a real call.
    #[test]
    fn a_lone_empty_argument_call_is_kept() {
        let calls = dedup_tool_calls(vec![call("call_a", serde_json::json!(""))]);
        assert_eq!(calls.len(), 1);
    }

    /// The captured assistant turn is what history replays, so it has to be
    /// collapsed the same way — with the text and thought signature genai put
    /// ahead of the calls left exactly where they were.
    #[test]
    fn assistant_turn_loses_the_clone_but_keeps_its_other_parts() {
        let mut phantom = call("call_a", serde_json::json!(""));
        phantom.thought_signatures = Some(vec!["sig".into()]);
        let turn = ChatMessage::assistant(MessageContent::from_parts(vec![
            ContentPart::ThoughtSignature("sig".into()),
            ContentPart::from_text("先看一下目录"),
            ContentPart::ToolCall(phantom),
            ContentPart::ToolCall(call("call_a", serde_json::json!({"command": "ls"}))),
        ]));
        let turn = dedup_assistant_turn(turn);
        let parts = turn.content.parts();
        assert_eq!(parts.len(), 3);
        assert!(matches!(parts[0], ContentPart::ThoughtSignature(_)));
        assert_eq!(turn.content.texts(), vec!["先看一下目录"]);
        let calls = turn.content.tool_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].fn_arguments, serde_json::json!({"command": "ls"}));
        // The signature genai hung on the phantom moved to the surviving call.
        assert_eq!(calls[0].thought_signatures.as_deref(), Some(&["sig".to_string()][..]));
    }
}
