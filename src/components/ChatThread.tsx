import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import type { ChatItem, ToolCall } from "../hooks/useChat";
import { MessageBubble } from "./ui/MessageBubble";
import { ReasoningBlock } from "./ui/ReasoningBlock";
import { CodeBlock } from "./ui/CodeBlock";
import { ToolCallBlock } from "./panel/ToolCallBlock";
import { ChevronRight, CheckIcon } from "./Icons";
import { formatHm, formatJson } from "../utils/format";

interface Props {
  items: ChatItem[];
  currentToolCalls: ToolCall[];
  streaming: string; // in-progress assistant text (empty when idle)
  streamingReasoning?: string; // in-progress chain-of-thought (empty when idle)
  loading: boolean;
  /** Extra classes for the scroll container (controls bg/padding/position). */
  className?: string;
  /** Shown when there are no messages. If omitted, the whole thread renders nothing when empty. */
  emptyHint?: ReactNode;
  /** When true, each row shows a checkbox and clicking it toggles selection. */
  selectionMode?: boolean;
  /** Stable item ids currently selected (never array indices — they shift). */
  selectedKeys?: Set<string>;
  /** Toggle selection for the item with id `id`. */
  onToggleSelect?: (id: string) => void;
}

const FIVE_MIN = 5 * 60 * 1000;

/** The "后台任务完成：XXX" system line. Click to expand the task's full result. */
function NotificationItem({ content, detail }: { content: string; detail?: string }) {
  const [expanded, setExpanded] = useState(false);
  const hasDetail = !!detail;
  return (
    <div className="flex w-full max-w-[90%] flex-col items-center self-center">
      <button
        type="button"
        disabled={!hasDetail}
        onClick={() => setExpanded((e) => !e)}
        className={`flex items-center gap-1 rounded-full bg-slate-100 px-3 py-1 text-[12px] text-slate-500 ${
          hasDetail ? "hover:bg-slate-200" : "cursor-default"
        }`}
      >
        {hasDetail && (
          <ChevronRight className={`h-3 w-3 shrink-0 transition-transform ${expanded ? "rotate-90" : ""}`} />
        )}
        <span>{content}</span>
      </button>
      {expanded && hasDetail && (
        <CodeBlock className="mt-1 w-full">{formatJson(detail!)}</CodeBlock>
      )}
    </div>
  );
}

function renderItem(item: ChatItem) {
  switch (item.type) {
    case "user":
      return <MessageBubble role="user" images={item.images}>{item.content}</MessageBubble>;
    case "assistant": {
      // Tool-produced images (e.g. screenshots) arrive as assistant items with
      // empty text — still render the bubble so the image shows. A reasoning-only
      // item (model thought, then called a tool with no preamble) still renders
      // so its thinking is viewable.
      const hasReasoning = !!item.reasoning?.trim();
      if (!item.content.trim() && !item.images?.length && !hasReasoning) return null;
      return (
        <MessageBubble role="assistant" images={item.images}>
          {hasReasoning && <ReasoningBlock text={item.reasoning!} />}
          {item.content}
        </MessageBubble>
      );
    }
    case "tool":
      return (
        <div className="max-w-[85%]">
          {item.toolCalls?.map((tc, j) => (
            <ToolCallBlock key={j} name={tc.name} arguments={tc.arguments} result={tc.result} />
          ))}
        </div>
      );
    case "error":
      return <MessageBubble role="assistant" error>{item.content}</MessageBubble>;
    case "notification":
      // A subtle system line (not a chat bubble) marking an auto-resumed turn;
      // expandable to view the task's full result.
      return <NotificationItem content={item.content} detail={item.detail} />;
    default:
      return null;
  }
}

/** Shared chat message list: renders items (incl. tool calls), live tool calls,
 *  streaming response and timestamps. Identical logic for the pet and panel
 *  windows — only `className` differs. */
export function ChatThread({
  items,
  currentToolCalls,
  streaming,
  streamingReasoning = "",
  loading,
  className = "",
  emptyHint,
  selectionMode = false,
  selectedKeys,
  onToggleSelect,
}: Props) {
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [items, streaming, streamingReasoning, currentToolCalls, loading]);

  const showStreaming = streaming.trim().length > 0 || streamingReasoning.trim().length > 0;
  const isEmpty = items.length === 0 && !showStreaming && !loading;
  if (isEmpty && !emptyHint) return null;

  return (
    <div className={`flex flex-col gap-2 overflow-y-auto ${className}`}>
      {isEmpty && emptyHint && <div className="mt-10 text-center text-[14px] text-slate-400">{emptyHint}</div>}

      {items.map((item, i) => {
        const prev = items[i - 1];
        const showTime =
          item.ts !== undefined && (i === 0 || prev?.ts === undefined || item.ts - prev.ts > FIVE_MIN);
        const selected = item.id ? (selectedKeys?.has(item.id) ?? false) : false;
        return (
          <div key={item.id ?? i} className="flex flex-col gap-2">
            {showTime && (
              <div className="self-center px-2 py-0.5 text-[11px] text-slate-400">{formatHm(item.ts!)}</div>
            )}
            {selectionMode ? (
              <button
                type="button"
                onClick={() => item.id && onToggleSelect?.(item.id)}
                className={`flex w-full items-start gap-2 rounded-lg p-1.5 text-left transition-colors ${
                  selected ? "bg-sky-50 ring-1 ring-accent" : "hover:bg-slate-50"
                }`}
              >
                <span
                  className={`mt-1 flex h-4 w-4 shrink-0 items-center justify-center rounded border ${
                    selected ? "border-accent bg-accent text-white" : "border-slate-300 bg-white"
                  }`}
                >
                  {selected && <CheckIcon className="h-3 w-3" />}
                </span>
                {/* Disable inner pointer events so the row click owns the toggle. */}
                <div className="min-w-0 flex-1 pointer-events-none">{renderItem(item)}</div>
              </button>
            ) : (
              renderItem(item)
            )}
          </div>
        );
      })}

      {currentToolCalls.length > 0 && (
        <div className="max-w-[85%]">
          {currentToolCalls.map((tc, j) => (
            <ToolCallBlock key={j} name={tc.name} arguments={tc.arguments} result={tc.result} isRunning={tc.isRunning} />
          ))}
        </div>
      )}

      {showStreaming && (
        <MessageBubble role="assistant">
          {streamingReasoning.trim() && <ReasoningBlock text={streamingReasoning} streaming />}
          {streaming}
          {streaming.trim() && <span className="animate-blink">▌</span>}
        </MessageBubble>
      )}

      {loading && !showStreaming && currentToolCalls.length === 0 && (
        <div className="flex gap-1 self-start rounded-2xl bg-slate-200 px-3 py-2.5">
          <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-slate-400 [animation-delay:-0.2s]" />
          <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-slate-400 [animation-delay:-0.1s]" />
          <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-slate-400" />
        </div>
      )}

      <div ref={endRef} />
    </div>
  );
}
