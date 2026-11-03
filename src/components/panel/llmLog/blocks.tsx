import { codeBlockClass as preClass } from "../../ui/CodeBlock";
import { ToolCallBlock } from "../../ui/ToolCallBlock";
import type { ToolCall } from "./types";

// Messages are logged as serialized genai `ChatMessage`s: content is an array of
// externally-tagged `ContentPart`s. A tool call and its response are content
// blocks — NOT the OpenAI `tool_calls` / `tool_call_id` message fields.
type ToolResponse = { call_id?: string; fn_name?: string; content?: unknown };

export type ContentBlock = {
  Text?: string;
  Binary?: { content_type?: string; source?: { Base64?: string; Url?: string } };
  ToolCall?: ToolCall;
  ToolResponse?: ToolResponse;
  ThoughtSignature?: string;
  ReasoningContent?: string;
};

/** The displayable image URL of a content block, or null if it isn't an image. */
function blockImageUrl(b: ContentBlock): string | null {
  if (b?.Binary) {
    const { content_type, source } = b.Binary;
    if (!content_type?.startsWith("image/")) return null;
    if (source?.Url) return source.Url;
    if (source?.Base64) return `data:${content_type};base64,${source.Base64}`;
    return null;
  }
  return null;
}

/** The plain text of a content block, or null if it carries none. */
function blockText(b: ContentBlock): string | null {
  if (typeof b?.Text === "string") return b.Text;
  if (typeof b?.ReasoningContent === "string") return b.ReasoningContent;
  return null;
}

/** A message body as blocks; empty text is dropped so a message that renders
 *  nothing can be skipped entirely. */
export function blocksOf(content: unknown): ContentBlock[] {
  const raw: ContentBlock[] =
    content == null ? []
    : typeof content === "string" ? [{ Text: content }]
    : Array.isArray(content) ? (content as ContentBlock[])
    : [{ Text: JSON.stringify(content, null, 2) }];
  return raw.filter((b) => blockText(b)?.trim() !== "");
}

/** One content block. Tool calls and their responses go through the same
 *  `ToolCallBlock` the chat thread uses — this view differs only in starting
 *  expanded, since it is a full replay of what went on the wire. */
export function Block({ block, onZoom, zoomTitle }: { block: ContentBlock; onZoom: (src: string) => void; zoomTitle: string }) {
  const url = blockImageUrl(block);
  if (url) {
    return (
      <img
        src={url}
        alt="attachment"
        onClick={() => onZoom(url)}
        title={zoomTitle}
        className="max-h-[300px] max-w-full cursor-zoom-in rounded-lg border border-line object-contain"
      />
    );
  }
  if (block.ToolCall) {
    const { fn_name, fn_arguments, call_id } = block.ToolCall;
    return <ToolCallBlock name={fn_name ?? "unknown"} arguments={fn_arguments ?? {}} callId={call_id} defaultExpanded />;
  }
  if (block.ToolResponse) {
    const { fn_name, content, call_id } = block.ToolResponse;
    return <ToolCallBlock name={fn_name ?? "unknown"} result={content ?? ""} callId={call_id} defaultExpanded />;
  }
  const text = blockText(block);
  if (text !== null) return <pre className={preClass}>{text}</pre>;
  // Opaque provider token replayed on the next turn — long, base64-ish and
  // unreadable, so don't let it drown the message.
  if (typeof block.ThoughtSignature === "string") return <pre className={preClass}>[ThoughtSignature]</pre>;
  return <pre className={preClass}>{JSON.stringify(block, null, 2)}</pre>;
}