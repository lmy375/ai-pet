import { useEffect, useRef } from "react";
import type { ReactNode } from "react";
import type { ChatItem, ToolCall } from "../hooks/useChat";
import { MessageBubble } from "./ui/MessageBubble";
import { ToolCallBlock } from "./panel/ToolCallBlock";
import { formatHm } from "../utils/format";

interface Props {
  items: ChatItem[];
  currentToolCalls: ToolCall[];
  streaming: string; // in-progress assistant text (empty when idle)
  loading: boolean;
  /** Extra classes for the scroll container (controls bg/padding/position). */
  className?: string;
  /** Shown when there are no messages. If omitted, the whole thread renders nothing when empty. */
  emptyHint?: ReactNode;
}

const FIVE_MIN = 5 * 60 * 1000;

function renderItem(item: ChatItem) {
  switch (item.type) {
    case "user":
      return <MessageBubble role="user">{item.content}</MessageBubble>;
    case "assistant":
      return item.content.trim() ? <MessageBubble role="assistant">{item.content}</MessageBubble> : null;
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
    default:
      return null;
  }
}

/** Shared chat message list: renders items (incl. tool calls), live tool calls,
 *  streaming response and timestamps. Identical logic for the pet and panel
 *  windows — only `className` differs. */
export function ChatThread({ items, currentToolCalls, streaming, loading, className = "", emptyHint }: Props) {
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [items, streaming, currentToolCalls, loading]);

  const showStreaming = streaming.trim().length > 0;
  const isEmpty = items.length === 0 && !showStreaming && !loading;
  if (isEmpty && !emptyHint) return null;

  return (
    <div className={`flex flex-col gap-2 overflow-y-auto ${className}`}>
      {isEmpty && emptyHint && <div className="mt-10 text-center text-[14px] text-slate-400">{emptyHint}</div>}

      {items.map((item, i) => {
        const prev = items[i - 1];
        const showTime =
          item.ts !== undefined && (i === 0 || prev?.ts === undefined || item.ts - prev.ts > FIVE_MIN);
        return (
          <div key={i} className="flex flex-col gap-2">
            {showTime && (
              <div className="self-center px-2 py-0.5 text-[11px] text-slate-400">{formatHm(item.ts!)}</div>
            )}
            {renderItem(item)}
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
          {streaming}
          <span className="animate-blink">▌</span>
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
